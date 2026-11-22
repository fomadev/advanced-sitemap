# ADR-0013 — Les fichiers publiés comme état du mode `delta`

- Statut : proposé (phase D, après A à C)
- Date : 2026-11-21

## Contexte

Appliquer un flux de changements (`upsert`, `delete`) sans relire toute la
source exige de connaître le contenu actuel des partitions touchées. Deux
choix : une base d'état séparée (copie de toutes les entrées), ou relire les
fichiers déjà publiés.

## Décision proposée

- Les fichiers sitemap publiés **sont** l'état : une partition touchée est
  relue avec `sitemap-reader` (déjà en place et borné), les opérations sont
  appliquées, puis la partition est réencodée.
- Condition de validité : l'aller-retour est exact,
  `encode(read(encode(e))) == encode(e)` pour toute entrée valide. Cette
  propriété devient un test de propriété obligatoire avant la phase D,
  couvrant toutes les extensions (alternates, images, vidéos, news).
- Le snapshot de niveau URL (ADR-0011) dit quelle partition contient une
  clé et sert à vérifier que le fichier relu est bien celui attendu
  (empreinte de partition). Une différence signifie que le fichier a été
  modifié hors du moteur : le planner refuse `delta` et propose `full`.
- Dans une partition, les entrées sont réécrites dans l'ordre des clés
  d'URL. Ce mode produit donc une sortie triée, différente de l'ordre de la
  source : c'est une option explicite, et l'oracle I1 le compare à une
  régénération complète **triée de la même façon**.

## Conséquences

- Aucune base de données à maintenir ni à synchroniser.
- Les champs que le moteur ne relit pas (extensions inconnues) seraient
  perdus : le mode `delta` est refusé si `inspect` signale des
  `unsupportedExtensions` dans une partition touchée.
- Coût d'une opération : relire et réécrire une partition (au plus 50 Mio).
  Pour beaucoup de changements dispersés, `skip-unchanged` peut être plus
  rapide ; le planner compare les deux coûts.

## Mise à jour du 2026-11-22

Le test de propriété d'aller-retour existe
(`reading_and_reencoding_is_exact_for_every_fixture`) : pour chaque fixture
de conformance (y compris alternates, images, vidéos et news), relire le
fichier puis réencoder ses entrées redonne exactement les blocs `<url>`
générés. Il devra être étendu à des entrées aléatoires avant la phase D.
