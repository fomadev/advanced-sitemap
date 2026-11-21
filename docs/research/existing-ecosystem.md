# Écosystème existant — état des lieux

Audit réalisé le 2026-11-21, en lecture seule, sur les dépôts de l'écosystème présents
dans l'espace de travail local du mainteneur. Toutes les affirmations ci-dessous proviennent de la
lecture des fichiers cités ; rien n'a été modifié dans ces projets.

## Vue d'ensemble

| Projet | Nature | Version | Licence | Dépend de | Taille du code source |
|---|---|---|---|---|---|
| `next-advanced-sitemap` | Bibliothèque npm publique, cœur actuel | 2.0.0 | Fordima Public License (FPL) v1.0 | `next >=13`, `react >=18` (peer) | ~2 400 lignes TS, 29 fichiers de tests Vitest |
| `next-advanced-sitemap-cli` | CLI de vérification d'un sitemap distant | 1.0.1 | FPL v1.0 | `next-advanced-sitemap >=2.0.0` | 3 fichiers |
| `next-advanced-sitemap-cms` | Adaptateurs Strapi, Contentful, Sanity, Shopify | — | FPL v1.0 | `next-advanced-sitemap` (types) | 6 fichiers |
| `next-advanced-sitemap-pro` | Signature HMAC, multi-tenant, analyse de logs | 1.0.0 | Fordima Commercial License (Pro) | `next-advanced-sitemap` (peer) | 4 fichiers |
| `next-advanced-sitemap-cloud` | Monitoring + alertes webhook | — | Fordima Commercial License (Cloud) | `next-advanced-sitemap` | 3 fichiers |
| `cloud-backend` | SaaS privé : API et workers (détails internes non publiés) | — | non précisée (pas de LICENSE) | indépendant (ré-implémente) | API + worker |
| `cloud-dashboard` | Front Next.js du SaaS, i18n EN/FR | — | non précisée | API du backend | UI |
| `social` | Une image promotionnelle (`next-advanced-sitemap-v2-devto-1000x420.png`) | — | — | — | aucun code |

## `next-advanced-sitemap` (référence fonctionnelle)

### Points d'entrée publics

- `src/index.ts` : point d'entrée « Edge-safe ». Exporte les fonctions
  `getServerSitemapResponse`, `getServerSitemapIndexResponse`,
  `getRobotsTextResponse`, `getServerSitemapStreamResponse` (toutes renvoient
  un objet `Response` Web), ainsi que les utilitaires : `chunkSitemapEntries`,
  `buildCacheControlHeader`, `buildRobotsText`, suppression/expiration,
  priorité par profondeur, détection staging, slash final, export CSV/JSON,
  diff, analyse, découpage par type, health check HTTP, IndexNow,
  `createSitemapXmlStream`.
- `src/scanner-entry.ts` (`next-advanced-sitemap/scanner`) : scanner de routes
  App Router, Node uniquement (`node:fs`, `node:path`), séparé pour ne pas
  casser les bundles Edge.

### Organisation interne

```
src/
  types/sitemap.ts, types/robots.ts      modèles (interfaces TS)
  core/generator.ts                       génération XML en mémoire (chaîne)
  core/streaming.ts                       génération XML en ReadableStream
  core/pipeline.ts                        traitement par entrée (staging, slash, suppression, validations)
  core/strict-validator.ts                mode strict
  core/validation/cross-validator.ts      validations croisées vidéo / news
  core/builders/*.ts                      un constructeur XML par extension + robots + index
  core/indexnow.ts                        soumission IndexNow
  core/scanner.ts                         découverte des routes App Router
  utils/*.ts                              utilitaires (échappement, chunking, diff, analyse...)
```

Le pipeline central est `processEntry()` (`core/pipeline.ts`), partagé entre
la génération en mémoire et la génération en flux : alias `video` → `videos`,
exclusion staging, normalisation du slash final, règles de
suppression/expiration (avec garde-fou sur la page d'accueil), validation
croisée, mode strict.

### Tests et CI

- 29 fichiers Vitest dans `tests/` couvrant échappement, génération e2e,
  index, streaming, strict mode, vidéo (durée, live, restrictions, métriques),
  news, images, robots, IndexNow (avec `fetch` mocké), scanner (fixtures
  App Router dans `tests/fixtures/app-router`), diff, suppression, sécurité.
- Workflows : `tests.yml`, `security.yml`, `release.yml`, `robots.yml`,
  Dependabot. `size-limit` borne le bundle à 40 Ko (index) et 8 Ko (scanner).
- Aucun benchmark de performance (le seul budget mesuré est la taille du
  bundle).

### Documentation

`README.md` (224 lignes) et `DOCUMENTATION.md` (873 lignes) : guide
d'installation, routes App Router, extensions, robots, streaming, strict
mode, cycle de vie, scanner (marqué expérimental), IndexNow, référence API.

## Projets satellites

### `next-advanced-sitemap-cli`

Binaire `next-advanced-sitemap-check <url>` : télécharge un sitemap distant,
détecte `urlset`/`sitemapindex` par expression régulière, extrait les `<loc>`
par regex, mesure la taille, puis exécute un balayage HEAD de chaque URL via
`validateSitemapHealth`. Code de sortie 1 si document inconnu ou liens cassés.
Pas de validation de schéma réelle malgré la description du package, pas de
suivi des index enfants, `fetch` sans protection SSRF ni limite de taille.

### `next-advanced-sitemap-cms`

`mapRecordsToSitemapEntries()` (`src/common.ts`) : mapping générique d'un
enregistrement vers `SitemapEntry` via un chemin pointé ou une fonction
(slug, date de mise à jour, image), plus des adaptateurs fins par CMS.
Conceptuellement, c'est un **connecteur de source** : à généraliser en
contrat de source dans le moteur, pas à porter tel quel.

### `next-advanced-sitemap-pro` (commercial)

- Signature HMAC-SHA256 ajoutée en commentaire XML en fin de
  document, vérification à temps constant.
- Multi-tenant : choix d'une configuration selon l'en-tête `Host`.
- Analyse de logs : couverture de crawl à partir de logs au format Common
  Log Format filtrés par user-agent.

### `next-advanced-sitemap-cloud` (commercial)

`runSitemapMonitorCheck` (fetch + regex `<loc>` + health check + diff
optionnel) et `sendWebhookAlert` (Slack/Discord/générique).

### `cloud-backend` / `cloud-dashboard` (SaaS)

Le backend contient la meilleure implémentation **de lecture** de sitemaps de
l'écosystème : parseur XML réel, décompression gzip bornée, suivi des index enfants
(max 200), limites 50 000 URL / 50 Mo, nouvelle tentative, et un catalogue de
codes d'anomalies stables (`INVALID_XML`, `SIZE_EXCEEDED`,
`URL_LIMIT_EXCEEDED`, `DUPLICATE_URLS`, `INVALID_LASTMOD`, `FUTURE_LASTMOD`,
`FOREIGN_HOSTS`, `INSECURE_URLS`, ...). Il dispose d'un client HTTP avec
protection SSRF (résolution DNS, refus des adresses privées à
chaque redirection), timeouts et lecture bornée, et gère IndexNow multi-endpoints avec vérification de la clé hébergée.

## Répartition moteur universel / produits

| Relève du moteur universel | Relève d'un adaptateur de framework | Relève d'un produit commercial / SaaS |
|---|---|---|
| Modèle, validation, sérialisation XML, index, partitionnement, streaming, diff, diagnostics, robots.txt, IndexNow (client), lecture/inspection de sitemaps, contrat de source | Objets `Response`, en-têtes `Cache-Control`, `X-Robots-Tag`, scanner App Router, conventions `app/sitemap.xml/route.ts` | Monitoring planifié, alertes, tableaux de bord, multi-tenant hébergé, analyse de logs, stockage historique |

La signature HMAC est techniquement générique ; sa place (moteur sous
feature optionnelle ou produit) est une décision commerciale à prendre par
FomaDev — voir `licensing-review.md`.

## Constats transverses

1. **Une seule logique de génération** existe aujourd'hui, en TypeScript,
   couplée à l'API Web `Response`. Le moteur Rust doit la remplacer comme
   référence, les packages Next.js devenant des adaptateurs.
2. **Trois parseurs de sitemaps différents** coexistent (regex dans la CLI,
   regex dans Cloud, `fast-xml-parser` dans le backend) avec des résultats
   potentiellement divergents. Un inspecteur unique dans le moteur résoudrait
   cette incohérence.
3. **Les erreurs sont des exceptions textuelles** (`throw new Error('[next-advanced-sitemap] ...')`),
   levées à la première violation. Seul le backend utilise des codes stables.
4. **Le temps système est lu pendant la génération** (`autoLastmod`,
   expiration, fenêtre de 48 h des news, commentaire d'horodatage robots),
   ce qui rend la sortie non déterministe.
