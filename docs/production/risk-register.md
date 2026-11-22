# Registre des risques

Priorités : **P0** perte ou corruption de données, faille critique,
publication incorrecte ; **P1** bloque une utilisation en production ;
**P2** limitation importante mais contournable ; **P3** amélioration.

État au 2026-11-22, après le lot A.

| ID | Risque | Priorité | État | Lot | Notes |
|---|---|---|---|---|---|
| R1 | Exécutions concurrentes sur le même dossier : publications entrelacées, perte d'un flux `apply` | P1 | **Corrigé** | A | Verrou système, libéré à la mort du processus |
| R2 | Publication d'un ensemble de fichiers non atomique : fenêtre où une URL est dans deux fichiers ou dans aucun | P2 | Ouvert, documenté | Décision | Converge à l'exécution suivante ; révisions + pointeur à décider |
| R3 | Renommages non durables sous Linux en cas de coupure | P2 | **Corrigé** (Unix) | A | Windows : dépend du système de fichiers |
| R4 | Résistance à une coupure électrique non démontrée | P2 | Ouvert | F | Pas de banc de coupure ; garanties fondées sur `fsync`/`rename` |
| R5 | Snapshot non relié à la publication qu'il décrit (publication interrompue avant son remplacement) | P2 | **Corrigé** | B | Sortie jamais fausse ; rapport de changements possiblement inexact, `apply` refuse les partitions touchées incohérentes. Piste : empreinte du manifest enregistrée avec le snapshot |
| R6 | Dossiers de staging orphelins | P2 | **Corrigé** | A | |
| R7 | MSRV déclarée fausse et non vérifiée | P2 | **Corrigé** | A | 1.89, job CI |
| R8 | Sous Windows, remplacer un sitemap ouvert par un autre programme (serveur web) peut échouer : ensemble mélangé | P2 | Ouvert | F | À tester ; documenter ou réessayer |
| R9 | Le flux de `apply` est tenu en mémoire (une entrée par URL changée) | P2 | Ouvert | C | Documenté ; borne configurable à prévoir |
| R10 | Aucun fuzzing (lecteur XML, protocole, décodeur de snapshot, flux) | P1 pour une release stable | Ouvert | D | Tests adversariaux ciblés existants (XXE, DOCTYPE, bombes gzip, profondeur) |
| R11 | `cargo audit` non exécuté localement | P3 | Ouvert | D | `cargo deny` couvre les avis en CI |
| R12 | Actions GitHub épinglées par étiquette (`@v4`), pas par empreinte | P3 | Ouvert | D | `permissions: contents: read` déjà en place |
| R13 | Le planner ne choisit pas automatiquement le mode d'exécution | P2 | **Corrigé** | B | `--execution-mode auto` ; `explain --changes` recommande `apply` ou une régénération ; pas de repli automatique qui ignorerait le flux |
| R14 | Plan d'`explain` non lié à une version de l'état | P2 | **Corrigé** | B | Jeton d'état et `--expect-state`, vérifiés sous verrou |
| R22 | `explain` sans `--out` regardait un autre dossier que `generate` et `apply` | P2 | **Corrigé** | B | |
| R23 | Le modèle de coût est calibré sur une seule machine ; les générations complètes sont surestimées de 12 à 30 % | P3 | Ouvert, documenté | C | Seul l'ordre des modes compte pour la décision |
| R15 | Tests à 10 M et 50 M d'URL non faits | P2 | Ouvert | C | Mesuré jusqu'à 5 M |
| R16 | Linux ARM64 et Windows ARM64 non testés | P2 | Ouvert | F | macOS ARM64 testé en CI |
| R17 | API Rust publique non stabilisée (structs publiques à champs publics, enums non `#[non_exhaustive]`) | P1 pour 1.0 | Ouvert | E | Chaque ajout de variante est aujourd'hui un changement incompatible |
| R18 | ADR-0010 à 0013 non approuvés ; format du snapshot dépendant de l'ADR-0011 | P1 pour 1.0 | Ouvert | Décision | |
| R19 | Licence FPL : clarifications et titulaire des droits non confirmés | P1 pour une distribution | Ouvert | Décision | Aucun package publié |
| R20 | Fichier de verrou illisible sous Windows pendant une exécution | P3 | Ouvert, documenté | A | À exclure des copies et déploiements |
| R21 | Sitemap index limité à 50 000 enfants, sans index d'index | P3 | Ouvert, documenté | — | 2,5 milliards d'URL au maximum |
