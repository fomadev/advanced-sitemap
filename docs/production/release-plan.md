# Plan de release

Objectif : une version candidate du cœur Rust, puis une version stable
quand toutes les conditions de la section 3 sont remplies. Aucune
publication (crates.io, binaires, tag de release) sans autorisation
explicite du propriétaire du projet.

## 1. Lots

| Lot | Contenu | Sortie attendue | État |
|---|---|---|---|
| A | Audit, P0/P1, durabilité, concurrence | Ce dossier ; verrou ; MSRV vérifiée | Fait (PR #16) |
| B | Planner automatique, modèle de coût, `explain` lié à l'état, repli sûr ; lien snapshot ↔ publication (R5) | `--execution-mode auto`, plan avec jeton d'état | Fait (PR #17) |
| C | Profilage, allocations, 10 M d'URL, bornes de ressources (flux `apply`) | Mesures avant/après | Fait (PR #18) |
| D | Fuzzing (lecteur XML, protocole, snapshot, flux, URL), `cargo audit`, Actions épinglées | Cibles `cargo fuzz` et corpus ; `security-report.md` | Fait (PR #19) |
| E | API Rust stable (`#[non_exhaustive]`, constructeurs, erreurs typées), CLI (`doctor`), règles de configuration | `semver-policy.md` | Fait (PR #23) |
| F | Tests interplateformes (ARM), conformité, documentation, version candidate | `release-readiness.md`, `final-audit.md`, workflow `Release candidate` | Fait (cette PR) |

## 2. Décisions attendues du propriétaire

| Décision | Bloque |
|---|---|
| ~~Approbation des ADR-0010 à 0013~~ acceptés le 2026-11-22 | — |
| ~~Clarifications FPL 1.1 et titulaire des droits~~ décidé le 2026-11-22 : FPL v1.0 conservée, Fordima Inc. | — |
| Publication atomique d'ensemble (révisions + pointeur) ou maintien de l'ordre actuel | R2 |
| MSRV 1.89 et politique MSRV | 1.0 |
| Nom de l'option de mode d'exécution (`--execution-mode`) | Lot B |
| Publication sur crates.io (noms de crates, métadonnées) | Distribution |

## 3. Conditions de sortie (release gate)

Une version n'est déclarée prête que si toutes sont vraies. Leur état au
lot F est dans [release-readiness.md](release-readiness.md) : toutes sont
remplies ; seule manque l'autorisation de publier.

- [x] Aucun P0 ni P1 ouvert
- [x] Tests et CI verts sur les plateformes annoncées (5, ARM64 compris)
- [x] Conformité XML vérifiée par des outils indépendants
- [x] Tests de reprise après interruption (pannes injectées, arrêts brutaux)
- [x] Fuzzing exécuté, corpus de régression conservé (lot D)
- [x] Performance mesurée sur les volumes annoncés (jusqu'à 10 M)
- [x] Documentation cohérente avec le code
- [x] Licence finalisée (R19 : FPL v1.0, Fordima Inc.)
- [x] Notices des dépendances jointes aux binaires (R27)
- [x] Formats persistants versionnés et politique de compatibilité écrite
- [x] Aucun secret dans les artefacts
- [x] Aucun défaut connu de corruption ou de publication incohérente
  (l'ensemble mixte R2 est documenté, détecté et réparé)
- [x] ADR-0010 à 0013 acceptés (R18)

## 4. Version candidate (prévue)

- Numérotation : `0.x` jusqu'à la stabilisation de l'API ; `1.0.0-rc.1`
  seulement quand la section 3 est remplie.
- Artefacts : binaires CLI pour 7 cibles (workflow `Release candidate`,
  voir [release-readiness.md](release-readiness.md)) ; sommes SHA-256 ;
  construction par la CI (`--locked`).
- Retour arrière : les formats persistants étant versionnés, revenir à
  une version antérieure coûte au pire une génération complète (snapshot
  ignoré si inconnu).
