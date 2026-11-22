# Limites connues

État au 2026-11-22, après le lot F. Chaque limite est documentée,
mesurée quand c'est possible, et suivie dans le
[registre des risques](risk-register.md) (colonne ID).

## Publication et durabilité

| Limite | Effet | Contournement | ID |
|---|---|---|---|
| La publication d'un ensemble de fichiers n'est pas atomique : chaque fichier est remplacé atomiquement, l'un après l'autre | Pendant quelques millisecondes, ou après un arrêt brutal au milieu, des fichiers anciens et nouveaux coexistent (chacun complet). Une URL déplacée peut être dans deux fichiers ou dans aucun | `doctor` le signale (`files` en erreur) ; l'exécution suivante répare l'ensemble. Une publication par révisions et pointeur est à décider | R2 |
| Résistance à une coupure électrique non démontrée par un banc | Les garanties reposent sur `fsync` du fichier avant `rename`, puis `fsync` du dossier (Unix). Un disque qui ignore `fsync` peut perdre la dernière publication | Les arrêts brutaux du processus sont testés (12 points d'arrêt, aucun fichier tronqué) ; garder les sauvegardes habituelles | R4 |
| Sous Windows, `fsync` d'un dossier n'existe pas | La durabilité des renommages dépend du système de fichiers (NTFS journalise les métadonnées) | — | R3 |
| Sous Windows, un fichier ouvert par un autre programme sans partage de suppression ne peut pas être remplacé | Le moteur réessaie pendant environ 2,3 s, puis s'arrête avec une erreur qui nomme le fichier et dit combien ont déjà été remplacés | Relancer une fois le fichier libéré ; `doctor` montre l'état | R8 |
| Le fichier verrou ne se lit pas sous Windows pendant une exécution | Une copie du dossier de sortie pendant une exécution échoue sur ce fichier | Copier après l'exécution, exclure `*.lock` et `.advanced-sitemap*` des déploiements | R20 |
| Les autres programmes ne respectent pas le verrou | Le verrou exclut seulement les processus `advanced-sitemap` | Ne pas écrire dans le dossier de sortie par un autre moyen | — |
| Les fichiers qui ne sont plus produits ne sont jamais supprimés | Ils restent publiés (`STALE_ARTIFACT` les signale) | Les supprimer à la main après vérification | — |

## Volumes et ressources

| Limite | Valeur | ID |
|---|---|---|
| URL par fichier, octets par fichier | 50 000 et 50 Mio (protocole) | — |
| Enfants d'un index, sans index d'index | 50 000, soit 2,5 milliards d'URL | R21 |
| Mémoire de la déduplication | 17 à 34 octets par URL distincte (415 Mio mesurés à 10 M) ; `--no-dedupe` la supprime | — |
| Réciprocité hreflang | 110 octets par lien | — |
| Flux d'`apply` tenu en mémoire | 806 octets par URL changée ; 500 000 par défaut (`state.maxFeedOperations`) | R9 |
| Plus grand volume mesuré | 10 M d'URL ; 50 M non mesuré (disque de la machine de référence) | R15 |
| Une seule unité de calcul | La validation domine le temps CPU | R24 |

Toutes les bornes sont listées dans [docs/concepts/limits.md](../concepts/limits.md).

## Planner et estimations

- Le modèle de coût est calibré sur une seule machine (portable de 2015,
  disque lent). Sur un autre matériel, seule la comparaison entre modes
  est fiable, pas les durées absolues (R23).
- `explain` estime à partir d'un échantillon ; les réserves en octets et
  en news sont prudentes.
- Il n'y a pas de repli automatique d'`apply` vers une génération
  complète : un flux refusé doit être relancé en `generate` (choix voulu,
  pour ne jamais ignorer un flux).

## Plateformes

- Testées en CI : Linux x86_64 et ARM64, Windows x86_64 et ARM64, macOS
  ARM64, avec la même sortie octet par octet sur les cinq.
- macOS x86_64 : binaire construit par compilation croisée, non exécuté
  en CI.
- Linux musl : binaire construit et vérifié contre les fixtures ; la suite
  de tests complète ne tourne qu'avec glibc.
- Non supportés : WebAssembly, systèmes 32 bits (non testés).

## API et compatibilité

- Aucune version publiée : la comparaison automatique d'API
  (`cargo semver-checks`) attend la première, qui servira de référence.
- Changer de version du moteur invalide le snapshot : le premier
  `generate` réécrit tout, et `apply` est refusé jusque-là
  ([semver-policy.md](semver-policy.md), section 6).
- La sortie humaine de la CLI et le texte des messages ne sont pas un
  contrat.

## Hors périmètre de cette version

Lecture de sitemaps distants (`inspect` lit des fichiers locaux), envoi
aux moteurs de recherche, bindings Node, Python et PHP, commande
`benchmark`, stockage objet (S3...).
