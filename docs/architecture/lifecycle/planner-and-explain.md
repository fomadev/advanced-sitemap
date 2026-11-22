# Generation Planner et Explain Engine

Statut : proposition. Voir les invariants dans le [README](README.md).

## 1. Ce qui existe déjà

`advanced-sitemap explain` (module `plan.rs`) échantillonne la source et
produit un `Plan` sans rien écrire. Chaque chiffre porte sa nature :

| `Quantity` | Sens |
|---|---|
| `Measured` | Mesuré sur la source réelle |
| `Estimated` | Extrapolé d'un échantillon |
| `Configured` | Repris de la configuration |
| `Unknown` | Inconnu avant l'exécution |

Il donne la taille de la source, le nombre de fichiers, une suggestion de
partitions avec 20 % de marge et la mémoire de déduplication.

Limite actuelle relevée pendant cette conception : la mémoire de
déduplication est estimée à 32 octets par URL, alors que le modèle mesuré
est de 17 à 34 octets selon la position de n entre deux puissances de deux,
avec un pic de 1,5 fois pendant l'agrandissement de la table (voir
[performance](../../performance/README.md)). Le planner utilisera le modèle
exact.

## 2. Rôle du planner

Le planner transforme « configuration + snapshot + source » en un
**plan exécutable**. Le même objet est :

- affiché par `explain` (lecture seule) ;
- exécuté par `generate`, qui n'a pas d'autre logique de décision.

C'est ce qui garantit I9 : `explain` ne peut pas annoncer autre chose que
ce que `generate` fera, puisqu'il n'y a qu'un seul code de décision.

```
PlanInput { config, snapshot: Option<Snapshot>, source: SourceInfo }
      │
      ▼  plan()                         (pur : pas d'E/S, pas d'horloge)
ExecutionPlan {
  mode: Full | SkipUnchanged | Delta,
  reason: Reason,                       // pourquoi ce mode
  partitions: Vec<PartitionAction>,     // Write | KeepIfUnchanged | Reread
  index: IndexAction,
  risks: Vec<Diagnostic>,               // réserves, migrations
  costs: Costs { reads, writes, memory } // chaque valeur est une Quantity
}
```

`plan()` est une fonction pure, donc testable sur des cas fabriqués sans
fichier. Les E/S (lire le snapshot, échantillonner la source) se font avant,
dans `PlanInput`.

## 3. Choix du mode

| Situation | Mode | `reason` |
|---|---|---|
| Pas de snapshot, ou version inconnue | `Full` | `NoSnapshot` / `SnapshotVersion` |
| Empreinte de configuration différente | `Full` | `ConfigChanged { fields }` (liste des réglages qui ont changé) |
| Source complète, snapshot valide | `SkipUnchanged` | `SnapshotValid` |
| Flux de changements, stratégie par hachage, snapshot niveau URL | `Delta` | `ChangeFeed` |
| Flux de changements avec `sequential` | Refus (erreur de configuration) | Voir [génération incrémentale](incremental-generation.md), section 3 |
| `--full` demandé | `Full` | `Forced` |

## 4. Réserves et risques

Le planner calcule, à partir du snapshot (chiffres `Measured`) ou d'un
échantillon (`Estimated`), la marge restante de chaque partition sur trois
limites : URL, octets, et news (1 000 par fichier). Il signale :

| Diagnostic proposé | Sévérité | Quand |
|---|---|---|
| `PARTITION_HEADROOM_LOW` | warning | Une partition dépasse 90 % d'une de ses trois limites : la prochaine croissance fera échouer la génération par hachage |
| `PARTITION_COUNT_SUGGESTED` | info | Nombre de partitions conseillé pour revenir à 80 % de remplissage |
| `PARTITION_MIGRATION_COST` | info | Pour un changement de N ou de stratégie : nombre exact d'URL qui changeront de fichier, calculé sur les clés du snapshot (pas une estimation) |

État (2026-11-22) : `PARTITION_MIGRATION_COST` et `PARTITION_HEADROOM_LOW`
(sur le nombre d'URL) sont émis par `explain` comme notes, avec une section
`migration` structurée. Les réserves en octets et en news, et
`PARTITION_COUNT_SUGGESTED` à partir du snapshot, restent à faire.

Le coût de migration est exact parce que le snapshot de niveau URL contient
la clé de chaque URL et que l'affectation par hachage est déterministe : il
suffit de recalculer `jump(fnv1a64(loc), N')` ou `fnv1a64(loc) mod N'`. Sans
niveau URL, le chiffre est `Estimated` (fraction 1/(N+1) pour
`consistent-hash`, N/(N+1) pour `stable-hash`).

## 5. Explain Engine

Sortie humaine et JSON (schéma versionné, comme le rapport). Ce qu'elle
ajoute à `explain` actuel :

- le mode choisi et sa raison ;
- par partition : action prévue, remplissage sur les trois limites ;
- les coûts prévus (lectures, écritures, mémoire), chacun avec sa nature ;
- pour un changement de configuration : la liste des réglages modifiés et
  leur effet (par exemple « `maxBytes` modifié : tous les fichiers seront
  réécrits »).

Après `generate`, le rapport contient le même plan plus ce qui a réellement
été fait. Un test compare les deux (I9) : toute divergence est un bogue.

## 6. Hors périmètre

- Planification dans le temps (« quand régénérer ») : le moteur ne lit pas
  l'horloge ; c'est le rôle d'un ordonnanceur externe (CI, cron, produit
  Cloud).
- Priorisation par trafic ou par logs de crawl : c'est une fonctionnalité de
  produit (`-pro`), pas du moteur.
