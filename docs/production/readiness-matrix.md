# Matrice de maturité

Statuts : **READY** (implémenté, testé, documenté, limites connues, aucun
défaut ouvert connu) ; **NEEDS HARDENING** (fonctionne et testé, mais un
point de production reste ouvert) ; **EXPERIMENTAL** (fonctionne, contrat
susceptible de changer) ; **BLOCKED** (dépend d'une décision).

READY ne veut pas dire « certifié » : aucun composant n'a encore été
fuzzé (R10), et l'API Rust n'est pas stabilisée (R17).

| Composant | Statut | Preuves | Ce qui manque |
|---|---|---|---|
| Validation (`sitemap-validation`, `sitemap-model`) | READY | 38 tests ; profils standard et strict ; codes stables documentés ; aller-retour sur 3 000 entrées aléatoires | Fuzzing des URL (D) |
| Écriture XML (`sitemap-xml`) | READY | Sortie déterministe octet par octet ; fixtures validées par xmllint et les XSD officiels (sitemap, index, vidéo, news) et par un vérificateur Python indépendant | — |
| Partitionnement et index (`sitemap-index`) | READY | Limites exactes (URL, octets, 1 000 news) ; trois stratégies figées par vecteurs ; 18 tests | Index d'index (R21) |
| Publication fichiers (`sitemap-storage`) | NEEDS HARDENING | Staging, `fsync` + `rename`, verrou (lot A), nettoyage des orphelins, injection de pannes | Atomicité d'ensemble (R2), coupure électrique (R4), Windows fichier ouvert (R8) |
| Moteur `generate` / `validate` | READY | Streaming à mémoire bornée mesuré jusqu'à 5 M d'URL ; tests de bout en bout ; codes de sortie stables | Mesures 10 M+ (R15) |
| Extensions images, vidéo, news | READY | Validées par les XSD Google en CI ; balises dépréciées refusées et documentées | — |
| hreflang (validation, réciprocité) | READY | Codes dédiés ; coût mesuré (110 octets par lien) | — |
| robots.txt (`sitemap-robots`) | READY | RFC 9309, sortie déterministe, 5 tests | Peu de tests (5) |
| Lecteur sécurisé (`sitemap-reader`, `inspect`) | NEEDS HARDENING | DOCTYPE refusé, bornes de taille décompressée, profondeur, texte ; 23 tests adversariaux | Fuzzing (R10) ; pas de lecture distante (choix) |
| Protocole JSON (`sitemap-protocol`) | NEEDS HARDENING | Versions 1.0/1.1, schémas JSON, champs inconnus refusés | Fuzzing ; politique de dépréciation à écrire (E) |
| Snapshot et `diff` (`sitemap-snapshot`) | NEEDS HARDENING | Format v1 avec somme BLAKE3 ; tri externe borné ; vecteurs figés ; détection des corruptions | ADR-0011 à approuver (R18) ; fuzzing du décodeur ; lien snapshot ↔ publication (R5) |
| `consistent-hash` et coût de migration | EXPERIMENTAL | Implémenté, mesuré (4,0 % d'URL déplacées de 24 à 25 partitions) ; prédiction égale au résultat | ADR-0012 à approuver |
| Mode delta (`apply`) | EXPERIMENTAL | Oracle I1 aléatoire et vérification à 1 M d'URL ; refus avant écriture ; verrou (lot A) | ADR-0013 à approuver ; flux en mémoire (R9) ; R5 |
| Planner / `explain` | NEEDS HARDENING | Estimations marquées mesurées/estimées ; coût de migration exact | Choix automatique du mode (R13), plan lié à l'état (R14) |
| CLI (`sitemap-cli`) | READY | 14 tests de bout en bout ; JSON et humain ; codes 0 à 4 documentés | `doctor` à évaluer (E) |
| API Rust publique | EXPERIMENTAL | Utilisable et documentée (`cargo doc` sans avertissement) | Stabilisation, `#[non_exhaustive]`, erreurs typées complètes (R17) |
| Bindings (Node, Python, PHP) | BLOCKED | Non commencés, hors périmètre de la version stable du cœur | Stabilisation du protocole et de l'API |
| Distribution (crates.io, binaires) | BLOCKED | `publish = false` partout | Licence (R19), décision de publication |
