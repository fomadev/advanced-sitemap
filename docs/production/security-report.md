# Rapport de sécurité — lot D

- Date : 2026-11-22
- Portée : lecteur XML et gzip, protocole JSON (configuration, enregistrements,
  flux de changements), décodeur de snapshot, validation (URL, dates,
  langues, textes des extensions), robots.txt, fichiers et dépendances.
- Méthode : fuzzing reproductible sur Rust stable (mutation d'entrées
  valides, graine fixe), fuzzing guidé par la couverture (`cargo fuzz`) en
  CI, tests adversariaux ciblés, revue du code.

## 1. Défaut trouvé et corrigé

| Défaut | Gravité | Trouvé par | Correction |
|---|---|---|---|
| Un caractère interdit par XML 1.0 (U+0000 à U+001F sauf tabulation et fins de ligne, U+FFFE, U+FFFF) dans un titre ou un nom de publication news, ou dans un titre, une description, un mot-clé ou un nom d'auteur vidéo passait la validation, puis faisait échouer l'écriture XML : l'exécution s'arrêtait sur une erreur interne (code 4) au lieu d'un diagnostic | P1 (une seule entrée suffit à bloquer toute une génération) | `fuzz_url_records`, graines 1 et 3, après 90 604 et 136 364 itérations | Règle unique partagée par la validation et l'écrivain (`sitemap_model::xml_forbidden_char`) ; diagnostics `NEWS_TEXT_INVALID`, `VIDEO_TEXT_INVALID`, `VIDEO_UPLOADER_INVALID` ; test de régression |

Aucun autre défaut : pas de panique, pas de boucle, pas de lecture au-delà
des limites, pas de snapshot endommagé accepté.

## 2. Campagnes de fuzzing

### Rust stable (`cargo test`, reproductible)

`crates/advanced-sitemap/tests/fuzz.rs` et le test `fuzz_snapshot_decoder`
de `sitemap-snapshot`. Chaque cible mute des entrées valides (inversion de
bits, suppression, insertion de jetons piégés : `<!DOCTYPE`, entités,
`]]>`, octets invalides, nombres extrêmes, URL avec identifiants, dates
impossibles…). Variables `FUZZ_ITERATIONS` et `FUZZ_SEED` ; une entrée qui
fait échouer un test est enregistrée pour être rejouée.

| Cible | Propriétés vérifiées | Campagne locale |
|---|---|---|
| Lecteur XML | Pas de panique ; limite d'octets respectée ; toute entrée acceptée se réencode et se relit à l'identique | 8 graines × 300 000 |
| Lecteur gzip | Idem, sur des flux gzip mutés | 8 × 300 000 |
| Enregistrements d'URL (décodage, validation standard et stricte, encodage) | Un refus porte un diagnostic d'erreur ; une entrée validée s'encode et se relit à l'identique | 8 × 300 000 (défaut trouvé) |
| Flux de changements | Pas de panique | 8 × 300 000 |
| Documents de configuration (lecture et résolution) | Pas de panique | 8 × 300 000 |
| URL, dates W3C, codes de langue | Pas de panique ; la forme normalisée se relit à l'identique ; URL ≤ 2 047 caractères | 8 × 300 000 |
| robots.txt | Pas de panique | 8 × 300 000 |
| Décodeur de snapshot et diff | Pas de panique ; un fichier modifié n'est jamais accepté avec d'autres enregistrements | 4 × 200 000 |

En CI : 3 000 itérations par cible dans `test` (trois systèmes) et 100 000
dans le job `fuzz-stable`, à chaque pull request.

### Guidé par la couverture (`cargo fuzz`)

Six cibles dans `fuzz/` (lecteur XML, enregistrements d'URL, flux,
configuration, snapshot, scalaires) avec des corpus de départ versionnés
dans `fuzz/corpus/`. Le workflow `fuzz` les exécute chaque semaine
(5 minutes par cible), à la demande, et 60 secondes par cible quand le code
fuzzé change. Une entrée qui fait planter une cible est publiée comme
artefact du job. Ces cibles demandent Rust nightly et libFuzzer : elles ne
sont pas exécutées sur la machine de développement.

Première campagne (CI, 2026-11-22, 60 secondes par cible, depuis les
corpus de départ), sans aucun plantage :

| Cible | Exécutions | Arêtes couvertes | Corpus final |
|---|---|---|---|
| `change_feed` | 1 374 026 | 1 252 | 1 925 |
| `config` | 2 799 169 | 3 866 | 2 254 |
| `scalars` | 1 600 899 | 2 408 | 1 775 |
| `snapshot` | 536 687 | 412 | 78 |
| `url_record` | 1 064 901 | 5 709 | 2 740 |
| `xml_reader` | 480 754 | 5 036 | 1 658 |

Les corpus enrichis par une campagne ne sont pas réintégrés
automatiquement au dépôt : seules les entrées qui font échouer une cible
doivent l'être, comme tests de régression.

## 3. Attaques XML

| Attaque | Comportement | Preuve |
|---|---|---|
| XXE, entités externes | Tout `DOCTYPE` est refusé (`XML_DOCTYPE_FORBIDDEN`) : aucune DTD n'est lue | Tests du lecteur ; jeton `<!DOCTYPE … <!ENTITY` injecté par le fuzzing |
| Expansion d'entités, « billion laughs » | Impossible sans DTD | Idem |
| Bombe gzip | Lecture arrêtée à la limite après décompression | `decompression_bombs_stop_at_the_limit` (64 Mio de zéros, limite 1 Mio) |
| XML mal formé, tronqué | Diagnostic, lecture arrêtée, jamais de panique | Fuzzing |
| Encodage autre qu'UTF-8, UTF-8 invalide | Refusé | Tests du lecteur, fuzzing |
| Espaces de noms incorrects | Racine refusée ou éléments ignorés et comptés | Tests du lecteur |
| Imbrication, textes géants | Profondeur ≤ 32, texte ≤ 64 Kio | Tests du lecteur |

## 4. Fichiers

| Risque | Comportement |
|---|---|
| Traversée de chemin dans les noms publiés | Impossible : noms limités à `[A-Za-z0-9._-]`, 128 caractères, sans point initial |
| Chemins de la configuration (source, sortie, état) | Relatifs au fichier de configuration ; absolus acceptés. La CLI fait confiance à son opérateur : ne pas exécuter une configuration non relue (voir le modèle de sécurité) |
| Lien symbolique dans le dossier de sortie | La publication remplace l'entrée du dossier (le lien), jamais sa cible. `apply` et `--skip-unchanged` lisent les fichiers publiés en suivant les liens : un lien vers un autre fichier fait au pire échouer la vérification d'`apply` (refus) ou réécrire un fichier |
| Permissions insuffisantes, dossier inutilisable | Erreur d'E/S, rien n'est publié (test `an_unusable_output_directory_fails_without_publishing`) |
| Remplacement concurrent | Verrou exclusif sur le dossier de sortie et le dossier d'état (lot A) |

## 5. Chaîne d'approvisionnement

| Contrôle | État |
|---|---|
| Avis de sécurité (base RustSec) | `cargo deny check advisories` en CI à chaque pull request : même base que `cargo audit`, qui n'est donc pas ajouté en double |
| Licences | `cargo deny check licenses` : MIT, Apache-2.0, Unicode-3.0, BSD-3-Clause, Zlib uniquement |
| Sources | `cargo deny check sources` : crates.io seulement |
| Versions | `Cargo.lock` versionné, `--locked` partout en CI |
| Actions GitHub | Épinglées par empreinte de commit ; Dependabot propose les mises à jour |
| Permissions des workflows | `contents: read` uniquement |
| Code `unsafe` | Interdit dans tout l'espace de travail ; seul l'outil de mesure `tools/alloc-profile` (hors espace de travail, non livré) en utilise pour compter les allocations |
| Dépendances inutilisées | Non vérifié (aucun outil installé, pas de revue dédiée) ; `cargo deny check bans` signale seulement les doublons de versions |

## 6. Limites de ce rapport

- Le fuzzing guidé par la couverture ne tourne qu'en CI (Linux). Aucun
  assainisseur mémoire n'est utile côté Rust sans code `unsafe`, mais les
  dépendances en contiennent : leur sûreté repose sur leurs propres audits.
- La résistance à une coupure électrique n'est pas testée par un banc ; les arrêts brutaux du processus le sont depuis le lot F (section 7).
- Le chemin `apply` complet (relecture d'une partition publiée puis
  réécriture) est couvert par l'oracle aléatoire du lot B, pas par le
  fuzzing de fichiers publiés corrompus au-delà du lecteur.

## 7. Complément du lot F

| Point | Constat |
|---|---|
| Arrêt brutal pendant `generate` | Test `killing_generate_never_leaves_a_damaged_file` : 12 arrêts répartis de 5 % à 130 % de la durée mesurée d'une exécution, avec deux entrées alternées pour que chaque fichier change. Après chaque arrêt, tout sitemap publié est un document complet et valide ; un arrêt pendant la publication laisse un ensemble mixte que `doctor` signale, et l'exécution suivante répare tout |
| Fichier tenu ouvert sous Windows (R8) | Reproduit : un fichier ouvert sans partage de suppression ne peut pas être remplacé (« accès refusé »). Le remplacement est maintenant réessayé environ 2,3 s ; au-delà, l'erreur nomme le fichier et dit combien ont été remplacés. Testé au niveau du stockage (blocage bref et permanent) et du moteur (échec, `doctor`, réparation) |
| Contrôle d'intégrité en exploitation | `doctor` compare chaque fichier publié au manifeste (taille et empreinte BLAKE3) sans rien écrire |
| Artefacts de la version candidate | Construits par la CI depuis le commit (`--locked`), archives aux membres horodatés de façon fixe, `SHA256SUMS`. Contenu : binaire, `LICENSE`, `THIRD-PARTY-NOTICES.txt`, `README.md`, `CHANGELOG.md`, `BUILDINFO.txt` ; aucun secret (le workflow n'a que `contents: read` et aucun jeton de publication). Les binaires contiennent les tables de lignes de débogage, donc des chemins de construction de la CI |
| Signature des artefacts | Non faite : seules les sommes SHA-256 sont produites. Une signature (Sigstore, GPG) relève de la décision de publication |
