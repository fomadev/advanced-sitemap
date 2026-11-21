# ADR-0001 — Moteur Rust unique, protocole JSON comme contrat polyglotte

- Statut : accepté
- Date : 2026-11-21

## Contexte

La logique de génération existe en TypeScript, couplée à l'objet `Response`
de Next.js. FomaDev veut des intégrations Nuxt, Laravel, Django, Spring,
ASP.NET, etc., sans réimplémenter les algorithmes.

## Décision

1. Le moteur de référence est écrit en Rust stable.
2. Le **contrat stable entre langages est le protocole JSON versionné**
   (documents de configuration, enregistrements d'URL, rapports), pas une ABI.
3. Les modes de distribution (crate Rust, CLI, bindings N-API/PyO3, service
   HTTP) sont des couches minces au-dessus du même cœur ; ils sérialisent
   aux frontières et ne contiennent aucune logique métier.
4. Aucune ABI Rust n'est exposée comme contrat public.

## Conséquences

- La CLI est utilisable immédiatement depuis n'importe quel langage
  (PHP/Laravel, Java, Go, .NET) sans compilation native.
- Les bindings natifs sont une optimisation (pas de processus, flux en
  mémoire), pas une nécessité.
- Le coût de sérialisation JSON aux frontières est accepté ; il sera mesuré.
