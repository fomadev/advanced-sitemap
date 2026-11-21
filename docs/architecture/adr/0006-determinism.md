# ADR-0006 — Sortie déterministe, pas d'horloge implicite

- Statut : accepté
- Date : 2026-11-21

## Décision

- Le moteur ne lit ni l'horloge système ni l'environnement.
- Pour une même entrée et une même configuration, la sortie est identique
  octet pour octet sur toutes les plateformes (fins de ligne `\n`,
  indentation fixe, ordre d'entrée préservé).
- Les règles dépendant du temps (lastmod dans le futur aujourd'hui ;
  fraîcheur des news et expiration plus tard) utilisent une `referenceTime`
  fournie par l'appelant ; sans elle, elles ne s'exécutent pas.
- Le `lastmod` d'une entrée d'index est le maximum des `lastmod` de la
  partition (comparaison en UTC), absent si aucune entrée n'en a.

## Conséquences

Les fixtures `fixtures/conformance` peuvent être comparées octet par octet
par tous les bindings.
