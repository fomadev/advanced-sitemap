# ADR-0011 — Empreintes persistées stables

- Statut : proposé
- Date : 2026-11-21

## Contexte

La déduplication utilise `std::hash::DefaultHasher`. Sa documentation ne
garantit pas que l'algorithme reste le même d'une version de Rust à
l'autre : une empreinte écrite sur disque pourrait ne plus correspondre
après une mise à jour du compilateur, et toutes les partitions seraient
alors vues comme modifiées, ou pire, deux contenus différents comme égaux
si l'algorithme changeait de façon défavorable. Le snapshot a besoin
d'empreintes stables, sur toutes les plateformes, pour toujours.

## Options

| Option | Collisions | Résistance à un adversaire | Dépendance |
|---|---|---|---|
| FNV-1a 128 écrit dans le dépôt | ~n²/2¹²⁹ pour des données ordinaires | Aucune : des collisions se fabriquent facilement | Aucune |
| XXH3-128 | ~n²/2¹²⁹ | Aucune | `xxhash-rust` (licence BSL-1.0, à ajouter à `deny.toml`) |
| BLAKE3 tronqué à 128 bits | ~n²/2¹²⁹ | Fonction cryptographique | `blake3` |

Un adversaire existe dans un usage multi-tenant (produit SaaS) : un client
qui fabrique deux contenus de même empreinte pourrait empêcher la
republication d'un fichier. Le coût de BLAKE3 est faible devant le reste :
l'encodage XML tourne à environ 270 Mo/s mesurés (1,57 M URL/s × environ
170 octets), et BLAKE3 hache typiquement bien au-delà d'1 Go/s sur un cœur
(à mesurer sur la machine de référence avant adoption).

## Décision proposée

- `digest128` = les 128 premiers bits de BLAKE3, utilisé pour toutes les
  empreintes persistées (URL, entrées, partitions, configuration).
- Le routage des partitions reste FNV-1a 64 (ADR-0005) : il est figé, et
  changer de hachage déplacerait toutes les URL.
- `DefaultHasher` reste réservé aux structures en mémoire d'une seule
  exécution.
- Vecteurs de test figés pour `digest128`, exécutés sur les trois OS.

## À vérifier avant adoption

- Licences du graphe de `blake3` avec `cargo deny`, avec la fonctionnalité
  `pure` (pas de compilation C ou assembleur) pour simplifier les bindings.
  Si une dépendance transitive n'est pas dans la liste autorisée (par
  exemple une licence BSD-2-Clause), la décision revient au mainteneur.
- Débit réel sur la machine de référence.
- En cas de refus, repli sur FNV-1a 128 dans le dépôt, avec la limite de
  résistance documentée.
