# ADR-0003 — Diagnostics structurés plutôt qu'exceptions

- Statut : accepté
- Date : 2026-11-21

## Contexte

L'implémentation TypeScript lève une exception textuelle à la première
violation. Le backend Cloud utilise déjà des codes stables
(`INVALID_LASTMOD`, ...).

## Décision

- La validation produit des `Diagnostic { code, severity, message, location,
  value, suggestion }`. `code` est un identifiant SCREAMING_SNAKE_CASE
  stable, documenté dans `docs/concepts/diagnostics.md`.
- Quatre sévérités : `error` (bloquante), `warning`, `recommendation`,
  `info`.
- Le profil `strict` promeut certaines règles (HTTPS, hôte, champs requis)
  au rang d'erreur ; un profil n'altère jamais les données.
- Le nombre de diagnostics conservés est plafonné (`maxDiagnostics`, défaut
  1 000) ; les compteurs par code restent exacts.
- Les erreurs d'entrée/sortie et de configuration sont des erreurs Rust
  (`Result`), pas des diagnostics.

## Conséquences

Un utilisateur voit toutes les erreurs d'un jeu de données en un passage.
Les codes deviennent un contrat versionné.
