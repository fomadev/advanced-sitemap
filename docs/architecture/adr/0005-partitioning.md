# ADR-0005 — Partitionnement exact et hachage stable

- Statut : accepté
- Date : 2026-11-21

## Décision

- Les limites par fichier (défaut : 50 000 URL, 52 428 800 octets non
  compressés) s'appliquent à la **taille réelle** : chaque bloc `<url>` est
  encodé dans un tampon avant écriture, puis routé. Aucune estimation
  n'intervient dans la génération ; la marge de sécurité se règle en
  abaissant `maxBytes` ou `maxUrls`.
- Stratégie `sequential` (défaut) : remplissage dans l'ordre d'entrée,
  nouvelle partition à saturation. Mémoire O(1).
- Stratégie `stable-hash` : partition = `fnv1a64(loc) mod N`. Une URL ne
  change pas de partition tant que N est constant, ce qui prépare la
  génération incrémentale. Une partition saturée fait échouer la génération
  (`PARTITION_LIMIT_EXCEEDED`) : déplacer l'URL casserait la stabilité. N est
  explicite ou suggéré par `explain`.
- FNV-1a 64 bits est implémenté dans le dépôt et figé par des vecteurs de
  test : il ne doit **jamais** changer.
- Une seule partition et `index = auto` : le fichier est publié sous
  `<stem>.xml` sans index. Sinon `<stem>-<n>.xml` et un index `<stem>.xml`.
