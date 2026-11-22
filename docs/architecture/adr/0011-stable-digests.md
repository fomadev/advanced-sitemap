# ADR-0011 — Empreintes persistées stables

- Statut : accepté (2026-11-22)
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

## Vérifications faites (2026-11-22)

- **Licences** du graphe de `blake3` 1.8.7 avec `default-features = false`
  et les fonctionnalités `std` et `pure` : `blake3` (CC0-1.0 OR Apache-2.0
  OR Apache-2.0 WITH LLVM-exception), `arrayvec`, `cfg-if`, `cpufeatures`,
  `constant_time_eq` (CC0-1.0 OR MIT-0 OR Apache-2.0), et pour la
  compilation `cc`, `find-msvc-tools`, `shlex`. Toutes acceptent MIT ou
  Apache-2.0, déjà autorisées dans `deny.toml`. La dépendance BSD-2-Clause
  envisagée plus haut n'existe pas dans cette version.
- **Débit** sur la machine de référence (i5-6300U, code portable `pure`,
  exemple `digest_throughput`) : 1 703 à 1 984 Mio/s sur un bloc de
  256 Mio ; 3,35 à 3,49 millions de blocs de 170 octets par seconde. Le
  pipeline complet traite environ 220 000 URL/s : l'empreinte d'entrée
  représente environ 6 % de son temps.
- **Vecteurs figés** : le préfixe du vecteur officiel BLAKE3 pour l'entrée
  vide, et une URL de référence, testés sur les trois OS par la CI.

Les deux conditions sont remplies ; la décision finale reste au mainteneur.

## Mise à jour du 2026-11-22 : format v2 et empreintes de fichiers

- Le snapshot passe au format v2 (`ASNAPv2`) : l'en-tête ajoute 16 octets,
  l'empreinte du manifest de la publication décrite. Le format v1 reste
  lu (fixture figée). Un moteur plus ancien traite un v2 comme invalide et
  le remplace, au prix d'une génération complète.
- Chaque fichier publié est haché pendant son écriture (même fonction,
  BLAKE3 tronqué à 128 bits) ; le manifest liste ces empreintes. Coût non
  mesurable sur une relance de 1 M d'URL (médiane identique avant et
  après).

## Acceptation (2026-11-22)

Accepté par le mainteneur le 2026-11-22, après une revue qui a vérifié la
décision contre le code et les tests (R18 du registre des risques). Le format du snapshot v2 et les
empreintes du manifeste deviennent un contrat : les changer est un
changement incompatible ([semver-policy.md](../../production/semver-policy.md),
section 6). Les vecteurs figés passent désormais sur cinq plateformes
(Linux et Windows en x86_64 et ARM64, macOS ARM64), et le job CI
`determinism` vérifie que les fichiers publiés et le snapshot ont les mêmes
octets sur les cinq.
