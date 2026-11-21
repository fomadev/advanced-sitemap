# ADR-0012 — Stratégie `consistent-hash` (jump consistent hash)

- Statut : proposé
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
