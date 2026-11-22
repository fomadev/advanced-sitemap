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
| E | API Rust stable (`#[non_exhaustive]`, constructeurs, erreurs typées), CLI (`doctor`), règles de configuration | `semver-policy.md` | Fait (cette PR) |
| F | Tests interplateformes (ARM), conformité, documentation, version candidate | Notes de version, artefacts, sommes de contrôle | À faire |

## 2. Décisions attendues du propriétaire

| Décision | Bloque |
|---|---|
| Approbation des ADR-0010 à 0013 (format du snapshot avec BLAKE3) | Stabilité des formats, 1.0 |
| Clarifications FPL 1.1 et titulaire des droits | Toute distribution |
| Publication atomique d'ensemble (révisions + pointeur) ou maintien de l'ordre actuel | R2 |
| MSRV 1.89 et politique MSRV | 1.0 |
| Nom de l'option de mode d'exécution (`--execution-mode`) | Lot B |
| Publication sur crates.io (noms de crates, métadonnées) | Distribution |

## 3. Conditions de sortie (release gate)

Une version n'est déclarée prête que si toutes sont vraies :

- [ ] Aucun P0 ni P1 ouvert (aujourd'hui : R10, R17, R18, R19 ouverts)
- [ ] Tests et CI verts sur les plateformes annoncées
- [ ] Conformité XML vérifiée par des outils indépendants (fait en CI)
- [ ] Tests de reprise après interruption (en partie fait, lot A)
- [x] Fuzzing exécuté, corpus de régression conservé (lot D)
- [ ] Performance mesurée sur les volumes annoncés
- [ ] Documentation cohérente avec le code
- [ ] Licence finalisée
- [ ] Formats persistants versionnés et politique de migration écrite
- [ ] Aucun secret dans les artefacts
- [ ] Aucun défaut connu de corruption ou de publication incohérente

## 4. Version candidate (prévue)

- Numérotation : `0.x` jusqu'à la stabilisation de l'API ; `1.0.0-rc.1`
  seulement quand la section 3 est remplie.
- Artefacts : binaires CLI Windows x64, Linux x64, macOS ARM64 ;
  sommes SHA-256 ; construction reproductible par la CI (`--locked`).
- Retour arrière : les formats persistants étant versionnés, revenir à
  une version antérieure coûte au pire une génération complète (snapshot
  ignoré si inconnu).
