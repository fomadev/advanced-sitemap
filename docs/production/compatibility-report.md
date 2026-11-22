# Rapport de compatibilité

État au 2026-11-22, après le lot F. Remplace la
[matrice de compatibilité](compatibility-matrix.md) pour les plateformes.

## 1. Plateformes testées

Chaque PR exécute la suite complète (218 tests sous Windows, dont 3
propres à Windows) sur cinq plateformes, puis la vérification de
déterminisme ci-dessous.

| Système | Architecture | Runner CI | Tests | Déterminisme | Binaire candidat |
|---|---|---|---|---|---|
| Linux (Ubuntu) | x86_64 | `ubuntu-latest` | Oui | Oui | glibc et musl, vérifiés contre les fixtures |
| Linux (Ubuntu) | ARM64 | `ubuntu-24.04-arm` | Oui | Oui | glibc, vérifié |
| Windows 11 | x86_64 | `windows-latest`, poste de développement | Oui | Oui | vérifié |
| Windows 11 | ARM64 | `windows-11-arm` | Oui | Oui | vérifié |
| macOS | ARM64 | `macos-latest` | Oui | Oui | vérifié |
| macOS | x86_64 | — | Non | Non | construit par compilation croisée, non exécuté |
| WebAssembly, 32 bits | — | — | Non | Non | Non |

R16 (ARM64 non testé) est donc corrigé.

## 2. Déterminisme entre plateformes

`tools/determinism.py`, exécuté sur les cinq plateformes :

1. génération complète de 20 000 URL (stratégie `consistent-hash`, 4
   partitions, snapshot), dont des URL avec `&` et des liens hreflang ;
2. `apply` d'un flux fixe : 21 mises à jour, 10 suppressions, une URL avec
   des caractères non ASCII et `<` `>` à échapper.

Le job `determinism` compare les SHA-256 des 6 fichiers publiés (4 sitemaps, l'index, le manifeste) et du
snapshot après chaque étape : ils sont identiques sur les cinq plateformes,
et identiques à l'exécution locale sous Windows x86_64. Les fixtures de
conformité sont aussi comparées octet par octet par les tests, et par
chaque binaire candidat.

## 3. Rust

| Élément | Valeur |
|---|---|
| Toolchain de développement | 1.99.0 (`rust-toolchain.toml`) |
| MSRV | 1.89, vérifiée par le job `msrv` |
| Raison du minimum | `File::try_lock` (1.89) ; `quick-xml` 0.42 exige 1.86 |
| Édition | 2024 |
| Politique de montée | proposée dans [semver-policy.md](semver-policy.md), à valider |

## 4. Formats

| Format | Version | Compatibilité |
|---|---|---|
| Configuration | `schemaVersion` 1.0 et 1.1 | Une version plus récente est refusée explicitement |
| Rapport JSON | 1.0 | Champs ajoutés facultatifs |
| Enregistrement de changement | 1.1 | — |
| Snapshot | v2 écrit, v1 lu (fichier figé dans les tests) | Abandonner une lecture serait un changement incompatible |
| Manifeste | 1.0, avec empreintes par fichier depuis le lot B | Un manifeste sans empreintes reste lu ; `doctor` le signale |
| Sitemaps | sitemaps.org 0.9, extensions image, vidéo 1.1, news 0.9, `xhtml:link` | Validés par les XSD officiels en CI |

## 5. Protocoles externes vérifiés

| Spécification | Vérification |
|---|---|
| Sitemap 0.9 et index | XSD sitemaps.org (xmllint, job `conformance`) |
| Google Video 1.1, News 0.9 | XSD Google (xmllint) |
| Images, hreflang | Vérificateur Python indépendant (`tools/check_sitemaps.py`) |
| robots.txt | RFC 9309, tests unitaires |
