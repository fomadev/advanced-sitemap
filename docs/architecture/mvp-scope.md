# Périmètre du MVP et critères d'acceptation

## Inclus

| # | Capacité | Critère d'acceptation vérifiable |
|---|---|---|
| 1 | Compilation du moteur | `cargo build --workspace` et `cargo test --workspace` passent sous Windows ; workflow CI Linux/macOS/Windows fourni |
| 2 | Sitemap simple via l'API Rust | `Sitemap::builder().url(...).build()?.to_xml()?` produit un document identique à une fixture |
| 3 | Validation URL et métadonnées | URL (schéma, hôte, longueur, espaces, identifiants, fragment), `lastmod` W3C, `changefreq`, `priority` 0–1, hreflang, images ; chaque règle a un test positif et négatif |
| 4 | XML conforme | Sortie bien formée selon un parseur indépendant (quick-xml), namespaces corrects, caractères interdits refusés ; test de propriétés sur entrées aléatoires |
| 5 | Sitemap Index | Généré automatiquement au-delà d'une partition ; `loc` = URL publique + nom de fichier ; `lastmod` = max de la partition |
| 6 | Source volumineuse en flux | JSONL lu ligne à ligne avec longueur de ligne bornée ; mémoire mesurée sur 1 M et 5 M d'URL |
| 7 | Partitionnement correct | Séquentiel exact sur nombre et octets (tests de bascule à N/N+1), hachage stable avec vecteurs FNV figés |
| 8 | CLI | `advanced-sitemap generate`, `validate`, `explain` ; codes de sortie documentés et testés |
| 9 | Configuration JSON | Document `schemaVersion: "1.0"` avec schéma JSON publié ; champs inconnus refusés |
| 10 | Diagnostics structurés | Codes stables, sévérité, contexte (ligne/enregistrement/champ), suggestion ; sortie humaine et JSON |
| 11 | Tests automatisés | Unitaires, intégration, conformité (fixtures octet par octet), propriétés, CLI, publication |
| 12 | Mesures de performance | Benchmarks Criterion 1k/10k/100k et mesure CLI 1 M / 5 M avec matériel documenté |

## Exclu du MVP (planifié)

Vidéo, News, robots.txt, IndexNow, `inspect` (lecture de sitemaps
existants), `diff`, snapshots et génération incrémentale, canonicalisation
configurable, gzip, CSV, bindings Node/Python, service HTTP, plugins,
connecteurs de bases de données, sorties S3.

## Garanties explicites du MVP

- Déterminisme octet pour octet (ADR-0006).
- Aucune publication partielle en cas d'échec (ADR-0007), aucune suppression
  d'artefact.
- Aucune lecture d'horloge, de réseau ou de variable d'environnement par le
  moteur.
