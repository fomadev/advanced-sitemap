# Snapshot, détection de changements et diff

Statut : proposition. Voir les invariants dans le [README](README.md).

## 1. Ce que le moteur garantit déjà et dont tout dépend

- **Sortie déterministe** (ADR-0006) : le bloc `<url>` d'une entrée est
  toujours encodé avec les mêmes octets, quel que soit le reste du fichier.
  `encode_url` est donc une forme canonique de l'entrée, extensions
  comprises (alternates, images, vidéos, news).
- **Manifest sans horodatage** : il liste les artefacts publiés, leurs
  tailles et leurs nombres d'entrées. Il sert déjà à `STALE_ARTIFACT`.
- **Empreinte de déduplication en mémoire** : `fingerprint()` utilise
  `DefaultHasher` (SipHash). Rust ne garantit pas cet algorithme d'une
  version à l'autre : elle **ne doit jamais être persistée** (I6). Le
  snapshot utilise un hachage distinct, figé (ADR-0011).

## 2. Empreintes

| Empreinte | Calcul | Usage |
|---|---|---|
| Clé d'URL | `digest128(loc)` sur la forme sérialisée WHATWG (celle qui est écrite) | Identifier une URL entre deux exécutions |
| Contenu d'entrée | `digest128(encode_url(entry))` | Savoir si une URL a changé, quel que soit le champ |
| Partition | `digest128(octets du fichier)`, calculé en flux pendant l'écriture | Savoir si un fichier doit être republié |
| Configuration | `digest128` d'une forme canonique des réglages qui influencent les octets | Invalider le snapshot (I7) |

`digest128` est BLAKE3 tronqué à 128 bits (ADR-0011). Le routage des
partitions reste FNV-1a 64 (ADR-0005), indépendant de ces empreintes.

Le contenu d'entrée couvre `lastmod` : une URL dont seul `lastmod` change
est une modification. C'est voulu : `lastmod` est publié, donc le fichier
change.

La configuration canonique contient : version du format de snapshot,
version d'encodage du moteur, `fileStem`, limites (`maxUrls`,
`maxBytes`), stratégie et nombre de partitions, espaces de noms déclarés,
`publicBaseUrl` (il apparaît dans l'index), mode d'index. Elle exclut ce
qui ne change pas les octets publiés : profil de validation, politique
d'invalides, nombre maximal de diagnostics.

## 3. Snapshot

### 3.1 Contenu

Deux niveaux, pour que la phase A ne paie pas le coût de la phase B :

```
snapshot
├── en-tête : version du format, empreinte de configuration, nombre d'URL
├── niveau partition (phase A) — toujours présent
│     pour chaque artefact publié : nom, type, empreinte, octets, entrées
│     (sitemaps, index, robots.txt)
└── niveau URL (phase B) — optionnel
      pour chaque partition : suite triée de (clé d'URL, contenu d'entrée)
```

Le niveau partition remplace et complète le manifest : il contient les
mêmes informations plus les empreintes. Le manifest JSON reste publié pour
sa lisibilité et pour la compatibilité.

### 3.2 Format

- **Niveau partition** : JSON, quelques Ko, lisible et stable.
- **Niveau URL** : binaire, 32 octets par URL (16 + 16), une section par
  partition, triée par clé. Taille : 32 Mo à 1 M d'URL, 1,6 Go à 50 M.
  Le tri permet une comparaison par fusion en flux, en mémoire constante
  (I8), section par section.
- Aucun horodatage, aucune donnée dépendant de la machine (I2).
- En-tête versionné : un snapshot d'une version inconnue est ignoré et le
  planner régénère tout, en le disant (I7).

### 3.3 Écriture et emplacement

- Écrit dans la zone temporaire, publié **en dernier**, après les
  partitions, l'index, robots.txt et le manifest (I3). Une panne avant
  cette étape laisse l'ancien snapshot, qui décrit encore des fichiers
  publiés : les fichiers republiés entre-temps seront simplement
  recomparés et, s'ils sont identiques, ignorés (I4).
- Emplacement proposé : un dossier d'état séparé du dossier public
  (question ouverte 2 du README).

## 4. Détection de changements

### 4.1 Avec une source complète (phases A et B)

Le moteur lit toute la source comme aujourd'hui. Pour chaque partition :

1. Les entrées sont encodées et routées comme aujourd'hui.
2. L'empreinte du fichier est calculée pendant l'écriture.
3. À la fermeture, elle est comparée à celle du snapshot :
   - identique : le fichier temporaire est abandonné, rien n'est publié ;
   - différente ou nouvelle : le fichier est publié.
4. L'index est régénéré en mémoire (il est petit). Il n'est publié que si
   ses octets changent, ce qui n'arrive que si un `lastmod` de partition ou
   la liste des partitions change.

Avec le niveau URL (phase B), on obtient en plus, pendant le même passage,
l'ensemble exact des URL ajoutées, retirées et modifiées.

### 4.2 Avec un flux de changements (phase D)

Voir [ADR-0013](../adr/0013-published-files-as-state.md) : seules les
partitions dont une clé change sont relues depuis les fichiers publiés,
modifiées et réencodées.

## 5. Diff Engine

Trois usages, une seule sortie (`ChangeSet`) :

| Usage | Entrées | Coût mémoire |
|---|---|---|
| Après `generate` (phase B) | Snapshot précédent et nouveau | Constant (fusion de sections triées) |
| `advanced-sitemap diff A B` | Deux dossiers de sitemaps publiés, relus par `sitemap-reader` | O(URL) en mémoire ; tri externe au-delà d'un seuil |
| Planification | Snapshot et flux de changements | O(changements) |

`ChangeSet` : nombre et liste (bornée, comme les diagnostics) des URL
ajoutées, retirées et modifiées, partitions touchées, et pour `diff A B`
le détail des champs changés (possible parce que les fichiers publiés
contiennent toutes les données).

Différence avec l'outil TypeScript actuel (`utils/diff.ts`) : il compare
le **nombre** d'images et de vidéos, donc une image remplacée par une autre
passe inaperçue. L'empreinte de contenu détecte tout changement d'octet.

## 6. Ce que ce composant ne fait pas

- Il ne décide pas quoi publier : c'est le planner.
- Il ne lit pas l'horloge : « récent » ou « ancien » n'a de sens qu'avec un
  `referenceTime` fourni.
- Il ne supprime rien (I5).
