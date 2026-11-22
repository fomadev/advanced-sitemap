# Génération incrémentale et partitionnement stable

Statut : proposition. Voir les invariants dans le [README](README.md).

## 1. Principe

Un fichier sitemap est l'unité de publication : si une seule de ses
entrées change, tout le fichier est réécrit. La génération incrémentale
consiste donc à **réduire le nombre de fichiers touchés**, ce qui dépend
entièrement de la façon dont les URL sont réparties.

Le résultat doit rester identique à une régénération complète (I1). La
génération incrémentale est une optimisation, jamais un autre algorithme.

## 2. Modes

| Mode | Source | Ce qui est évité | Phase |
|---|---|---|---|
| `full` | Complète | Rien (comportement actuel) | existe |
| `skip-unchanged` | Complète | Publication des fichiers identiques (écriture finale, `fsync`, renommage) | A |
| `delta` | Flux de changements | Lecture de la source complète et réencodage des partitions non touchées | D |

Le planner choisit le mode et explique pourquoi (voir
[planner](planner-and-explain.md)). L'utilisateur peut forcer `full`.

## 3. Stratégie `sequential`

Les fichiers sont remplis dans l'ordre d'entrée. Insérer une URL au début
décale toutes les suivantes : **tous les fichiers changent**. Conséquences :

- `skip-unchanged` n'aide que si les changements sont en fin de flux (site
  en ajout seul, trié par date de création) ou si rien ne change ;
- `delta` est **refusé** avec `sequential` : appliquer un changement ne peut
  pas reproduire les octets d'une régénération complète sans connaître
  l'ordre exact de la source (I1) ;
- I1 exige que la source fournisse ses enregistrements dans un ordre
  stable. Le moteur ne peut pas le vérifier ; la documentation le dira et
  le diff le révélera (beaucoup de fichiers modifiés pour peu d'URL).

## 4. Stratégies par hachage

### 4.1 `stable-hash` (existe déjà)

`partition = fnv1a64(loc) mod N`. Une URL ne bouge pas tant que N est
constant. Mais changer N déplace presque tout. Mesuré sur 200 000 URL :

| N → N+1 | URL déplacées avec `mod N` | avec jump hash | 1/(N+1) |
|---|---|---|---|
| 4 → 5 | 80,0 % | 19,96 % | 20,00 % |
| 20 → 21 | 95,3 % | 4,76 % | 4,76 % |
| 100 → 101 | 99,0 % | 0,99 % | 0,99 % |

### 4.2 `consistent-hash` (proposé, ADR-0012)

`partition = jump(fnv1a64(loc), N)`, l'algorithme « jump consistent hash »
(Lamping et Veach, 2014) : 10 lignes, sans table, sans dépendance. Quand N
passe à N+1, seule une fraction 1/(N+1) des URL change de partition (voir le
tableau), toutes vers la nouvelle partition. Répartition mesurée sur 20
partitions et 200 000 URL : entre 9 845 et 10 112 URL par partition (moyenne
10 000), contre 9 921 et 10 046 avec `mod N`.

Ce n'est **pas** un remplacement de `stable-hash` : changer de stratégie
déplace toutes les URL une fois. Les deux restent disponibles et figées par
des vecteurs de test.

### 4.3 Saturation

Une partition pleine fait échouer la génération (`PARTITION_LIMIT_EXCEEDED`,
ADR-0005) parce que déplacer une URL casserait la stabilité. Avec un
hachage, le nombre d'URL par partition suit à peu près une loi binomiale :
écart-type d'environ √(moyenne), soit 200 URL pour 40 000 URL en moyenne. La
marge de 20 % suggérée par `explain` (déjà en place) laisse donc plus de
40 écarts-types côté nombre d'URL. La contrainte la plus serrée est souvent
la taille en octets ou la limite de 1 000 news par fichier : le planner doit
surveiller ces trois réserves (section 4 du [planner](planner-and-explain.md)).

## 5. Déroulé d'une exécution `skip-unchanged`

1. Le planner lit le snapshot et vérifie l'empreinte de configuration
   (I7). Si elle diffère ou si le snapshot manque, il passe en `full`.
2. Le moteur lit, valide, encode et route exactement comme aujourd'hui.
   L'empreinte de chaque partition est calculée pendant l'écriture dans la
   zone temporaire.
3. À la fin, pour chaque partition : identique au snapshot, le fichier
   temporaire est abandonné ; sinon il est publié.
4. L'index est publié s'il change ; robots.txt aussi ; puis le manifest ;
   puis le snapshot (I3).
5. Le rapport liste les fichiers publiés et ceux qui sont inchangés.
   `STALE_ARTIFACT` fonctionne comme aujourd'hui (I5).

Effet sur l'index : le `lastmod` d'une partition est le plus grand `lastmod`
de ses entrées (existe déjà). Une partition inchangée garde son `lastmod`,
donc les moteurs de recherche ne retéléchargent que les fichiers modifiés.

## 6. Reprise après panne

La publication n'est pas atomique sur plusieurs fichiers (un renommage par
fichier). L'ordre partitions, index, manifest, snapshot garantit :

| Panne après | État visible | Exécution suivante |
|---|---|---|
| quelques partitions | Ancien index ; il pointe vers des fichiers anciens ou nouveaux, tous valides | Le snapshot est l'ancien : les partitions déjà publiées sont recomparées, identiques au nouveau contenu, donc ignorées ; les autres sont publiées |
| l'index | Nouvel index, anciennes empreintes au snapshot | Idem, converge |
| le manifest | Tout est publié sauf le snapshot | Tout est recomparé, rien n'est republié, le snapshot est écrit |

Le harnais de test injectera une panne entre chaque publication et
vérifiera I1 et I4 après l'exécution suivante.

## 7. Mode `delta` (phase D)

Voir [ADR-0013](../adr/0013-published-files-as-state.md). Résumé :

- entrée : un flux d'opérations `upsert(record)` et `delete(loc)` ;
- chaque opération est validée et routée vers sa partition ;
- seules les partitions touchées sont relues depuis les fichiers publiés
  (avec `sitemap-reader`), modifiées, triées dans un ordre canonique et
  réencodées ;
- l'ordre canonique dans une partition est l'ordre des clés d'URL, sinon
  I1 serait impossible sans connaître l'ordre de la source ; ce mode
  impose donc un ordre de sortie trié, ce qui en fait une **option
  distincte**, jamais un changement silencieux de la sortie actuelle.

État (2026-11-22) : implémenté (`advanced-sitemap apply`). Voir la mise à
jour de l'ADR-0013 et les mesures dans `docs/performance/README.md`.

## 8. Tests prévus

| Test | Vérifie |
|---|---|
| Oracle différentiel : 1 000 séquences aléatoires d'ajouts, retraits et modifications, comparaison avec une régénération complète | I1, I10 |
| Exécution relancée sans changement : 0 fichier publié | Phase A |
| Injection de pannes à chaque étape de publication | I3, I4 |
| Changement de chaque paramètre de la configuration canonique | I7 |
| Vecteurs de test de `jump` et de `digest128` sur les trois OS | I6 |
| Fraction d'URL déplacées quand N change, comparée à 1/(N+1) | ADR-0012 |
