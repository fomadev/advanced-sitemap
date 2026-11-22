# Audit de production — Lot A

- Date : 2026-11-22
- Version auditée : `main` au commit `258a2e8` (PR #15), puis branche
  `lot-a-production-audit`
- Méthode : lecture du code et des ADR, exécution des tests, revue des
  chemins d'erreur et des opérations disque, essais ciblés (verrou,
  reprise, erreurs d'E/S). Ce document décrit ce qui a été **vérifié** ;
  ce qui ne l'a pas été est dit explicitement.

## 1. État de référence vérifié

| Élément | Constat |
|---|---|
| État Git | Arbre propre, `main` à jour avec `origin/main` |
| Tests (avant le lot) | 170 réussis, 0 échec (`cargo test --workspace`, Windows 11, Rust 1.99.0) |
| Tests (après le lot) | 178 réussis, 0 échec |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` propre ; `unwrap`, `expect` et `panic` interdits hors tests |
| Format | `cargo fmt --check` propre |
| CI | Linux, Windows, macOS (`macos-latest`, ARM64), lint, conformance (xmllint + XSD officiels), supply-chain (`cargo deny`) ; un job MSRV est ajouté par ce lot |
| Code `unsafe` | Aucun ; désormais interdit par `#![forbid(unsafe_code)]` dans les 11 crates |
| Dépendances | 69 crates externes (graphe normal) ; licences contrôlées par `cargo deny` (MIT, Apache-2.0, Unicode-3.0, BSD-3-Clause, Zlib) |
| `cargo audit` | Non installé localement : non exécuté. Les avis de sécurité sont vérifiés en CI par `cargo deny` (section `advisories`) |

Taille par crate (lignes de `src`, tests `#[test]`) :

| Crate | Lignes | Tests |
|---|---|---|
| `advanced-sitemap` (façade, moteur, delta, planner) | 3 345 | 41 |
| `sitemap-model` | 2 119 | 22 |
| `sitemap-validation` | 1 459 | 16 |
| `sitemap-reader` | 1 237 | 23 |
| `sitemap-storage` | 1 187 | 12 |
| `sitemap-index` | 1 126 | 18 |
| `sitemap-cli` | 880 | 14 |
| `sitemap-protocol` | 878 | 11 |
| `sitemap-xml` | 714 | 9 |
| `sitemap-snapshot` | 702 | 7 |
| `sitemap-robots` | 395 | 5 |

## 2. Publication et durabilité

Chaîne réelle (`engine.rs`, `FsStore`) :

1. Génération en flux dans un dossier de staging caché
   (`.advanced-sitemap-staging-<pid>-<n>`) **à l'intérieur** du dossier de
   sortie, donc sur le même volume.
2. Validation et partitionnement pendant l'écriture ; une erreur bloquante
   jette tout le staging (rien n'est publié).
3. Publication dans l'ordre : sitemaps enfants, index, robots.txt,
   manifest. Pour chaque fichier : `fsync` du fichier, puis `rename`.
4. Snapshot (si `state.directory`) écrit dans `<stem>.snapshot.tmp`,
   `fsync`, comparé à l'ancien, puis renommé **après** la publication.

Garanties vérifiées :

- **Par fichier** : un lecteur voit l'ancien ou le nouveau fichier, jamais
  un fichier partiel (renommage sur le même volume).
- **Par ensemble : pas d'atomicité.** Une panne au milieu de l'étape 3
  laisse un mélange d'anciens et de nouveaux fichiers. Chaque fichier
  reste un sitemap valide, l'ancien index ne référence que des fichiers
  existants (rien n'est jamais supprimé), et l'exécution suivante converge
  (testé par injection de pannes, `tests/incremental.rs`). Mais pendant
  cette fenêtre, une URL peut apparaître dans deux fichiers ou dans aucun.
  C'est documenté depuis l'ADR-0007 ; une publication atomique d'ensemble
  demande un changement d'architecture (voir section 6).
- **Renommages durables** : avant ce lot, les données étaient synchronisées
  mais pas le répertoire. Sous Linux, une coupure de courant pouvait perdre
  un renommage. **Corrigé** : `fsync` du dossier après les renommages et
  après celui du snapshot (Unix). Sous Windows, `std::fs::rename` n'offre
  pas d'équivalent ; la durabilité du renommage y dépend du système de
  fichiers (NTFS journalise les métadonnées).
- **Résistance réelle à une coupure électrique : non vérifiée.** Aucun
  test ne coupe l'alimentation ; les garanties ci-dessus reposent sur la
  sémantique documentée de `fsync` et `rename`.

## 3. Concurrence

Avant ce lot, **aucun verrou** : deux processus pouvaient publier dans le
même dossier en même temps.

| Scénario | Avant | Après |
|---|---|---|
| Deux `generate` sur le même dossier | Publications entrelacées ; manifest de l'un, snapshot de l'autre | Le second échoue immédiatement (`EngineError::Busy`, code 3, `"kind": "busy"`), sans rien lire ni écrire |
| `generate` et `apply` simultanés | Idem ; `apply` pouvait relire un fichier en cours de remplacement | Idem |
| Deux `apply` sur le même snapshot | **Perte silencieuse** possible des opérations d'un des flux | Le second est refusé |
| Deux dossiers de sortie, même `state.directory` et même `fileStem` | Les deux écrivaient `<stem>.snapshot.tmp` | Verrou `<state>/<stem>.lock` |
| Processus tué en tenant le verrou | — | Le verrou du système est libéré à la mort du processus : aucun verrou périmé |
| `inspect` ou `diff` pendant une publication | Lecture fichier par fichier, chacun ancien ou nouveau | Inchangé (lecteurs, pas de verrou) ; un ensemble lu pendant une publication peut être mélangé |

Limites du verrou : verrou consultatif (seuls les processus Advanced
Sitemap sont exclus) ; sous Windows, le fichier de verrou ne peut pas être
lu pendant une exécution (verrou obligatoire) : un outil qui copie le
dossier de sortie pendant une génération échouera sur
`.advanced-sitemap.lock`. Il est vide et doit être exclu des déploiements.

## 4. Reprise après interruption

| Scénario | Comportement constaté | Preuve |
|---|---|---|
| Interruption pendant la génération ou le staging | Rien n'est publié ; le dossier de staging restait indéfiniment. **Corrigé** : supprimé par l'exécution suivante (sous verrou) | `leftovers_of_an_interrupted_run_do_not_block_the_next_one` |
| Interruption au milieu de la publication | Ensemble mélangé, valide fichier par fichier ; converge à l'exécution suivante | `interrupted_publication_converges_on_the_next_run` (injection de pannes à chaque étape) |
| Interruption pendant l'écriture du snapshot | `<stem>.snapshot.tmp` résiduel, écrasé ensuite ; l'ancien snapshot reste valide | idem (le test laisse un `.tmp` tronqué) |
| Publication faite, snapshot non remplacé | Le rapport de changements suivant compte trop de changements ; `apply` refuse toute partition modifiée | `a_snapshot_older_than_the_published_files_makes_apply_refuse` |
| Snapshot endommagé | Détecté (compteur, somme BLAKE3), `SNAPSHOT_INVALID`, ignoré et remplacé | `snapshot_follows_successful_publications_only` |
| Fichier publié modifié hors du moteur | `apply` refuse | `delta_is_refused_when_it_cannot_match_a_full_generation` |
| Dossier de sortie inutilisable (erreur d'E/S) | Erreur d'E/S, rien publié | `an_unusable_output_directory_fails_without_publishing` |
| Disque plein | Non testé réellement. Une erreur d'écriture pendant le staging jette le staging ; pendant les renommages, elle laisse un ensemble mélangé comme ci-dessus | Raisonnement sur le code |

## 5. Intégrité des formats persistants

| Format | Versionné | Contrôle d'intégrité |
|---|---|---|
| Snapshot | `ASNAPv1\n` | Ordre, compteur, BLAKE3-256 ; empreinte des réglages (I7) |
| Manifest | `schemaVersion` | Aucun checksum ; relu seulement pour `STALE_ARTIFACT` et par `apply`, qui le recoupe avec le snapshot |
| Configuration, rapport, flux | `schemaVersion` (1.0, 1.1) | Schémas JSON ; mineure plus récente refusée |
| Sitemaps publiés | Format XML standard | Relus et vérifiés entrée par entrée par `apply` (empreinte de séquence) |

Point faible (P2) : rien ne relie le snapshot à la publication qu'il
décrit. Une publication interrompue avant le remplacement du snapshot
n'est détectée par `apply` que sur les partitions qu'il touche. Les
partitions non touchées gardent alors des empreintes périmées dans le
nouveau snapshot ; une sortie fausse est impossible (les fichiers publiés
restent la source), mais le rapport de changements peut être inexact
jusqu'à la génération complète suivante.

## 6. Décisions architecturales ouvertes

1. **Publication atomique d'ensemble** : révisions immuables (dossier
   `rev-<n>/`) et pointeur de publication (fichier ou lien), ou journal de
   reprise. Change la forme du dossier publié (les URL publiques des
   sitemaps ou un redirect) : décision du propriétaire du projet.
2. **ADR-0010 à 0013** encore au statut « proposé » alors que le code les
   implémente. L'ADR-0011 fixe BLAKE3 dans le format du snapshot : à
   approuver avant de déclarer ce format stable.
3. **Licence FPL** : clarifications 1.1 et titulaire des droits à
   confirmer avant toute distribution.
4. **MSRV** : relevée à 1.89 par ce lot (voir ci-dessous) ; à confirmer.

## 7. Corrections faites dans ce lot

| Problème | Priorité | Correction |
|---|---|---|
| Aucun verrou entre processus ; perte possible d'un flux `apply` | P1 | Verrou du dossier de sortie et du dossier d'état, `EngineError::Busy` |
| Renommages non synchronisés (Unix) | P2 | `fsync` du dossier après publication et après le snapshot |
| Dossiers de staging orphelins après un crash | P2 | Supprimés sous verrou par l'exécution suivante |
| MSRV déclarée (1.85) jamais vérifiée et fausse (`quick-xml` 0.42 exige 1.86) | P2 | MSRV 1.89, job CI qui compile et teste avec 1.89.0 |
| Absence d'interdiction explicite du code `unsafe` | P3 | `#![forbid(unsafe_code)]` partout |
| README présentant comme « prévues » des fonctions livrées | P2 | README corrigé |

Aucun P0 (perte ou corruption de données, publication incorrecte d'un
fichier, faille critique) n'a été trouvé. Les problèmes ouverts sont dans
le [registre des risques](risk-register.md).
