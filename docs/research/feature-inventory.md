# Inventaire des fonctionnalités

Statuts : **Implémenté** (code + tests), **Expérimental** (marqué comme tel
dans le code ou la doc), **Doc seule** (annoncé sans code), **Absent**.

Priorités : **P0** = MVP Rust, **P1** = phase 3 (fonctionnalités avancées),
**P2** = phase 4+ (intelligence, bindings), **Hors moteur** = adaptateur ou
produit.

Abréviations projets : NAS = `next-advanced-sitemap`, CLI = `-cli`,
CMS = `-cms`, PRO = `-pro`, CLD = `-cloud`, BE = `cloud-backend`.

## 1. Génération et protocole

| Fonctionnalité | Projet | Fichier | Comportement actuel | Statut | Limites identifiées | Généralisation | Module Rust | Priorité | Tests nécessaires |
|---|---|---|---|---|---|---|---|---|---|
| `urlset` / `url` / `loc` | NAS | `core/generator.ts`, `builders/url-builder.ts` | Concaténation de chaînes, 5 namespaces toujours déclarés | Implémenté | Pas de mesure de taille ; namespaces inutiles déclarés | Écriture vers `io::Write`, namespaces selon les extensions utilisées | `sitemap-xml` | P0 | Conformité XML, octet à octet sur fixtures |
| `lastmod` | NAS | `url-builder.ts` | Chaîne recopiée telle quelle, `Date` → ISO | Implémenté | **Aucune validation du format** W3C Datetime | Type `W3cDateTime` validé et comparable | `sitemap-model` | P0 | Formats valides/invalides, années bissextiles, fuseaux |
| `changefreq` | NAS | `types/sitemap.ts` | Union TS, non vérifiée à l'exécution | Implémenté | Valeurs arbitraires acceptées depuis JS non typé | Enum + désérialisation stricte | `sitemap-model` | P0 | Valeurs inconnues rejetées |
| `priority` | NAS | `url-builder.ts` | `toFixed(1)` | Implémenté | **Pas de contrôle 0.0–1.0** ; 0.75 devient 0.8 | Type `Priority` borné, sérialisation sans perte | `sitemap-model` | P0 | Bornes, NaN, arrondi |
| Limite 50 000 URL | NAS | `generator.ts`, `streaming.ts` | Exception au-delà | Implémenté | Pas de limite 50 Mo ; l'utilisateur doit découper lui-même | Partitionnement automatique sur URL **et** octets mesurés | `sitemap-index` | P0 | Bascule exacte à N et N+1, limite d'octets |
| Sitemap Index | NAS | `builders/index-builder.ts` | `loc` + `lastmod` (option `autoLastmod` = date courante) | Implémenté | Index construit à la main par l'utilisateur ; `lastmod` non validé | Index généré automatiquement depuis les partitions, `lastmod` = max des entrées | `sitemap-index` | P0 | Index cohérent avec les fichiers écrits |
| Découpage (`chunkSitemapEntries`) | NAS | `utils/chunker.ts` | Découpe un tableau en tranches de taille fixe | Implémenté | En mémoire uniquement, ignore la taille en octets | Partitionnement séquentiel en flux | `sitemap-index` | P0 | Propriétés : aucune perte, ordre préservé |
| Découpage par type | NAS | `utils/split-by-type.ts` | Groupes pages/images/vidéos/news | Implémenté | Les entrées sont dupliquées entre groupes | Stratégie de partitionnement « par catégorie » | `sitemap-index` | P1 | Partition déterministe |
| Tri par priorité | NAS | `generator.ts` | Tri décroissant (défaut 0.5) | Implémenté | Impossible en flux | Transformation optionnelle en mode mémoire | `advanced-sitemap` | P2 | Stabilité du tri |
| Streaming | NAS | `core/streaming.ts` | `ReadableStream`, itérable asynchrone | Implémenté | Toujours limité à 50 000 URL, un seul fichier | Flux multi-partitions avec mémoire bornée | `advanced-sitemap` + `sitemap-xml` | P0 | Mémoire stable, sortie identique au mode mémoire |
| Échappement XML | NAS | `utils/xml-escape.ts` | 5 entités | Implémenté | **Caractères de contrôle interdits en XML 1.0 non filtrés** | Échappement + refus des caractères invalides | `sitemap-xml` | P0 | Fuzz / propriétés : sortie toujours bien formée |

## 2. Extensions

| Fonctionnalité | Projet | Fichier | Comportement actuel | Statut | Limites identifiées | Généralisation | Module Rust | Priorité | Tests nécessaires |
|---|---|---|---|---|---|---|---|---|---|
| Images | NAS | `builders/image-builder.ts` | `loc`, `title`, `caption`, `geo_location`, `license` | Implémenté | `title`/`caption`/`geo_location`/`license` ne sont plus pris en charge par Google depuis 2022 ; pas de limite 1 000 images/URL | `image:loc` uniquement + limite par URL | `sitemap-model`, `sitemap-xml` | P0 (loc) | Namespace déclaré, limite 1 000 |
| Vidéos | NAS | `builders/video-builder.ts` | ~17 champs, contrôles durée 0–28 800 s, pays, plateformes, prix, 32 tags | Implémenté | Validation mêlée à la sérialisation ; `category` et `price` retirés par Google en 2023 (à reconfirmer) ; longueurs titre/description non contrôlées | Modèle typé + règles de validation séparées | `sitemap-extensions` | P1 | Chaque contrainte, règle croisée live/durée |
| Google News | NAS | `builders/news-builder.ts`, `cross-validator.ts` | publication, langue, titre, tickers ; **exception** si article > 48 h | Implémenté | La règle des 48 h dépend de l'horloge et bloque la génération | Diagnostic configurable avec horloge injectée ; limite 1 000 news/fichier | `sitemap-extensions` | P1 | Horloge figée en test |
| Hreflang | NAS | `url-builder.ts` | `xhtml:link` émis sans contrôle du code langue | Implémenté | Pas de validation BCP 47, pas de `x-default` vérifié, pas de réciprocité | Validation des codes, doublons de langue, réciprocité (passe globale) | `sitemap-validation` | P0 (codes) / P1 (réciprocité) | Codes valides/invalides, doublons |

## 3. Robots, IndexNow, découverte

| Fonctionnalité | Projet | Fichier | Comportement actuel | Statut | Limites identifiées | Généralisation | Module Rust | Priorité | Tests nécessaires |
|---|---|---|---|---|---|---|---|---|---|
| robots.txt | NAS | `builders/robots-builder.ts` | Groupes user-agent, Allow/Disallow, Crawl-delay, Host, Sitemap ; commentaire horodaté | Implémenté | `Host` est une directive Yandex obsolète et est **déduite automatiquement** ; horodatage non déterministe ; pas d'analyse d'un robots existant | Générateur déterministe + validateur (RFC 9309) | `sitemap-robots` | P1 | Sortie stable, groupes, lignes vides |
| IndexNow | NAS, BE | `core/indexnow.ts`, service privé | POST de 1 à 10 000 URL ; BE vérifie la clé hébergée | Implémenté | NAS : pas de retry, pas de rate limiting ; pas de contrôle que les URL appartiennent à `host` | Client optionnel (feature Cargo), lots, retry borné, SSRF | `sitemap-indexnow` | P2 | Serveur HTTP local simulé |
| Scanner App Router | NAS | `core/scanner.ts` | Parcours `app/`, groupes ignorés, `[lang]` expansé, métadonnées par regex | Expérimental | Spécifique Next.js | Reste dans l'adaptateur Next ; le moteur expose un contrat de **source** | Hors moteur (adaptateur) | Hors moteur | — |
| Connecteurs CMS | CMS | `src/common.ts` et adaptateurs | Mapping d'enregistrements vers entrées | Implémenté | Lié aux types TS de NAS | Contrat de source + connecteur de référence | `sitemap-plugin-api` | P2 | Fixtures par connecteur |

## 4. Cycle de vie, analyse, diagnostics

| Fonctionnalité | Projet | Fichier | Comportement actuel | Statut | Limites identifiées | Généralisation | Module Rust | Priorité | Tests nécessaires |
|---|---|---|---|---|---|---|---|---|---|
| Suppression / expiration | NAS | `core/pipeline.ts`, `utils/deletion.ts` | Entrée conservée avec priority 0, changefreq never, commentaire XML « Pending Removal » ; refus sur la page d'accueil | Implémenté | Les moteurs de recherche n'interprètent pas les commentaires XML ; dépend de l'horloge | Filtrage explicite (`expiresAt` évalué avec une horloge injectée) — ne pas porter le commentaire | `sitemap-lifecycle` | P2 | Horloge figée |
| Diff | NAS, CLD | `utils/diff.ts` | Ajouts / suppressions / changements par champ (images/vidéos comparées en nombre) | Implémenté | En mémoire, comparaison superficielle | Empreintes déterministes par entrée, partitions affectées | `sitemap-lifecycle` | P2 | Propriétés diff(a,a)=vide |
| Analyse / score | NAS | `utils/analyze.ts` | Doublons, lastmod manquant, score 0–100 | Implémenté | Score heuristique non documenté | Remplacé par diagnostics codés + statistiques | `sitemap-diagnostics` | P0 (diagnostics) | Codes stables |
| Mode strict | NAS | `core/strict-validator.ts` | HTTPS, lastmod/changefreq/priority requis, vidéo player/content | Implémenté | S'arrête à la première erreur | Profil de validation `strict` qui promeut certains avertissements | `sitemap-validation` | P0 | Matrice standard/strict |
| Exclusion staging | NAS | `utils/staging.ts` | Sous-chaîne sur le hostname | Implémenté | « staging. » attrape aussi des domaines légitimes | Règle d'hôtes autorisés (`site`) + motifs explicites | `sitemap-validation` | P1 | — |
| Slash final | NAS | `utils/trailing-slash.ts` | add/remove/preserve | Implémenté | — | Option de canonicalisation documentée | `sitemap-model` (canonicalisation) | P1 | Racine jamais modifiée |
| Priorité par profondeur | NAS | `utils/priority.ts` | 1.0 − 0.2 × profondeur | Implémenté | Heuristique | Transformation optionnelle | `advanced-sitemap` | P2 | — |
| Export CSV/JSON | NAS | `utils/export.ts` | Sérialisation simple | Implémenté | — | Format de rapport du protocole | `sitemap-protocol` | P2 | — |
| Health check HTTP | NAS, CLI, CLD | `utils/health-check.ts` | HEAD concurrent, timeout | Implémenté | Pas de protection SSRF dans NAS (BE en a une) | Module réseau optionnel avec garde SSRF | `sitemap-inspect` (futur) | P2 | Serveur local |
| Lecture / inspection d'un sitemap | CLI, CLD, BE | `parse.ts`, `monitor.ts`, `scans/sitemap.ts` | Regex (CLI, CLD) ou parseur XML (BE) | Implémenté | Trois implémentations divergentes | Inspecteur XML unique, en flux, borné | `sitemap-inspect` | P1 | Fixtures réelles, XML malformé, gzip |
| Codes d'anomalie stables | BE | `scans/sitemap.ts` | `code` + `severity` + `params` | Implémenté | Uniquement côté SaaS | Catalogue de diagnostics du moteur | `sitemap-model::diagnostic` | P0 | Codes figés par test |
| Cache-Control / X-Robots-Tag | NAS | `utils/cache-control.ts`, `robots-tag.ts` | En-têtes HTTP | Implémenté | Propres à la couche HTTP | Restent dans les adaptateurs | Hors moteur | Hors moteur | — |

## 5. Produits commerciaux

| Fonctionnalité | Projet | Fichier | Statut | Décision proposée |
|---|---|---|---|---|
| Signature HMAC | PRO | privé | Implémenté | Candidat à une feature Cargo optionnelle **si** FomaDev accepte de la rendre publique ; sinon reste produit |
| Multi-tenant | PRO | privé | Implémenté | Le moteur est sans état et multi-instanciable : la sélection par `Host` reste dans l'adaptateur/produit |
| Analyse de logs de crawl | PRO | privé | Implémenté | Produit (hors moteur) |
| Monitoring + alertes | CLD, BE | `monitor.ts`, `alerts/*` | Implémenté | Produit ; s'appuiera sur l'inspecteur du moteur |
| GEO / audit IA | BE | privé | Implémenté | Produit ; hors moteur (le moteur ne dépend d'aucun LLM) |

## Synthèse : réutilisation conceptuelle vs nouvelle conception

- **Réutilisables conceptuellement** : liste des champs et contraintes vidéo
  et news, règles du mode strict, garde-fous de volume, structure robots.txt,
  codes d'anomalies du backend, garde SSRF du backend.
- **Nouvelle conception nécessaire** : validation non bloquante produisant
  des diagnostics, partitionnement par octets mesurés, index automatique,
  streaming multi-partitions, empreintes et diff déterministes, horloge
  injectée, contrats de protocole versionnés, publication atomique.
- **Ne pas porter** : commentaires XML « Pending Removal », directive `Host`
  déduite automatiquement, champs image retirés par Google, horodatages dans
  la sortie.
