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
| A | Empreinte stable des partitions et snapshot de partitions ; ne publier que les fichiers dont l'empreinte change | Moins d'écritures disque et un `lastmod` d'index stable | `generate` relancé sans changement : 0 fichier publié ; durée comparée à `validate` |
| B | Snapshot par URL (clé et contenu), Diff Engine, rapport de changements | Liste exacte des ajouts, retraits et modifications | Comparaison avec un diff calculé par relecture complète |
| C | Stratégie `consistent-hash` (ADR-0012) et planner qui chiffre les déplacements | Agrandir le site sans déplacer la plupart des URL | Fraction d'URL déplacées mesurée contre 1/(N+1) |
| D | Flux de changements (ajouts, mises à jour, suppressions) appliqués sur les fichiers publiés (ADR-0013) | Plus besoin de relire toute la source | Oracle I1 entre application du flux et régénération complète |

La phase A ne demande aucun nouveau format d'entrée et ne change aucun
contrat existant : c'est le point de départ recommandé.

## 5. Questions ouvertes (à mesurer avant de trancher)

1. **Coût de `fsync`** par rapport à l'écriture elle-même. La phase A écrit
   d'abord chaque partition dans la zone temporaire puis compare son
   empreinte ; si le coût dominant est l'écriture, il faudra hacher avant
   d'écrire (tampon d'une partition, au plus 50 Mio) plutôt qu'après.
2. **Emplacement du snapshot** : le dossier de sortie est souvent public.
   Le snapshot ne contient que des empreintes d'URL déjà publiques, mais il
   peut peser 32 octets par URL. Proposition : un dossier d'état séparé
   (`state.directory`, défaut `.advanced-sitemap/`), à valider.
3. **Taille du snapshot par URL à 50 M d'URL** (environ 1,6 Go) : faut-il un
   format trié sur disque dès la phase B, ou un seuil au-delà duquel seul le
   snapshot de partitions est tenu ?
4. **`lastmod` d'un index quand aucune entrée n'a de `lastmod`** : aujourd'hui
   l'index omet `lastmod`. Le moteur ne lisant jamais l'horloge, cela reste
   ainsi ; à confirmer.
