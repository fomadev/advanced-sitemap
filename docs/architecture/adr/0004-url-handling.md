# ADR-0004 — Traitement des URL

- Statut : accepté
- Date : 2026-11-21

## Décision

- Analyse selon le standard WHATWG (crate `url`), schémas `http` et `https`
  uniquement, hôte obligatoire, identifiants (`user:pass@`) refusés.
- L'entrée brute ne doit contenir ni espace ni caractère de contrôle
  (sinon `URL_INVALID`) : le moteur ne répare pas silencieusement une URL.
- La forme émise est la sérialisation WHATWG : ASCII, hôte en minuscules et
  IDNA, percent-encodage des caractères non ASCII, port par défaut retiré,
  `/` ajouté à un chemin vide. Ces transformations ne fusionnent jamais deux
  ressources distinctes.
- Aucune canonicalisation supplémentaire implicite : la casse du chemin, le
  slash final, l'ordre et le contenu de la requête sont préservés. Une
  canonicalisation configurable (slash final, etc.) viendra plus tard,
  désactivée par défaut.
- Longueur : strictement moins de 2 048 caractères (2 047 au plus, selon
  sitemaps.org), mesurée sur la forme émise.
- Fragment (`#...`) : avertissement `URL_HAS_FRAGMENT`.
- La déduplication porte sur la forme émise.
