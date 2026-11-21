# Distribution par écosystème (recommandation)

Aucun binding n'est implémenté à ce stade. Le contrat commun à toutes les
voies est le protocole JSON v1 et les fixtures `fixtures/conformance`.

| Écosystème | Voie recommandée en premier | Alternative | Raisons |
|---|---|---|---|
| Next.js, Nuxt, Express, NestJS | Binding N-API (napi-rs), binaires précompilés par plateforme en paquets npm optionnels | CLI | Pas de processus externe, flux en mémoire. **Le runtime Edge ne charge pas d'addon natif** : étudier une cible WASM ou garder la CLI au build |
| Vue / Vite (sites statiques) | CLI appelée par un plugin de build | Binding Node | La génération a lieu au build, le coût d'un processus est négligeable |
| Django, Flask | Binding PyO3 (maturin, wheels abi3) | CLI | Installation `pip` sans compilateur grâce aux wheels |
| Laravel, PHP | CLI (binaire fourni par un package Composer) | Extension FFI PHP ou service HTTP | FFI exige `ffi.enable`, souvent désactivé en hébergement mutualisé ; la CLI marche partout |
| Spring, Java | CLI | JNI, service HTTP | JNI coûte cher en maintenance ; à justifier par la demande |
| Go | CLI | cgo vers une ABI C (`sitemap-ffi`) | cgo complique la compilation croisée |
| ASP.NET, .NET | CLI | P/Invoke vers une ABI C | P/Invoke est simple une fois l'ABI C stable |
| Rust | Crate `advanced-sitemap` | — | Natif |

## Comparaison des modes

| Critère | Crate Rust | Binding natif | CLI | Service HTTP |
|---|---|---|---|---|
| Performance | Maximale | Élevée (sérialisation aux frontières) | Élevée, avec un coût de démarrage et d'E/S | Réseau + sérialisation |
| Installation | `cargo` | Binaires précompilés par OS et architecture | Un binaire | Déploiement d'un service |
| Portabilité | Toutes cibles Rust | Limitée aux binaires publiés | Limitée aux binaires publiés | Indépendante du client |
| Maintenance | Faible | Élevée (une chaîne de build par écosystème) | Faible | Moyenne (exploitation) |
| Erreurs | Types Rust | Exceptions natives mappées depuis les diagnostics | Codes de sortie + rapport JSON | Codes HTTP + rapport JSON |
| Sécurité | Celle de l'application | Celle de l'application | Le processus hérite des droits de l'utilisateur | Auth, limites, SSRF, isolation à prévoir |

## Ordre proposé

1. **Node.js (phase 6)** : `generate(config, records) -> report` et un writer
   en flux. Tests : mêmes fixtures que Rust et la CLI, comparaison octet par
   octet (la sérialisation est déterministe).
2. **Python (phase 7)** : même surface, mêmes fixtures.
3. **ABI C (`sitemap-ffi`)** seulement si Go, .NET ou PHP-FFI le justifient,
   avec une interface minimale (`generate_json(const char*) -> char*`).
