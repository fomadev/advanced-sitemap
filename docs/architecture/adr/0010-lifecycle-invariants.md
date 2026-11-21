# ADR-0010 — Invariants du Lifecycle Engine et découpage en phases

- Statut : proposé (à valider avant toute implémentation)
- Date : 2026-11-21

## Contexte

Le moteur régénère et réécrit tout à chaque exécution. À 5 M d'URL,
`generate` prend 80 s contre 24 s pour `validate` : l'écriture domine. Le
« Lifecycle Engine » (snapshot, détection de changements, diff, génération
incrémentale, partitionnement stable, planner, explain) doit réduire ce
coût sans jamais produire un résultat différent d'une régénération
complète.

## Décision

1. Les dix invariants I1 à I10 de
   [lifecycle/README.md](../lifecycle/README.md) sont la définition de la
   correction. Le premier, l'**équivalence avec une régénération
   complète**, est vérifié par un oracle différentiel dans les tests de
   chaque phase.
2. Quatre phases indépendantes, dans cet ordre : A (ne republier que les
   fichiers changés), B (snapshot par URL et diff), C (stratégie
   `consistent-hash` et chiffrage des migrations), D (flux de changements).
   Chaque phase est fusionnée seule, avec ses mesures.
3. Le planner est une fonction pure ; `explain` et `generate` partagent le
   même plan.
4. Rien n'est supprimé automatiquement ; le snapshot est publié en dernier.
5. Aucun comportement par défaut ne change : les modes incrémentaux sont
   activés explicitement, et `delta` impose un ordre trié en option
   distincte.

## Conséquences

- La phase A n'ajoute aucun format d'entrée et ne change aucun contrat.
- Un harnais d'injection de pannes dans le store est nécessaire (I3, I4).
- Les décisions de format (ADR-0011) et de partitionnement (ADR-0012) sont
  prises séparément pour pouvoir être révisées.

## Mise en œuvre de la phase A (2026-11-22)

La phase A est implémentée **sans snapshot** : au lieu de comparer une
empreinte stockée, le store compare les octets produits au fichier publié
lui-même. Avantages : aucun nouveau format, aucune décision de hachage
nécessaire (l'ADR-0011 reste en suspens pour la phase B), et un fichier
modifié hors du moteur est détecté et republié. Mesures dans
`docs/performance/README.md`.
