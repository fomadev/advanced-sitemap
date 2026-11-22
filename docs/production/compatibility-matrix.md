# Matrice de compatibilité

État du lot A. Pour l'état actuel des plateformes (ARM64 compris) et des
formats, voir le [rapport de compatibilité](compatibility-report.md).

## Plateformes

| Plateforme | Architecture | Vérification | Statut |
|---|---|---|---|
| Windows 11 | x86_64 | Développement local (tests, mesures) et CI `windows-latest` | Supportée |
| Linux (Ubuntu, `ubuntu-latest`) | x86_64 | CI : tests, lint, conformance (xmllint), supply-chain, MSRV | Supportée |
| macOS (`macos-latest`) | ARM64 (Apple Silicon) | CI : tests | Supportée |
| macOS | x86_64 | Non testée | Non annoncée |
| Linux | ARM64 | Non testée | Non annoncée (R16) |
| Windows | ARM64 | Non testée | Non annoncée (R16) |
| WebAssembly | — | Non testée ; `FsStore` et les verrous dépendent du système de fichiers | Non annoncée |

La sortie XML est identique octet par octet sur les trois plateformes
testées (fixtures comparées en CI).

## Rust

| Élément | Valeur |
|---|---|
| Toolchain de développement | 1.99.0 (épinglée par `rust-toolchain.toml`) |
| MSRV (`rust-version`) | **1.89**, vérifiée par le job CI `msrv` (build et tests avec 1.89.0) |
| Raison du minimum | `File::try_lock` (1.89) pour le verrou ; `quick-xml` 0.42 exige 1.86 |
| Édition | 2024 |

Politique proposée (à valider) : la MSRV peut monter dans une version
mineure avant 1.0 ; après 1.0, uniquement dans une version mineure, avec
une MSRV d'au moins six mois, et jamais dans un correctif.

## Formats persistants

| Format | Version actuelle | Lecture des versions antérieures | Changement incompatible |
|---|---|---|---|
| Configuration | `schemaVersion` 1.0 et 1.1 | Oui (1.0 lu par 1.1) | Majeure suivante ; une mineure plus récente est refusée explicitement |
| Rapport JSON | 1.0 (champs ajoutés en 1.1 optionnels) | — | Majeure |
| Flux de changements | 1.1 | — | Majeure |
| Snapshot | `ASNAPv1` | Une version inconnue est traitée comme absente (`SNAPSHOT_INVALID`), puis remplacée : jamais de faux résultat | Nouveau magic ; l'ancien snapshot coûte une génération complète |
| Manifest | 1.0 | — | Majeure |
| Sitemaps XML | Protocole sitemaps.org 0.9, extensions Google | — | Ne change pas |

## Protocoles externes vérifiés

| Spécification | Vérification |
|---|---|
| Sitemap 0.9 et sitemap index | XSD sitemaps.org (xmllint, CI) |
| Google Images | Vérificateur Python indépendant |
| Google Video 1.1 | XSD Google (xmllint, CI) |
| Google News 0.9 | XSD Google (xmllint, CI) |
| hreflang (`xhtml:link`) | Vérificateur Python, tests de réciprocité |
| robots.txt | RFC 9309, tests unitaires |
