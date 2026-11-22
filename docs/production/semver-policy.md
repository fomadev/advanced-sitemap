# Politique de compatibilité (SemVer)

Ce document dit ce qui est garanti d'une version à l'autre, et comment
c'est vérifié. Il s'applique à partir de la première version publiée ;
aucune ne l'est encore (`publish = false` partout).

## 1. Ce qui est couvert

| Surface | Contrat |
|---|---|
| API Rust de la crate `advanced-sitemap` | Tout élément public, y compris les modules réexportés `protocol`, `reader` et `xml` |
| CLI `advanced-sitemap` | Noms des commandes et des options, codes de sortie 0 à 4, sortie JSON (`--format json`), valeurs de `error.kind` |
| Documents du protocole | Configuration v1, enregistrement d'URL, enregistrement de changement, rapport v1 (`schemaVersion` `MAJEUR.MINEUR`, schémas dans `schemas/`) |
| Codes de diagnostic | Le nom (`DUPLICATE_URL`...) et son sens ; la gravité par défaut |
| Formats persistés | Snapshot (v1 lu, v2 écrit), manifeste 1.0, nom du fichier verrou, préfixe du staging |

Ne sont **pas** couverts :

- les crates internes `sitemap-*` utilisées directement : elles sont
  versionnées ensemble et peuvent changer sans préavis. Passez par
  `advanced-sitemap` ;
- la sortie humaine de la CLI (`--format human`) et le texte des messages,
  y compris dans le JSON (le champ `message`). Les programmes lisent
  `code`, `kind` et les champs structurés ;
- les performances et les estimations du planner (`CostModel::REFERENCE`
  est recalibré quand les mesures changent) ;
- les octets exacts produits par une **autre** version du moteur : la
  sortie est déterministe pour une même version, une même entrée et les
  mêmes réglages. Une nouvelle version peut changer les octets ; le
  CHANGELOG le signale.

## 2. Règles de version

Avant 1.0 (versions `0.y.z`) :

- un changement incompatible incrémente `y` (0.1 → 0.2) et figure dans le
  CHANGELOG sous **Breaking** avec la migration à faire ;
- tout le reste incrémente `z`.

À partir de 1.0 : SemVer standard. Un changement incompatible demande une
version majeure.

Un élément destiné à disparaître reçoit d'abord `#[deprecated]` (ou une
mention « déprécié » dans la documentation de la CLI et du protocole)
pendant au moins une version mineure, avec son remplaçant.

## 3. Ce qui n'est pas un changement incompatible

- Ajouter une variante à une énumération ou un champ à une structure
  marquées `#[non_exhaustive]` (presque tous les types publics, voir
  section 4).
- Ajouter une fonction, une méthode, un type, un module.
- Ajouter un code de diagnostic, un `error.kind`, une commande, une option.
- Ajouter un champ facultatif à un document JSON, en entrée comme en
  sortie. Les lecteurs de rapports doivent ignorer les champs inconnus.
- Passer un document du protocole de 1.0 à 1.1 (version mineure) : un
  moteur plus ancien le refuse explicitement plutôt que d'en ignorer une
  partie.
- Corriger un défaut, même si la sortie change pour une entrée qui était
  mal traitée.

## 4. Types exhaustifs et non exhaustifs

Les types publics à champs publics et les énumérations qui peuvent
grandir sont `#[non_exhaustive]`. Conséquences pour le code appelant :

- une structure se construit par son constructeur (`UrlEntry::new`,
  `Video::new`, `Layout::new`, `SourceRecord::new`...) ou par
  `Default::default()` suivi d'affectations de champs ; la syntaxe
  `Type { champ, ..Default::default() }` n'est pas disponible hors de la
  crate ;
- un `match` sur une énumération se termine par un bras `_`.

Exceptions, gardées exhaustives parce que la spécification fixe leurs
valeurs (en ajouter serait un changement incompatible) :

| Type | Raison |
|---|---|
| `ChangeFreq` | Valeurs du protocole sitemaps.org |
| `DateTimePrecision` | Précisions du profil W3C Datetime |
| `video::Relationship`, `video::Platform` | Valeurs de l'extension vidéo |
| `Severity` | Gravités du rapport v1 |
| `IndexMode` | `auto`, `always` et `never` couvrent toutes les politiques |

Les lints clippy `exhaustive_enums` et `exhaustive_structs` sont actifs
dans le workspace : un nouveau type public exhaustif fait échouer la CI,
sauf exception justifiée par un `#[allow(..., reason = "...")]`.

## 5. Erreurs

- `EngineError::kind()` renvoie un nom stable (`config`, `io`,
  `internal`, `delta-refused`, `busy`, `state-changed`) qui détermine le
  code de sortie de la CLI. Un nouveau nom peut apparaître ; un nom
  existant ne change ni de sens ni de code.
- `EngineError::is_retryable()` dit si relancer à l'identique peut réussir.
- Les erreurs qui enveloppent une autre erreur l'exposent par `source()`.
- Le texte de `Display` n'est pas un contrat.

## 6. Formats persistés

- **Snapshot** : un nouveau moteur lit toutes les versions de format
  écrites par les versions précédentes (v1 et v2 aujourd'hui, avec un
  fichier v1 figé dans les tests). Abandonner la lecture d'un format est un
  changement incompatible.
- **Changement de version du moteur** : la version du moteur fait partie
  des réglages qui façonnent la sortie (invariant I7). Après une mise à
  jour, le premier `generate` réécrit tout, et `apply` est refusé jusqu'à
  ce `generate` (`DeltaRefused`). C'est voulu : une nouvelle version peut
  encoder différemment.
- **Manifeste** : nouveaux champs facultatifs seulement en 1.x ; un
  manifeste sans empreintes (antérieur au lot B) reste accepté.

## 7. Version minimale de Rust (MSRV)

MSRV actuelle : **1.89** (`rust-version`, vérifiée par le job CI `msrv`).
Proposition, en attente de décision : relever la MSRV est permis dans une
version mineure (jamais dans un correctif), annoncé dans le CHANGELOG, et
seulement vers une version de Rust publiée depuis au moins six mois.

## 8. Vérification

| Contrôle | Où |
|---|---|
| Types publics non exhaustifs | clippy `exhaustive_enums` / `exhaustive_structs`, job `lint` |
| Codes de sortie et sortie JSON de la CLI | `crates/sitemap-cli/tests/cli.rs` |
| Noms et gravités des codes de diagnostic | liste figée `crates/sitemap-model/tests/stable_codes.txt`, et chaque code doit être documenté |
| `error.kind` | `crates/advanced-sitemap/tests/errors.rs` |
| Lecture des anciens snapshots | fichier v1 figé, `crates/sitemap-snapshot/tests` |
| Schémas JSON | fixtures de conformité, job `conformance` |

Limite : aucune comparaison automatique d'API entre deux versions
(`cargo semver-checks`) n'est possible tant qu'aucune version n'est
publiée, faute de référence. Elle sera ajoutée à la CI dès la première
version (lot F).
