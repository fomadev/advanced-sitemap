# Audit final

- Date : 2026-11-22
- Portée : moteur Rust, CLI, protocole JSON et formats persistés, après
  les lots A à F (PR #16 à #19, #23 et celle du lot F). Les bindings
  (Node, Python, PHP) sont hors périmètre.
- Point de départ : [audit du lot A](audit.md).

## 1. Ce qui a été vérifié

| Domaine | Vérification | Résultat |
|---|---|---|
| Fonctionnel | 218 tests (Windows), dont conformité octet par octet, oracle aléatoire du mode delta, vecteurs figés des stratégies | Verts sur 5 plateformes |
| Formats XML | XSD officiels (sitemaps.org, Google vidéo et news), vérificateur Python indépendant | Conformes |
| Déterminisme | Mêmes octets après `generate` et `apply` sur Linux, Windows (x86_64 et ARM64) et macOS ARM64 | Identiques |
| Concurrence | Verrou de dossier ; deuxième exécution refusée (`busy`) | Corrigé au lot A (R1) |
| Durabilité | `fsync` avant `rename`, `fsync` du dossier (Unix), injection de pannes, 12 arrêts brutaux du processus | Aucun fichier tronqué ; ensemble mixte détecté et réparé |
| Cohérence état ↔ publication | Snapshot v2 relié au manifeste, empreintes par fichier, `SNAPSHOT_STALE`, jeton d'état | Corrigé au lot B (R5, R14) |
| Ressources | Mémoire bornée mesurée jusqu'à 10 M d'URL ; flux d'`apply` borné | Lot C |
| Sécurité | Fuzzing stable et guidé par la couverture, lecteur borné, noms d'artefacts contraints, chaîne d'approvisionnement | Un défaut P1 trouvé et corrigé (R25) |
| API | Types `#[non_exhaustive]` imposés par clippy, erreurs typées, politique SemVer, codes de diagnostic figés | Lot E (R17) |
| Exploitation | `doctor` en lecture seule, codes de sortie et `error.kind` stables, liste de contrôle | Lots E et F |
| Distribution | 7 binaires construits, 6 vérifiés contre les fixtures, notices des dépendances, `SHA256SUMS` | Lot F, R27 ; rien n'est publié |

## 2. Défauts trouvés et corrigés pendant la mission

| ID | Défaut | Gravité | Lot |
|---|---|---|---|
| R1 | Deux exécutions sur le même dossier pouvaient entrelacer leurs publications et perdre un flux `apply` | P1 | A |
| R7 | MSRV déclarée (1.85) fausse | P2 | A |
| R5 | Un snapshot pouvait décrire une autre publication que celle en place | P2 | B |
| R22 | `explain` sans `--out` planifiait sur un autre dossier | P2 | B |
| R25 | Un caractère de contrôle dans un texte news ou vidéo arrêtait la génération sur une erreur interne | P1 | D |
| R17 | Tout ajout à l'API publique était un changement incompatible | P1 pour 1.0 | E |
| R8 | Sous Windows, un fichier tenu ouvert par un autre programme faisait échouer la publication immédiatement | P2 | F |

## 3. Risques encore ouverts

| ID | Risque | Priorité | Traitement |
|---|---|---|---|
| R2 | Publication d'un ensemble non atomique | P2 | Documenté, détecté par `doctor`, réparé à l'exécution suivante ; révisions + pointeur à décider |
| R4 | Coupure électrique non testée par un banc | P2 | Arrêts brutaux du processus testés ; dépend d'un disque qui respecte `fsync` |
| R15 | 50 M d'URL non mesurés | P2 | Non annoncé |
| R26 | Licence des contributions externes non tranchée | P2 | Avant la première contribution externe |
| R20, R21, R23, R24 | Verrou illisible sous Windows, index d'index absent, calibration sur une machine, monothread | P3 | Documentés |

Le détail de chaque limite est dans [known-limitations.md](known-limitations.md).

## 4. Ce qui n'a pas été vérifié

- Une vraie coupure électrique, des disques réseau, des systèmes de fichiers
  autres que NTFS et ceux des runners CI (ext4, APFS).
- Les performances ailleurs que sur la machine de référence.
- macOS x86_64 à l'exécution (compilé seulement).
- Une revue de sécurité externe.

## 5. Conclusion

Le moteur et la CLI remplissent les conditions techniques de sortie
([release-readiness.md](release-readiness.md)) : aucun défaut connu de
corruption, comportement identique sur cinq plateformes, erreurs et
limites documentées. La licence est décidée (FPL v1.0, Fordima Inc.). Les
notices des dépendances sont jointes aux archives et les ADR-0010 à 0013
sont acceptés. Une distribution n'attend plus que votre autorisation de
publier.
