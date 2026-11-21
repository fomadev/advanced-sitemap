# ADR-0007 — Publication en staging, jamais de suppression implicite

- Statut : accepté
- Date : 2026-11-21

## Décision

- Toute écriture passe par une zone de staging propre au store
  (`<sortie>/.advanced-sitemap-staging-<id>/` pour le système de fichiers,
  sur le même volume que la destination).
- La publication renomme les fichiers dans l'ordre : partitions, puis index,
  puis manifeste. Un crawler ne voit jamais un index qui référence un fichier
  absent. Chaque renommage est atomique sur un même volume ; **l'ensemble**
  ne l'est pas, et c'est documenté.
- En cas d'échec, le staging est abandonné ; les fichiers publiés auparavant
  restent intacts.
- Le moteur ne supprime jamais d'artefact publié. Les fichiers listés dans le
  manifeste précédent mais absents du nouveau sont signalés
  (`STALE_ARTIFACT`).
