# Revue des licences

Ceci n'est pas un avis juridique ; c'est un relevé factuel des textes
présents dans les dépôts et des points à trancher par FomaDev.

## Licences relevées

| Projet | Fichier | Licence | Titulaire indiqué |
|---|---|---|---|
| `next-advanced-sitemap` | `LICENSE` | Fordima Public License (FPL) v1.0 | « Fordima Inc. » |
| `next-advanced-sitemap-cli` | `LICENSE` | FPL v1.0 | idem |
| `next-advanced-sitemap-cms` | `LICENSE` | FPL v1.0 | idem |
| `next-advanced-sitemap-pro` | `LICENSE` | Fordima Commercial License (Pro) v1.0 | idem |
| `next-advanced-sitemap-cloud` | `LICENSE` | Fordima Commercial License (Cloud) v1.0 | idem |
| `cloud-backend`, `cloud-dashboard` | — | aucun fichier de licence | — |

## Points pertinents de la FPL v1.0

- Section 4 a/b : une licence commerciale payante est requise pour
  construire, à partir de ce code, une bibliothèque similaire ou concurrente,
  ou un produit dérivé / de remplacement.
- Section 5 : interdiction des forks indépendants permanents.
- Section 6 : conservation de la notice de copyright.

Ces restrictions visent des tiers. Le moteur `advanced-sitemap` est
développé par le titulaire des droits (FomaDev / Fordima) : il n'y a pas
d'obstacle à réutiliser ses propres idées. Néanmoins :

1. **Aucun code TypeScript n'a été copié.** Le moteur Rust est une nouvelle
   implémentation fondée sur la compréhension des comportements et des
   spécifications publiques (sitemaps.org, documentation Google, RFC 9309,
   indexnow.org).
2. Les contraintes issues des produits commerciaux (`-pro`, `-cloud`) n'ont
   pas été intégrées au moteur ; leur éventuelle intégration est une
   décision commerciale.

## Incohérences à corriger (dans les dépôts existants, plus tard)

- Les en-têtes de fichiers mélangent « Fordima Inc. » (`types/*.ts`,
  `cross-validator.ts`) et « Fordi / FomaDev » sous « FomaDev Public
  License » (`generator.ts`, `pipeline.ts`, `streaming.ts`, builders).
- `package.json` de `next-advanced-sitemap` déclare `"license": "FomaDev Public License"`,
  le fichier `LICENSE` s'intitule « Fordima Public License ». npm attend un
  identifiant SPDX ou `SEE LICENSE IN LICENSE`.

## Décisions requises pour `advanced-sitemap`

1. **Licence du moteur.** Décidée le 2026-11-21 : FPL v1.0, appliquée
   telle que fournie. Les clarifications proposées sont dans
   [docs/legal/fpl-review.md](../legal/fpl-review.md). Toutes les crates
   gardent `publish = false`. Options examinées à l'origine :
   FPL (cohérence avec l'écosystème, mais non reconnue OSI, ce qui freine
   l'adoption par les frameworks et les distributions), MIT/Apache-2.0
   (adoption maximale), ou double licence (open source + commerciale).
2. **Titulaire** : « Fordima Inc. » dans le texte fourni ; titularité réelle à confirmer (voir fpl-review.md, 2.6).
3. **Dépendances** : toutes les dépendances retenues sont sous MIT et/ou
   Apache-2.0 (serde, serde_json, url, clap ; criterion et quick-xml en
   développement uniquement). Une vérification automatique (`cargo deny`)
   est prévue dans la CI.
