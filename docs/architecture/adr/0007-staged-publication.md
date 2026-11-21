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

## Mise à jour du 2026-11-22 (phase A, ADR-0010)

- Le `fsync` d'un fichier a lieu dans `publish`, juste avant son renommage,
  et non plus à la fin de son écriture en staging. La garantie est la même :
  les données sont sur disque avant d'être visibles sous leur nom public.
  Un fichier de staging jamais publié ne coûte plus de `fsync`.
- Avec `output.publish = "changed"`, le store compare chaque nouvel
  artefact au fichier publié de même nom **pendant** son écriture. Tant que
  les octets sont identiques, rien n'est écrit ; à la première différence, le
  préfixe déjà comparé est recopié depuis le fichier publié et l'écriture
  continue en staging. Un artefact identique n'est ni écrit, ni synchronisé,
  ni renommé.
