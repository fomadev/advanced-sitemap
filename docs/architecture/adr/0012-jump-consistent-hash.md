# ADR-0012 — Stratégie `consistent-hash` (jump consistent hash)

- Statut : accepté (2026-11-22)
- Date : 2026-11-21

## Contexte

`stable-hash` (`fnv1a64(loc) mod N`) garde chaque URL dans sa partition
tant que N ne change pas. Mais un site qui grandit doit augmenter N, et
alors presque toutes les URL changent de fichier. Simulation sur 200 000
URL du jeu de données de référence :

| N → N+1 | Déplacées (`mod N`) | Déplacées (jump) |
|---|---|---|
| 4 → 5 | 80,0 % | 19,96 % |
| 20 → 21 | 95,3 % | 4,76 % |
| 100 → 101 | 99,0 % | 0,99 % |

## Décision proposée

- Nouvelle stratégie `consistent-hash` :
  `partition = jump(fnv1a64(loc), N) + 1`, avec l'algorithme de Lamping et
  Veach (2014), implémenté dans le dépôt (une dizaine de lignes, aucune
  dépendance), figé par des vecteurs de test comme FNV-1a.
- Quand N passe à N+1, environ 1/(N+1) des URL vont dans la nouvelle
  partition ; aucune ne bouge entre les anciennes.
- `stable-hash` est conservé tel quel : passer de l'une à l'autre déplace
  toutes les URL une fois, ce que le planner chiffre exactement
  (`PARTITION_MIGRATION_COST`).
- Nouvelle valeur `consistent-hash` pour `partitioning.strategy` dans le
  protocole (ajout mineur).

## Conséquences

- Équilibre comparable : sur 20 partitions et 200 000 URL, 9 845 à 10 112
  URL par partition avec jump, 9 921 à 10 046 avec `mod N`.
- Réduire N n'est pas « stable » : les URL des partitions supprimées se
  répartissent sur les autres. Le planner le signale.
- Les noms de fichiers restent `<stem>-<n>.xml`.

## Mise à jour du 2026-11-22 : implémenté (phase C)

- `PartitionStrategy::ConsistentHash` et la valeur de protocole
  `consistent-hash`. `jump_consistent_hash` est figé par des vecteurs tirés
  d'une transcription Python indépendante de la figure 1 de l'article. Un
  test vérifie qu'en passant de N à N+1 aucune URL ne bouge entre anciennes
  partitions.
- Le calcul utilise des `f64` (division et multiplication correctement
  arrondies selon IEEE 754) : le résultat ne dépend pas de la plateforme ;
  les vecteurs passent en CI sous Linux, Windows et macOS.
- `explain` chiffre la migration à partir du snapshot :
  `PARTITION_MIGRATION_COST` (exact pour les deux stratégies par hachage)
  et `PARTITION_HEADROOM_LOW` (sur le nombre d'URL seulement).
- Mesuré à 1 M d'URL, de 24 à 25 partitions : 40 112 URL déplacées
  (prédiction d'`explain` égale au rapport de `generate`), contre 959 996
  avec `stable-hash`. Même débit que `stable-hash`.
- Limite constatée : tous les anciens fichiers sont réécrits lors de
  l'agrandissement, puisque chacun cède des URL à la nouvelle partition. Le
  gain porte sur l'emplacement des URL, pas sur le nombre de fichiers
  écrits ce jour-là.

## Acceptation (2026-11-22)

Accepté par le mainteneur le 2026-11-22, après une revue qui a vérifié la
décision contre le code et les tests (R18 du registre des risques). La valeur `consistent-hash` de
`partitioning.strategy` et l'affectation des URL deviennent un contrat :
la changer déplacerait les URL publiées. Le calcul en `f64` donne la même
partition sur les cinq plateformes de la CI, ARM64 compris : le job
`determinism` utilise cette stratégie.
