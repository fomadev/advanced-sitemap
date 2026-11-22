# Rapport de performance

État au 2026-11-22, après le lot F. Synthèse des mesures des lots
précédents ; le protocole et le détail sont dans
[docs/performance](../performance/README.md), la référence avant
optimisation dans [performance-baseline.md](performance-baseline.md).

## 1. Conditions

Une seule machine de mesure : portable Intel Core i5-6300U (2015, 2
cœurs), 11,9 Go, Windows 11, disque lent dont la vitesse varie jusqu'à 3
fois d'un jour à l'autre. Les comparaisons ne valent qu'à l'intérieur d'une
même série. Moteur monothread. Ces chiffres sont des ordres de grandeur.

## 2. Résultats

| Mesure | 1 M d'URL | 10 M d'URL |
|---|---|---|
| `generate` complet | 16,3 s, 57 Mio | 80,8 s, 415 Mio |
| `validate` (déduplication) | 7,1 s, 56 Mio | 54,4 s, 416 Mio |
| `validate --no-dedupe` | 5,0 s, 5 Mio | 29,6 s, 5 Mio |
| Relance `auto` sans changement | 3,3 s | 58,9 s |
| Relance avec snapshot | +3,0 à +3,4 s | 76,6 à 80,4 s, 461 Mio |
| Taille du snapshot | 44,8 Mo | 448 Mo |
| `apply` d'une opération (24 partitions) | 3,3 s, 50 Mio | — |
| `explain` avec coût de migration | 0,7 s, 5 Mio | — |

À retenir :

- la mémoire ne dépend que de la déduplication et de la réciprocité
  hreflang ; sans elles, elle reste à 5 Mio quel que soit le volume ;
- `generate` est limité par l'écriture disque ;
- l'analyse des URL représente environ deux tiers du temps de
  validation (5,7 allocations par URL mesurées) ;
- le coût fixe du snapshot est surtout sa synchronisation disque,
  conservée volontairement.

## 3. Changements de performance par lot

| Lot | Changement | Mesure avant / après |
|---|---|---|
| Avant A | Snapshot lu, haché et copié par blocs de 64 Kio (même format) | Surcoût du snapshot à 1 M : 4 à 5,5 s avant, 3,0 à 3,4 s après ; lecture 0,61-0,70 s → 0,26 s |
| C | Modèle de coût recalibré sur une série homogène | Surestimation des générations complètes ramenée de 2 fois à 26 % à 10 M |
| C | Borne du flux d'`apply` | 806 octets par opération mesurés ; aucun effet sur le temps |
| D | Contrôle des caractères interdits dans les textes d'extensions | Un passage par texte ; non mesurable sur le banc |
| E | Constructeurs, `#[non_exhaustive]` | Aucun effet à l'exécution ; une copie d'URL par vidéo validée |
| F | Nouvelle tentative de remplacement d'un fichier tenu ouvert (Windows) | Aucun coût sans blocage ; au plus 2,3 s en cas de blocage |

Aucune optimisation sans mesure n'a été faite au lot F.

## 4. Non mesuré

- Machines multicœurs, disques SSD ou NVMe, Linux et macOS : la CI
  vérifie le fonctionnement sur ces plateformes, pas les performances ;
- 50 M d'URL (place disque) ;
- octets lus et écrits par étape.

Le planner l'indique : sur un autre matériel, ses durées absolues ne sont
pas fiables, seule la comparaison entre modes l'est (R23).
