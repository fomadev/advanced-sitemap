# Matrice de maturité

Statuts : **READY** (implémenté, testé, documenté, limites connues, aucun
défaut ouvert connu) ; **NEEDS HARDENING** (fonctionne et testé, mais un
point de production reste ouvert) ; **EXPERIMENTAL** (fonctionne, contrat
susceptible de changer) ; **BLOCKED** (dépend d'une décision).

READY ne veut pas dire « certifié » : aucune version n'est publiée, et
la compatibilité d'API n'est vérifiée automatiquement qu'à partir de la
première (voir `semver-policy.md`).

| Composant | Statut | Preuves | Ce qui manque |
|---|---|---|---|
| Validation (`sitemap-validation`, `sitemap-model`) | READY | 38 tests ; profils standard et strict ; codes stables documentés ; aller-retour sur 3 000 entrées aléatoires ; fuzzing (un défaut corrigé : caractères de contrôle dans les textes d'extensions) | — |
| Écriture XML (`sitemap-xml`) | READY | Sortie déterministe octet par octet ; fixtures validées par xmllint et les XSD officiels (sitemap, index, vidéo, news) et par un vérificateur Python indépendant | — |
| Partitionnement et index (`sitemap-index`) | READY | Limites exactes (URL, octets, 1 000 news) ; trois stratégies figées par vecteurs ; 18 tests | Index d'index (R21) |
| Publication fichiers (`sitemap-storage`) | NEEDS HARDENING | Staging, `fsync` + `rename`, verrou (lot A), nettoyage des orphelins, injection de pannes, arrêts brutaux du processus, fichier tenu ouvert sous Windows (lot F) | Atomicité d'ensemble (R2), banc de coupure électrique (R4) |
| Moteur `generate` / `validate` | READY | Streaming à mémoire bornée mesuré jusqu'à 10 M d'URL (5 Mio sans déduplication, 416 Mio avec) ; tests de bout en bout ; codes de sortie stables ; limites documentées (`docs/concepts/limits.md`) | 50 M non mesuré (R15) |
| Extensions images, vidéo, news | READY | Validées par les XSD Google en CI ; balises dépréciées refusées et documentées | — |
| hreflang (validation, réciprocité) | READY | Codes dédiés ; coût mesuré (110 octets par lien) | — |
| robots.txt (`sitemap-robots`) | READY | RFC 9309, sortie déterministe, 5 tests | Peu de tests (5) |
| Lecteur sécurisé (`sitemap-reader`, `inspect`) | READY | DOCTYPE refusé, bornes de taille décompressée, profondeur, texte ; 23 tests adversariaux ; fuzzing XML et gzip (2,4 M de mutations sans défaut) | Pas de lecture distante (choix) |
| Protocole JSON (`sitemap-protocol`) | NEEDS HARDENING | Versions 1.0/1.1, schémas JSON, champs inconnus refusés ; fuzzing des configurations, enregistrements et flux ; politique de compatibilité et de dépréciation (`semver-policy.md`) ; règles de configuration documentées | ADR à approuver (R18) |
| Snapshot et `diff` (`sitemap-snapshot`) | NEEDS HARDENING | Format v2 relié à la publication (v1 toujours lu) ; somme BLAKE3 ; tri externe borné ; vecteurs figés ; détection des corruptions et des snapshots périmés ; décodeur fuzzé (800 000 mutations, aucun fichier modifié accepté) | ADR-0011 à approuver (R18) |
| `consistent-hash` et coût de migration | EXPERIMENTAL | Implémenté, mesuré (4,0 % d'URL déplacées de 24 à 25 partitions) ; prédiction égale au résultat | ADR-0012 à approuver |
| Mode delta (`apply`) | EXPERIMENTAL | Oracle I1 aléatoire et vérification à 1 M d'URL ; refus avant écriture ; verrou (lot A) ; snapshot relié à la publication (lot B) ; flux borné (lot C) | ADR-0013 à approuver |
| Planner / `explain` | NEEDS HARDENING | Mode `auto`, plan d'exécution avec préconditions et coûts, `explain --changes`, jeton d'état vérifié sous verrou ; estimations comparées aux mesures (décisions justes dans tous les cas mesurés) | Calibration sur une seule machine (R23) ; réserves en octets et en news |
| CLI (`sitemap-cli`) | READY | 15 tests de bout en bout ; JSON et humain ; codes 0 à 4 et `error.kind` documentés ; `doctor` (contrôle en lecture seule) | — |
| API Rust publique | NEEDS HARDENING | Types publics `#[non_exhaustive]` (clippy l'impose), constructeurs, `EngineError::kind`/`source`/`is_retryable`, `Engine::doctor` ; politique SemVer ; codes de diagnostic figés par test | Comparaison d'API automatique après la première version (F) ; ADR à approuver (R18) |
| Bindings (Node, Python, PHP) | BLOCKED | Non commencés, hors périmètre de la version stable du cœur | Stabilisation du protocole et de l'API |
| Distribution (crates.io, binaires) | BLOCKED | Version candidate construite par la CI pour 7 cibles, vérifiée contre les fixtures, notices des dépendances, `SHA256SUMS` ; `publish = false` partout | ADR (R18), décision de publication |
