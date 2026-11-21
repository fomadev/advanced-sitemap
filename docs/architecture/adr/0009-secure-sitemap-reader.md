# ADR-0009 — Lecteur de sitemaps sécurisé

- Statut : accepté
- Date : 2026-11-21

## Contexte

`inspect`, puis le diff et la génération incrémentale (phase 4), doivent
lire des sitemaps existants, y compris des fichiers téléchargés depuis des
sites tiers. L'écosystème actuel a trois lecteurs divergents (regex dans la
CLI et dans Cloud, `fast-xml-parser` dans le backend). Un fichier hostile
peut viser l'expansion d'entités (« billion laughs »), les entités externes
(XXE), une bombe gzip, une imbrication profonde ou un texte démesuré.

## Décision

1. Nouvelle crate `sitemap-reader`, en flux : les entrées ne sont jamais
   accumulées, elles sont passées à un callback.
2. Analyse XML avec `quick-xml` (déjà utilisé en test), qui ne traite pas
   les DTD et n'étend aucune entité déclarée. En plus, tout `DOCTYPE` arrête
   la lecture : un sitemap n'en a jamais besoin.
3. Décompression gzip avec `flate2`, backend `miniz_oxide` (pur Rust, pas de
   compilateur C). Le gzip est détecté par ses octets magiques.
4. Un lecteur compteur placé **après** la décompression borne la taille non
   compressée (50 MiB par défaut, limite du protocole). La profondeur (32)
   et la taille du texte d'un élément (64 KiB) sont bornées.
5. Chaque entrée est validée par `sitemap-validation`, avec les mêmes codes
   que la génération. Les problèmes propres à la lecture ont leurs codes
   (`XML_*`, `SITEMAP_TOO_LARGE`, `GZIP_INVALID`, ...).
6. Pas de réseau dans cette crate. Le téléchargement viendra avec une garde
   SSRF (résolution DNS, refus des adresses privées à chaque redirection,
   délais, taille bornée).

## Conséquences

- `quick-xml` devient une dépendance d'exécution ; `flate2`, `miniz_oxide`,
  `crc32fast`, `adler2` et `simd-adler32` (toutes sous MIT ou
  Apache-2.0) entrent dans le graphe, contrôlés par `cargo deny`.
- Les extensions vidéo et news sont comptées et ignorées tant qu'elles ne
  sont pas modélisées.
- L'ordre XSD est vérifié en avertissement : les moteurs de recherche
  tolèrent probablement l'ordre inverse, mais un validateur XSD le refuse.
