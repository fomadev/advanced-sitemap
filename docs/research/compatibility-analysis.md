# Analyse de compatibilité

## 1. Protocoles et moteurs de recherche

Les points ci-dessous résument les spécifications publiques telles que
connues à la date de l'audit. Chaque règle implémentée dans le moteur est
rattachée à un code de diagnostic ; les règles marquées « à reconfirmer »
doivent être revérifiées contre la documentation officielle avant d'être
figées en erreur bloquante.

| Règle | Source | Traitement moteur |
|---|---|---|
| Document UTF-8, entités échappées | sitemaps.org | Toujours appliqué |
| ≤ 50 000 URL et ≤ 50 Mo (52 428 800 octets) non compressés par fichier | sitemaps.org, Google | Partitionnement automatique |
| ≤ 50 000 sitemaps par index ; pas d'index imbriqués | sitemaps.org, Google | Erreur `INDEX_LIMIT_EXCEEDED` |
| `loc` absolue, même protocole/hôte que le sitemap, < 2 048 caractères | sitemaps.org | Validation (`URL_*`, `HOST_MISMATCH`) |
| `lastmod` au format W3C Datetime | sitemaps.org | Validation `LASTMOD_INVALID` |
| `changefreq`, `priority` | sitemaps.org | Pris en charge pour compatibilité ; Google déclare les ignorer |
| Images : `image:loc` seul encore utilisé par Google (autres balises retirées en 2022), ≤ 1 000 images par URL | Google | `image:loc` + limite |
| Vidéo : `thumbnail_loc`, `title`, `description`, `content_loc` ou `player_loc` requis ; certaines balises retirées en 2023 (`category`, `price`, `gallery_loc`, `tvshow` — à reconfirmer) | Google | Phase 3 |
| News : articles des 2 derniers jours, ≤ 1 000 `news:news` par sitemap | Google | Phase 3, diagnostic avec horloge injectée |
| Hreflang : codes ISO 639-1 (+ région ISO 3166-1 alpha-2), `x-default`, liens réciproques | Google | Codes et doublons : MVP ; réciprocité : phase 3 |
| Ping de sitemap | Google | Retiré en 2023 : **non implémenté** |
| IndexNow | indexnow.org | Bing, Yandex, Seznam, Naver, Yep ; **Google ne participe pas** |
| robots.txt | RFC 9309 | Phase 3 ; `Host` (Yandex) non émis par défaut |

## 2. Compatibilité avec `next-advanced-sitemap`

### Champs d'entrée

| `next-advanced-sitemap` | Protocole moteur v1 | Remarque |
|---|---|---|
| `url` | `loc` | `url` accepté comme alias en entrée |
| `lastmod` (`string \| Date`) | `lastmod` (chaîne W3C) | Les objets `Date` sont convertis côté adaptateur |
| `changefreq`, `priority` | identiques | `priority` désormais bornée |
| `alternates[{hreflang, href}]` | identique | Codes validés |
| `images[{loc, title, caption, geo_location, license}]` | `images[{loc}]` | Champs retirés par Google rejetés (champ inconnu) |
| `videos`, `news` | phase 3 | — |
| `isDeleted`, `expiresAt` | non repris tels quels | Voir `migration-strategy.md` |

### Options

| Option | Équivalent moteur |
|---|---|
| `autoLastmod` | Non repris par défaut (non déterministe). Si nécessaire, l'adaptateur fournit une date explicite |
| `sortByPriority` | Transformation optionnelle future (mode mémoire) |
| `maxAge` | En-tête HTTP : reste dans l'adaptateur |
| `strict` | `validation.mode = "strict"` |
| `excludeStaging` | Futur filtre d'hôtes ; `site` + `HOST_MISMATCH` couvrent le cas principal |
| `trailingSlash` | Canonicalisation optionnelle future |
| `debug` | Rapport structuré + `explain` |

### Comportements qui changent volontairement

1. Erreurs non levées à la première violation : toutes sont collectées sous
   forme de diagnostics (plafond configurable).
2. Plus de dépassement « 50 000 URL » en exception : partitionnement
   automatique.
3. Namespaces déclarés uniquement s'ils sont utilisés (mode mémoire).
4. `priority` sérialisée sans perte (0.75 reste 0.75 ; 1 devient 1.0).
5. Les URL sont émises sous forme WHATWG (percent-encodage, IDNA).

## 3. Plateformes

| Cible | État |
|---|---|
| Windows x86_64 (MSVC) | Développement et tests locaux |
| Linux x86_64 | CI (workflow préparé, non exécuté localement) |
| macOS | CI (workflow préparé, non exécuté localement) |
| Rust | stable, MSRV fixée à 1.85 (édition 2024) — à confirmer en CI |

## 4. Compatibilité des contrats

- Les documents JSON portent `schemaVersion` (`"1.0"`). Le moteur accepte
  toute version `1.x` dont le mineur est ≤ au mineur supporté ; une version
  mineure plus récente est refusée avec un message explicite plutôt que
  d'ignorer silencieusement des champs.
- Les champs inconnus sont refusés (`deny_unknown_fields`) pour attraper les
  fautes de frappe (`lastMod`, `changeFreq`).
- Les codes de diagnostic et les codes de sortie de la CLI sont des
  contrats : leur suppression ou renommage est un changement majeur.
