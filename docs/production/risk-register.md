# Registre des risques

Priorités : **P0** perte ou corruption de données, faille critique,
publication incorrecte ; **P1** bloque une utilisation en production ;
**P2** limitation importante mais contournable ; **P3** amélioration.

État au 2026-11-22, après le lot F.

| ID | Risque | Priorité | État | Lot | Notes |
|---|---|---|---|---|---|
| R1 | Exécutions concurrentes sur le même dossier : publications entrelacées, perte d'un flux `apply` | P1 | **Corrigé** | A | Verrou système, libéré à la mort du processus |
| R2 | Publication d'un ensemble de fichiers non atomique : fenêtre où une URL est dans deux fichiers ou dans aucun | P2 | Ouvert, documenté | Décision | Converge à l'exécution suivante ; révisions + pointeur à décider |
| R3 | Renommages non durables sous Linux en cas de coupure | P2 | **Corrigé** (Unix) | A | Windows : dépend du système de fichiers |
| R4 | Résistance à une coupure électrique non démontrée | P2 | Ouvert, réduit | F | Pas de banc de coupure ; garanties fondées sur `fsync`/`rename`. Arrêts brutaux du processus testés (12 points, aucun fichier tronqué, ensemble mixte détecté par `doctor` et réparé) |
| R5 | Snapshot non relié à la publication qu'il décrit (publication interrompue avant son remplacement) | P2 | **Corrigé** | B | Le manifest liste l'empreinte de chaque fichier et le snapshot v2 celle du manifest : `SNAPSHOT_STALE` et refus d'`apply` |
| R6 | Dossiers de staging orphelins | P2 | **Corrigé** | A | |
| R7 | MSRV déclarée fausse et non vérifiée | P2 | **Corrigé** | A | 1.89, job CI |
| R8 | Sous Windows, remplacer un sitemap ouvert par un autre programme (serveur web) peut échouer : ensemble mélangé | P2 | **Corrigé** | F | Reproduit (accès refusé) ; nouvelle tentative pendant environ 2,3 s, puis erreur qui nomme le fichier ; `doctor` montre l'état, l'exécution suivante répare |
| R9 | Le flux de `apply` est tenu en mémoire (une entrée par URL changée) | P2 | **Corrigé** | C | 806 octets par URL mesurés ; borne `state.maxFeedOperations` (500 000 par défaut), refus avant écriture |
| R10 | Aucun fuzzing (lecteur XML, protocole, décodeur de snapshot, flux) | P1 pour une release stable | **Corrigé** | D | Fuzzing stable reproductible à chaque PR, `cargo fuzz` hebdomadaire ; un défaut trouvé et corrigé (R25) |
| R25 | Caractère interdit par XML dans un texte news ou vidéo : validé puis refusé par l'écrivain, arrêt sur erreur interne | P1 | **Corrigé** | D | Trouvé par fuzzing ; règle partagée validation/écrivain |
| R11 | `cargo audit` non exécuté localement | P3 | **Clos** | D | `cargo deny check advisories` utilise la même base RustSec à chaque PR |
| R12 | Actions GitHub épinglées par étiquette (`@v4`), pas par empreinte | P3 | **Corrigé** | D | Empreintes de commit, Dependabot |
| R13 | Le planner ne choisit pas automatiquement le mode d'exécution | P2 | **Corrigé** | B | `--execution-mode auto` ; `explain --changes` recommande `apply` ou une régénération ; pas de repli automatique qui ignorerait le flux |
| R14 | Plan d'`explain` non lié à une version de l'état | P2 | **Corrigé** | B | Jeton d'état et `--expect-state`, vérifiés sous verrou |
| R22 | `explain` sans `--out` regardait un autre dossier que `generate` et `apply` | P2 | **Corrigé** | B | |
| R23 | Le modèle de coût est calibré sur une seule machine | P3 | Ouvert, documenté | F | Recalibré au lot C sur une série homogène (1 M et 10 M) ; reste une surestimation de 26 % des générations complètes à 10 M ; seule la comparaison entre modes vaut sur un autre matériel |
| R15 | Tests à 10 M et 50 M d'URL non faits | P2 | **Corrigé pour 10 M** ; 50 M ouvert | C | 10 M mesuré (mémoire bornée vérifiée) ; 50 M impossible sur la machine de référence (10 Go libres), coûts extrapolés |
| R24 | Une seule unité de calcul utilisée : la validation (dont l'analyse des URL) domine le temps CPU | P3 | Ouvert | — | Gain possible par validation parallèle avec fusion ordonnée ; à mesurer avant de décider |
| R16 | Linux ARM64 et Windows ARM64 non testés | P2 | **Corrigé** | F | Tests et déterminisme en CI sur `ubuntu-24.04-arm` et `windows-11-arm` ; macOS x86_64 seulement compilé |
| R17 | API Rust publique non stabilisée (structs publiques à champs publics, enums non `#[non_exhaustive]`) | P1 pour 1.0 | **Corrigé** | E | Types publics `#[non_exhaustive]` imposés par clippy, constructeurs, `EngineError::kind`, politique `semver-policy.md` ; comparaison automatique d'API (`cargo semver-checks`) seulement après une première version publiée |
| R18 | ADR-0010 à 0013 non approuvés ; format du snapshot dépendant de l'ADR-0011 | P1 pour 1.0 | **Décidé** | Décision | 2026-11-22 : les quatre ADR acceptés après une revue contre le code ; le snapshot v2, BLAKE3-128, `consistent-hash` et le mode delta deviennent des contrats |
| R19 | Licence FPL : clarifications et titulaire des droits non confirmés | P1 pour une distribution | **Décidé** | Décision | 2026-11-22 : FPL v1.0 conservée sans modification, titulaire Fordima Inc. (constituée, droits cédés), droit applicable non précisé (`docs/legal/fpl-review.md`, section 8) |
| R26 | Les contributions externes n'accordent pas explicitement à Fordima un droit de sous-licence commerciale (FPL section 2) | P2 avant la première contribution externe | Ouvert | Décision | À trancher avant d'accepter une Pull Request externe : clause de licence ou CLA |
| R27 | Les archives de la version candidate ne contiennent pas les notices de licence des dépendances (MIT, Apache-2.0...), que ces licences exigent dans les binaires distribués | P1 pour une distribution de binaires | **Corrigé** | Release | `tools/notices.py` écrit `THIRD-PARTY-NOTICES.txt` (51 à 56 crates selon la cible) dans chaque archive ; le job `lint` échoue si une dépendance n'a pas de fichier de licence |
| R20 | Fichier de verrou illisible sous Windows pendant une exécution | P3 | Ouvert, documenté | A | À exclure des copies et déploiements |
| R21 | Sitemap index limité à 50 000 enfants, sans index d'index | P3 | Ouvert, documenté | — | 2,5 milliards d'URL au maximum |
