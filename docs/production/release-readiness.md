# État de préparation à la release

État au 2026-11-22, après le lot F. Ce document applique les conditions de
sortie du [plan de release](release-plan.md) et dit ce qui reste à
décider. Aucune version n'est publiée.

## 1. Verdict

**Techniquement prêt pour une version candidate 0.1.0, non publiable en
l'état.** Le code, les tests, la CI et les artefacts sont prêts ; trois
décisions du propriétaire bloquent toute distribution (section 3).

## 2. Conditions de sortie

| Condition | État | Preuve |
|---|---|---|
| Aucun P0 ni P1 technique ouvert | Oui | [Registre des risques](risk-register.md) : R1, R5, R10, R17, R25 corrigés ; R18 et R19 sont des décisions, pas des défauts |
| Tests et CI verts sur les plateformes annoncées | Oui | 5 plateformes (Linux et Windows x86_64 et ARM64, macOS ARM64), MSRV 1.89, lint, conformance, supply-chain, fuzzing stable, déterminisme |
| Même sortie sur toutes les plateformes | Oui | Job `determinism` : empreintes identiques après `generate` et `apply` |
| Conformité XML par des outils indépendants | Oui | XSD sitemaps.org et Google (xmllint), vérificateur Python |
| Reprise après interruption | Oui | Injection de pannes (lot A), arrêts brutaux du processus (lot F), staging nettoyé, snapshot périmé détecté |
| Fuzzing exécuté, corpus conservé | Oui | Lot D : fuzzing stable à chaque PR, `cargo fuzz` hebdomadaire, corpus dans `fuzz/corpus` |
| Performance mesurée sur les volumes annoncés | Oui jusqu'à 10 M d'URL | [performance-report.md](performance-report.md) ; 50 M non mesuré, non annoncé |
| Documentation cohérente avec le code | Oui | Rapports de ce dossier, `docs/cli`, `docs/protocol`, `cargo doc` sans avertissement |
| Formats versionnés, politique de compatibilité écrite | Oui | [semver-policy.md](semver-policy.md) |
| Aucun secret dans les artefacts | Oui | [security-report.md](security-report.md), section 7 |
| Aucun défaut connu de corruption | Oui | Aucun fichier publié tronqué dans aucun test ; l'ensemble mixte (R2) est documenté, détecté par `doctor` et réparé à l'exécution suivante |
| Licence finalisée | **Non** | R19 : clarifications FPL et titulaire des droits |
| Formats approuvés | **Non** | R18 : ADR-0010 à 0013 |

## 3. Décisions qui bloquent une distribution

| Décision | Pourquoi elle bloque |
|---|---|
| Licence : clarifications FPL 1.1 et titulaire des droits (R19) | Aucune distribution sans licence définitive |
| Approbation des ADR-0010 à 0013 (R18) | Le format du snapshot v2 et le mode delta en dépendent ; les changer après publication serait incompatible |
| Autorisation de publier, et où (GitHub Releases, crates.io) | Rien n'est publié sans votre accord explicite ; pour crates.io il faut aussi des métadonnées et des noms de crates disponibles |

Décisions utiles mais non bloquantes pour une 0.1 : publication atomique
d'ensemble (R2), politique de montée de MSRV, signature des artefacts.

## 4. Version candidate

Le workflow `Release candidate` (`.github/workflows/release-candidate.yml`)
se lance à la main (`workflow_dispatch`) et sur une PR qui le modifie. Il :

1. construit la CLI pour 7 cibles : Linux x86_64 (glibc, musl), Linux
   ARM64, Windows x86_64 et ARM64, macOS ARM64 et x86_64 ;
2. vérifie chaque binaire exécutable sur son runner (6 sur 7 ; macOS
   x86_64 est compilé de façon croisée) : `--version`, puis `generate` sur
   les 6 fixtures de conformité, comparées octet par octet ;
3. produit des archives `advanced-sitemap-<version>-<commit>-<cible>`
   (`.tar.gz`, `.zip` pour Windows) et `SHA256SUMS` ;
4. les garde 14 jours comme artefacts du workflow. Il ne crée ni tag, ni
   release, ni paquet.

Le numéro reste `0.1.0` : changer de version est une décision de
publication. Le commit figure dans le nom de l'archive et dans
`BUILDINFO.txt`.

## 5. Publier, une fois autorisé (non fait)

1. Trancher R18 et R19 ; mettre à jour `LICENSE` et les métadonnées.
2. Fixer la version (`0.1.0` ou `0.1.0-rc.1`), compléter le CHANGELOG.
3. Lancer `Release candidate` sur le commit, vérifier `SHA256SUMS`.
4. Créer le tag et la release GitHub avec ces archives.
5. Ajouter `cargo semver-checks` à la CI avec cette version comme
   référence.
6. crates.io, si décidé : retirer `publish = false` crate par crate, dans
   l'ordre des dépendances.

## 6. Retour arrière

Les formats persistants sont versionnés. Revenir à une version antérieure
coûte au pire une génération complète : un snapshot d'un format inconnu
est ignoré, puis remplacé.
