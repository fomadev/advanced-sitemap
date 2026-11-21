# Opportunités d'architecture

## 1. Séparer validation et sérialisation

Aujourd'hui, chaque builder valide **et** écrit. Séparer les deux permet :
une commande `validate` sans écriture, des diagnostics complets plutôt que la
première exception, et des builders triviaux à auditer. Dans le moteur, une
entrée brute (`RawUrlRecord`, chaînes non vérifiées) est convertie en entrée
typée (`UrlEntry`) par la validation ; le writer XML n'accepte que des
entrées typées. Une entrée typée est valide par construction.

## 2. Le moteur comme pipeline de flux

```
Source ─▶ Décodage ─▶ Validation ─▶ Déduplication ─▶ Partitionnement ─▶ Writer XML ─▶ Store (staging) ─▶ Publication ─▶ Index + manifeste
                          │                                │
                          └────────── Diagnostics ◀────────┘
```

Chaque étape traite une entrée à la fois ; seules la déduplication
(ensemble d'empreintes 128 bits, ~16 octets/URL) et les compteurs croissent
avec le volume.

## 3. Écriture exacte plutôt qu'estimation

Comme chaque bloc `<url>` est sérialisé dans un tampon avant écriture, la
taille réelle est connue : le partitionnement n'a besoin d'aucune
estimation. Le planificateur (`explain`) n'estime que ce qui n'est pas encore
lu (nombre d'URL d'un fichier JSONL à partir d'un échantillon), et le
signale comme estimation.

## 4. Publication en deux temps

Écrire dans un répertoire de staging du même volume, puis renommer chaque
fichier, index en dernier. Un crawler ne voit jamais un index qui pointe vers
un fichier absent. Les anciens artefacts ne sont jamais supprimés
automatiquement : ils sont signalés (`STALE_ARTIFACT`) par comparaison avec
le manifeste précédent.

## 5. Un inspecteur unique

Les trois lecteurs de sitemaps actuels (regex ×2, `fast-xml-parser`) peuvent
être remplacés par `advanced-sitemap inspect`, en flux, borné en taille,
avec les mêmes codes que la génération. Le backend Cloud pourrait l'appeler
via le binding Node.

## 6. Horloge et environnement injectés

Le moteur ne lit ni l'horloge ni les variables d'environnement. Cela rend
la sortie reproductible octet pour octet et permet des fixtures partagées
entre Rust, Node, Python et CLI.

## 7. Bindings minces

Le contrat polyglotte stable est le **protocole JSON** (documents et
rapports). Les bindings natifs (N-API, PyO3) exposent `generate(config) ->
report` et un mode flux, en sérialisant aux frontières : aucune logique
métier dans les bindings. L'ABI Rust n'est jamais exposée.

## 8. Extensions typées avant plugins dynamiques

Sources, validateurs, stratégies de partitionnement et stores sont des
traits Rust. Les plugins dynamiques (WASM ou processus isolé parlant le
protocole JSON) viendront après stabilisation de ces traits.

## 9. Ce qui reste hors du moteur

En-têtes HTTP, objets `Response`, conventions de routes, scanner de
framework, interface utilisateur, monitoring planifié, alertes, multi-tenant
hébergé : adaptateurs et produits.
