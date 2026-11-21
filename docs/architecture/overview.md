# Architecture du moteur

## Principe

Un seul moteur de référence en Rust ; plusieurs interfaces (crate Rust, CLI,
protocole JSON, bindings natifs à venir, service HTTP éventuel) ; un
comportement identique sur toutes les plateformes, vérifié par des fixtures
partagées.

## Crates (état du MVP)

```
crates/
  sitemap-model        Types validés (URL, dates W3C, priorité, hreflang, entrées),
                       entrées brutes, diagnostics, limites du protocole. Dépend de `url`.
  sitemap-validation   Conversion entrée brute -> entrée typée + diagnostics (profils standard/strict).
  sitemap-xml          Sérialisation XML en flux (urlset, sitemapindex), échappement sûr.
  sitemap-storage      Abstraction de destination : staging, publication ordonnée, abandon.
                       Implémentations : système de fichiers local, mémoire.
  sitemap-index        Partitionnement (séquentiel exact, hachage stable FNV-1a) et index.
  sitemap-protocol     Documents JSON versionnés (configuration v1), lecteurs JSON/JSONL bornés.
  sitemap-reader       Lecture bornée de sitemaps existants (XML ou gzip), base de `inspect`.
  sitemap-robots       robots.txt validé et déterministe (RFC 9309).
  advanced-sitemap     Façade publique : moteur (pipeline), API `Sitemap::builder()`, `explain`.
  sitemap-cli          Binaire `advanced-sitemap` (generate, validate, explain).
```

Graphe de dépendances (aucun cycle) :

```
sitemap-model ◀── sitemap-validation ◀──┐
      ▲   ▲                              │
      │   └── sitemap-xml ◀── sitemap-index ◀── advanced-sitemap ◀── sitemap-cli
      │                          ▲                 ▲
      └── sitemap-protocol       sitemap-storage ──┘
              ▲                                    │
              └────────────────────────────────────┘
```

Les crates de la cible conceptuelle non encore créées (`sitemap-lifecycle`,
`sitemap-planner`, `sitemap-indexnow`, `sitemap-plugin-api`, `sitemap-ffi`,
bindings) le seront quand leur interface sera stable. Les extensions vidéo
et news vivent dans `sitemap-model` et `sitemap-validation`, la lecture dans
`sitemap-reader`, robots.txt dans `sitemap-robots`. Le planificateur vit pour
l'instant dans `advanced-sitemap::plan`.

La conception du Lifecycle Engine (snapshot, diff, génération incrémentale,
partitionnement stable, planner, explain) est dans
[lifecycle/](lifecycle/README.md), avec ses invariants et ses phases.

## Pipeline

```
Source (itérateur d'items)              JSON en mémoire, JSONL en flux, itérateur Rust
  │  Item = (Location, RawUrlRecord) ou Diagnostic de décodage
  ▼
Validator                               RawUrlRecord -> Option<UrlEntry> + diagnostics
  ▼
Déduplication (empreinte 128 bits)      DUPLICATE_URL, première occurrence conservée
  ▼
Partitioner                             encode le bloc <url> dans un tampon, mesure, route
  ▼
UrlsetWriter -> ArtifactWriter          fichiers en staging
  ▼
Fin de flux : index (si nécessaire), manifeste
  ▼
Publication ordonnée (partitions, index, manifeste) ou abandon
```

Politique d'échec (`onInvalid`) :

- `fail` (défaut) : à la première erreur bloquante, plus rien n'est écrit,
  mais la validation continue jusqu'au bout pour produire un rapport
  complet ; le staging est abandonné, aucun fichier publié.
- `skip` : les entrées invalides sont exclues et signalées ; les autres sont
  publiées.

Les erreurs structurelles (partition pleine en hachage stable, entrée trop
grosse, index trop grand, URL publique manquante) font toujours échouer la
génération.

## Contrats publics

| Contrat | Emplacement | Versionnement |
|---|---|---|
| API Rust | `advanced-sitemap` | SemVer (0.x pendant le MVP) |
| Document de configuration | `schemas/config.v1.schema.json`, `docs/protocol/` | `schemaVersion` |
| Enregistrement d'URL | `schemas/url-record.v1.schema.json` | `schemaVersion` |
| Rapport | `schemas/report.v1.schema.json` | champ `schemaVersion` du rapport |
| Codes de diagnostic | `docs/concepts/diagnostics.md` | ajout = mineur, retrait = majeur |
| Codes de sortie CLI | `docs/cli/README.md` | idem |
| Format de sortie XML | fixtures `fixtures/conformance` | sérialisation déterministe garantie |

## Décisions

Voir `docs/architecture/adr/`.
