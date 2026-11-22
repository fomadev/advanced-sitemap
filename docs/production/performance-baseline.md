# Référence de performance

Ces chiffres sont la référence avant les lots C (performance) et suivants.
Ils viennent de [docs/performance](../performance/README.md), où se trouvent
le protocole et le détail. Une seule machine : portable Intel Core
i5-6300U (2015, 2 cœurs), 11,9 Go, Windows 11, disque E: lent, moteur
monothread. Ce sont des ordres de grandeur, pas des garanties.

## Volumes réellement testés

| Volume | Testé | Commandes |
|---|---|---|
| 10 000 URL | Oui (tests, Criterion) | toutes |
| 100 000 URL | Oui (Criterion) | pipeline |
| 1 000 000 URL | Oui | `validate`, `generate`, `--skip-unchanged`, `--state-dir`, `inspect`, `diff`, `apply` |
| 5 000 000 URL | Oui | `validate`, `generate`, `--skip-unchanged`, `--state-dir` (validate) |
| 10 000 000 URL | **Non** | — |
| 50 000 000 URL | **Non** | Taille du snapshot extrapolée : environ 2,2 Go |

## Chiffres clés

| Mesure | 1 M d'URL | 5 M d'URL |
|---|---|---|
| `generate` complet | 16,3 s, 57 Mio | 80,4 s, 209 Mio |
| `validate` (déduplication) | 7,1 s, 56 Mio | 23,6 s, 210 Mio |
| `validate --no-dedupe` | 5,0 s, 5 Mio | 31,3 s, 5 Mio |
| Relance `--skip-unchanged`, sans changement | 3,3 s | 22,6 s |
| Surcoût de `--state-dir` (snapshot) | +3,0 à +3,4 s | +17 s (avant l'optimisation du lot précédent) |
| Taille du snapshot | 44,8 Mo (45 octets par URL) | — |
| `inspect` (relecture) | 9,5 s | — |
| `diff` de deux sorties | 26 s (cache chaud) | — |
| `apply`, 1 opération (24 partitions) | 3,3 s, 50 Mio | — |
| `explain` avec coût de migration | 0,7 s, 5 Mio | — |

## Coûts à connaître

- La mémoire ne croît qu'avec la déduplication (17 à 34 octets par URL
  distincte) et la réciprocité hreflang (110 octets par lien).
- `generate` est limité par l'écriture disque (`validate` est 2 à 3 fois
  plus rapide).
- Le coût fixe du snapshot est dominé par sa synchronisation disque
  (1,1 à 1,7 s à 1 M sur ce disque), conservée volontairement.
- `apply` n'est intéressant que si le flux touche peu de partitions.

## Non mesuré

Allocations, octets lus et écrits par étape, disques SSD/NVMe, machines
multicœurs, Linux et macOS (les mesures sont toutes sous Windows).
