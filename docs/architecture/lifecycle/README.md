# Lifecycle Engine — conception

Statut : **proposition à valider**. Rien de ce qui est décrit ici n'est
implémenté, sauf mention contraire (« existe déjà »). Ce document définit
les composants, leurs invariants et leurs interactions avec le moteur
actuel avant toute implémentation.

Documents détaillés :

- [Snapshot, détection de changements et diff](snapshot-and-diff.md)
- [Génération incrémentale et partitionnement stable](incremental-generation.md)
- [Planner et Explain Engine](planner-and-explain.md)
- ADR : [0010](../adr/0010-lifecycle-invariants.md),
  [0011](../adr/0011-stable-digests.md),
  [0012](../adr/0012-jump-consistent-hash.md),
  [0013](../adr/0013-published-files-as-state.md)

## 1. Problème

Aujourd'hui chaque `generate` relit toute la source, réencode et
**réécrit tous les fichiers**. Les mesures (voir
[performance](../../performance/README.md)) montrent où va le temps :

| 5 M d'URL | Durée | Ce qui change |
|---|---|---|
| `validate` (même pipeline, store nul) | 23,6 s | lecture, validation, encodage |
| `generate` | 80,4 s | + écriture de 848 Mo et un `fsync` par fichier |

Sur cette machine, **écrire coûte plus que tout le reste réuni**. Et pour
les moteurs de recherche, un fichier réécrit à l'identique n'est pas un
problème, mais un `lastmod` d'index qui bouge sans raison leur fait
retélécharger des fichiers inutiles.

Objectifs, par ordre de priorité :

1. Ne réécrire que les fichiers dont le contenu change, avec un résultat
   **identique octet par octet** à une régénération complète.
2. Savoir ce qui a changé (URL ajoutées, retirées, modifiées) pour les
   rapports, IndexNow et la planification.
3. Pouvoir appliquer un flux de changements sans relire toute la source.
4. Expliquer chaque décision avant de l'exécuter.

## 2. Composants et interactions

```
                  source (flux complet ou flux de changements)
                                   │
                                   ▼
 ┌───────────────┐   lit   ┌───────────────────┐  plan   ┌──────────────────┐
 │ Snapshot      │────────▶│ Generation        │────────▶│ Explain Engine   │
 │ Engine        │         │ Planner           │         │ (lecture seule)  │
 └──────┬────────┘         └────────┬──────────┘         └──────────────────┘
        ▲                           │ plan exécutable
        │ écrit en dernier          ▼
        │                 ┌───────────────────┐  empreintes  ┌──────────────┐
        │                 │ moteur actuel :   │─────────────▶│ Change       │
        │                 │ validation,       │              │ Detection    │
        │                 │ encodage, routage │              └──────┬───────┘
        │                 │ (Partitioner)     │                     │
        │                 └────────┬──────────┘                     ▼
        │                          │ partitions modifiées     ┌──────────────┐
        │                          ▼                          │ Diff Engine  │
        │                 ┌───────────────────┐               └──────────────┘
        └─────────────────│ publication en    │
                          │ zone temporaire   │  (existe déjà, ADR-0007)
                          └───────────────────┘
```

| Composant | Rôle | Réutilise |
|---|---|---|
| Snapshot Engine | État persistant minimal de la dernière publication | Store, staging (ADR-0007) |
| Change Detection | Empreinte de contenu de chaque entrée et de chaque partition | `encode_url` (sortie déterministe, ADR-0006) |
| Diff Engine | Ajouts, retraits, modifications entre deux états | Snapshot, `sitemap-reader` |
| Incremental Generation | Ne réécrire que les partitions touchées | `Partitioner`, publication, manifest |
| Stable Partitioning | Une URL garde sa partition quand le site grandit | FNV-1a 64 (ADR-0005), nouvelle stratégie (ADR-0012) |
| Generation Planner | Choisir complet ou incrémental, et quelles partitions | `plan.rs` (existe déjà pour `explain`) |
| Explain Engine | Montrer le plan, avec la nature de chaque chiffre | `Quantity` (existe déjà) |

## 3. Invariants

Ils sont la définition de « correct ». Chaque invariant a un test prévu.

| # | Invariant | Test |
|---|---|---|
| I1 | **Équivalence** : pour une même entrée finale et une même configuration, une génération incrémentale publie exactement les mêmes octets qu'une génération complète | Oracle différentiel : séquences aléatoires de changements, comparaison octet par octet avec une régénération complète |
| I2 | **Déterminisme** : mêmes entrées, mêmes octets, snapshot compris (pas d'horloge, pas d'ordre de table de hachage) | Deux exécutions, comparaison des fichiers et du snapshot |
| I3 | **Le snapshot ne ment jamais** : il ne décrit que des fichiers effectivement publiés. Il est publié en dernier | Injection de pannes entre chaque publication, puis exécution suivante |
| I4 | **Reprise idempotente** : après une panne à n'importe quelle étape, l'exécution suivante converge vers l'état de I1 | Même harnais d'injection de pannes |
| I5 | **Aucune suppression automatique** ; les fichiers qui ne sont plus produits sont signalés (`STALE_ARTIFACT`, existe déjà) | Test existant étendu |
| I6 | **Empreintes persistées stables** : tout hachage écrit sur disque est figé par des vecteurs de test et ne dépend ni de la version de Rust ni de la plateforme | Vecteurs de test ; CI sur 3 OS |
| I7 | **Invalidation explicite** : si la configuration qui influence les octets change (limites, espaces de noms, stratégie, nombre de partitions, nom de fichier, version du format), le planner passe en régénération complète et le dit | Test par paramètre |
| I8 | **Mémoire bornée** : la comparaison avec le snapshot ne charge jamais tout le snapshot si sa taille dépasse un seuil configuré | Test avec un seuil faible |
| I9 | **Explain dit vrai** : `explain` et `generate` utilisent le même plan ; après `generate`, le rapport indique les mêmes partitions que celles annoncées | Test qui compare les deux |
| I10 | **Diagnostics inchangés** : le mode incrémental produit les mêmes diagnostics de validation qu'une exécution complète sur la même entrée | Oracle différentiel (I1) appliqué au rapport |

Contrainte héritée : l'ordre de sortie dépend de l'ordre d'entrée dans la
stratégie `sequential`. I1 n'y est donc garanti que si la source fournit
ses enregistrements dans un ordre stable (voir
[génération incrémentale](incremental-generation.md), section 3).

## 4. Phases proposées

Chaque phase est utile seule, mesurée, et protégée par l'oracle I1.

| Phase | Contenu | Gain attendu | Mesure de validation |
|---|---|---|---|
| A (**faite**, voir ci-dessous) | Ne publier que les fichiers dont les octets changent, par comparaison avec le fichier publié | Moins d'écritures disque et un `lastmod` d'index stable | `generate` relancé sans changement : 0 fichier publié ; durée égale à `validate` |
| B (**faite**) | Snapshot par URL (clé et contenu), Diff Engine, rapport de changements | Liste exacte des ajouts, retraits et modifications | Comparaison avec les changements connus du test |
| C (**faite**) | Stratégie `consistent-hash` (ADR-0012) et planner qui chiffre les déplacements | Agrandir le site sans déplacer la plupart des URL | Fraction d'URL déplacées mesurée contre 1/(N+1) |
| D | Flux de changements (ajouts, mises à jour, suppressions) appliqués sur les fichiers publiés (ADR-0013) | Plus besoin de relire toute la source | Oracle I1 entre application du flux et régénération complète |

La phase A ne demande aucun nouveau format d'entrée et ne change aucun
contrat existant.

### Phase A : ce qui a été fait (2026-11-22)

- `output.publish = "changed"` (ou `generate --skip-unchanged`).
- Implémentation plus simple que prévu : pas de snapshot ni d'empreinte
  persistée. Le store compare les octets produits au fichier publié de même
  nom pendant l'écriture et n'écrit rien tant qu'ils sont identiques.
- Le `fsync` a été déplacé juste avant le renommage (même garantie).
- Mesures (5 M d'URL, aucun changement) : 22,4 à 22,8 s contre 44 à 123 s
  pour tout republier, soit le coût de `validate` (23,6 s). Une URL modifiée
  sur 1 M : 5,8 s, un seul sitemap et le manifest republiés.
- Tests : oracle d'équivalence (I1) sur 480 étapes aléatoires en mémoire et
  50 sur disque, pannes injectées à 8 points de la publication (I3, I4),
  test CLI qui vérifie que la date de modification des fichiers identiques
  ne change pas.

### Phase B : ce qui a été fait (2026-11-22)

- Crate `sitemap-snapshot` : format binaire v1 trié, compressé par préfixe,
  avec un compteur et une somme BLAKE3 en fin de fichier ; tri externe
  sous budget mémoire ; diff en mémoire constante.
- `state.directory` (ou `--state-dir`) active le snapshot et la section
  `changes` du rapport. Désactivé par défaut. Le snapshot est remplacé
  seulement après une publication réussie (I3) ; `validate` ne l'écrit pas.
- Emplacement : choisi explicitement par l'utilisateur, aucun défaut, ce qui
  laisse ouverte la question 2 ci-dessous sans risque de publier l'état.
- Tests : rapport de changements comparé aux changements connus sur 40
  étapes aléatoires, mode à blanc, exécution échouée, snapshot endommagé,
  changement de réglages.
- Mesures : voir `docs/performance/README.md`. Coût réel : +4 à +5,5 s à
  1 M d'URL et +17 s à 5 M sur cette machine, mémoire bornée.
- `advanced-sitemap diff A B` compare deux ensembles publiés : relecture
  bornée, réencodage canonique, même tri externe et même diff. Le test
  d'aller-retour exact exigé par l'ADR-0013 (relire puis réencoder redonne
  les octets du générateur) passe sur toutes les fixtures.

### Phase C : ce qui a été fait (2026-11-22)

- Stratégie `consistent-hash` (jump hash sur `fnv1a64(loc)`), figée par des
  vecteurs de test, disponible dans le protocole et la CLI.
- `explain` lit le snapshot précédent et compte exactement les URL qui
  changeraient de fichier, et signale une partition trop remplie (nombre
  d'URL seulement).
- Mesure de validation : de 24 à 25 partitions à 1 M d'URL, 4,0 % d'URL
  déplacées (1/(N+1) = 4 %), contre 96,0 % avec `stable-hash` ; la
  prédiction d'`explain` est égale au rapport de `generate`.
- Limite : tous les anciens fichiers sont réécrits lors de
  l'agrandissement. Les réserves en octets et en news ne sont pas encore
  mesurées (le snapshot ne garde pas les tailles).

## 5. Questions ouvertes (à mesurer avant de trancher)

1. ~~Coût de `fsync`~~ **Tranché par la mesure** : comparer après avoir
   écrit en staging ne faisait rien gagner (12,2 s contre 12,4 s à 1 M
   d'URL). Déplacer le `fsync` a ramené la réexécution à 4,5 s à 1 M, mais
   pas à 5 M (74,8 s) : l'écriture en staging dominait encore. La
   comparaison pendant l'écriture, sans fichier temporaire, a donné 22,6 s
   à 5 M.
2. **Emplacement du snapshot** : le dossier de sortie est souvent public.
   Le snapshot ne contient que des empreintes d'URL déjà publiques, mais il
   peut peser 32 octets par URL. Proposition : un dossier d'état séparé
   (`state.directory`, défaut `.advanced-sitemap/`), à valider.
3. ~~Taille du snapshot à 50 M d'URL~~ **Mesuré** : environ 45 octets par
   URL après compression par préfixe, soit environ 2,2 Go à 50 M, trié sur
   disque et lu en flux.
4. **`lastmod` d'un index quand aucune entrée n'a de `lastmod`** : aujourd'hui
   l'index omet `lastmod`. Le moteur ne lisant jamais l'horloge, cela reste
   ainsi ; à confirmer.
