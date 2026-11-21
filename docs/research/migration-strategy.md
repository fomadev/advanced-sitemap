# Stratégie de migration

Objectif : faire de `advanced-sitemap` la référence technique, puis faire
évoluer `next-advanced-sitemap` vers un adaptateur, **sans casser ses
utilisateurs**. Aucun dépôt existant n'est modifié pendant les phases 0 à 7.

## Étapes

1. **Moteur Rust + CLI** (phases 2 à 5) : le moteur produit des sitemaps
   valides depuis JSON/JSONL. Les fixtures de conformité sont figées.
2. **Binding Node** (phase 6) : package `@fomadev/advanced-sitemap` (nom à
   confirmer) exposant `generate`, `validate`, `explain` et un writer en flux.
   Exemple d'utilisation depuis une route App Router, hors dépôt Next.
3. **Shadow mode** dans `next-advanced-sitemap` (après autorisation) : une
   option expérimentale génère via le moteur et compare au résultat
   TypeScript sur les fixtures de la suite Vitest existante. Les écarts
   attendus sont listés ci-dessous ; tout autre écart est un bug.
4. **Bascule** en version majeure de `next-advanced-sitemap` : l'API publique
   (`getServerSitemapResponse`, etc.) est conservée, son implémentation
   délègue au moteur. Le scanner App Router et les en-têtes HTTP restent dans
   le package.
5. **CLI, Cloud, backend** : remplacement progressif des lecteurs regex par
   `inspect`.

## Écarts attendus entre l'implémentation TS et le moteur

| Sujet | TS actuel | Moteur | Action d'adaptateur |
|---|---|---|---|
| Plus de 50 000 URL | Exception | Partitions + index | Exposer le résultat multi-fichiers |
| Erreur de validation | Exception à la 1re | Rapport complet | Convertir en exception si l'API l'exige |
| `priority: 0.75` | `0.8` | `0.75` | Aucune (correction) |
| `priority: 1.5` | `1.5` émis | Erreur `PRIORITY_OUT_OF_RANGE` | Documenter |
| `lastmod: "hier"` | Émis tel quel | Erreur `LASTMOD_INVALID` | Documenter |
| `autoLastmod` | Heure courante | Non implicite | L'adaptateur passe la date |
| `isDeleted` / `expiresAt` | Entrée conservée + commentaire | Non repris | L'adaptateur filtre ; recommandation : retirer l'URL et répondre 404/410 |
| Namespaces | Toujours les 5 | Selon usage | Aucune |
| URL non ASCII | Émise brute | Percent-encodée | Aucune (correction) |
| Champs image retirés par Google | Émis | Refusés | Avertir dans le changelog de l'adaptateur |

## Risques

- Distribution des binaires natifs sur Vercel/Edge : le runtime Edge ne
  charge pas d'addon natif. Il faudra soit garder un chemin TypeScript pour
  Edge, soit envisager une cible WASM du moteur (à évaluer en phase 6).
- Taille des binaires prébuilt par plateforme pour npm (packages
  optionnels par plateforme, à la manière des outils natifs connus).
- Licence du nouveau dépôt à décider avant toute publication (voir
  `licensing-review.md`).
