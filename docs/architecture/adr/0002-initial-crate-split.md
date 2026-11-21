# ADR-0002 — Découpage initial en huit crates

- Statut : accepté
- Date : 2026-11-21

## Contexte

La cible conceptuelle compte une vingtaine de crates. Créer des crates vides
fige des frontières avant que les interfaces ne soient connues.

## Décision

Le MVP comporte huit crates, chacune avec une responsabilité et des tests
réels : `sitemap-model`, `sitemap-validation`, `sitemap-xml`,
`sitemap-storage`, `sitemap-index`, `sitemap-protocol`, `advanced-sitemap`
(façade, moteur, planificateur), `sitemap-cli`.

Une nouvelle crate est créée lorsqu'un module a (a) une interface publique
stable, (b) des dépendances propres qu'on ne veut pas imposer aux autres, ou
(c) un rythme d'évolution distinct (par exemple `sitemap-indexnow` avec un
client HTTP derrière une feature).

## Conséquences

- Le planificateur vit dans `advanced-sitemap::plan` jusqu'à stabilisation.
- Toutes les crates portent `publish = false` tant que la licence et la
  politique de publication ne sont pas décidées.
