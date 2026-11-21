# ADR-0008 — Politique de dépendances

- Statut : accepté
- Date : 2026-11-21

| Dépendance | Usage | Justification |
|---|---|---|
| `url` | Analyse WHATWG | Standard de fait, implémente IDNA et le percent-encodage ; le réécrire serait risqué |
| `serde`, `serde_json` | Protocole JSON | Standard de l'écosystème, derive fiable |
| `clap` | CLI uniquement | Analyse d'arguments, aide et erreurs cohérentes |
| `criterion` | Benchmarks (dev) | Mesures statistiques reproductibles |
| `quick-xml` | Tests (dev) | Vérifier la bonne formation de la sortie avec un parseur indépendant |

Écrits dans le dépôt (sans dépendance) : échappement XML (contrôle total des
caractères interdits), parseur W3C Datetime (format borné), FNV-1a 64 (doit
rester figé), validation syntaxique réduite des codes hreflang.

Pas de `thiserror`, `anyhow`, `chrono` ni `tokio` dans le MVP : aucune
fonctionnalité ne les exige encore. Le moteur est synchrone ; une couche
asynchrone viendra avec les sources réseau.
