# ADR-0013 — Les fichiers publiés comme état du mode `delta`

- Statut : accepté (2026-11-22)
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

## Mise à jour du 2026-11-22 : phase D implémentée

- Le test d'aller-retour couvre maintenant 3 000 entrées aléatoires (tous
  les champs et extensions, caractères à échapper, texte non ASCII, espaces
  autour des valeurs) : `tests/roundtrip.rs`.
- `Engine::apply_changes` et la commande `apply` appliquent un flux
  `upsert` / `delete`. Les partitions touchées sont relues, comparées au
  snapshot par une empreinte de séquence (BLAKE3 sur les couples `loc`,
  empreinte de contenu, dans l'ordre des URL), modifiées et réécrites ;
  les autres restent publiées telles quelles, avec leurs compteurs tirés du
  manifest et leur `lastmod` tiré de l'index.
- Refus avant toute écriture (`EngineError::DeltaRefused`) : stratégie
  `sequential`, pas de snapshot, réglages changés, manifest absent,
  réciprocité hreflang activée, fichier touché modifié hors du moteur ou
  contenant des éléments non relus, partitions non nommées `<stem>-<n>.xml`.
- Oracle I1 : 5 suites de 30 flux aléatoires sur les deux stratégies par
  hachage donnent exactement les octets d'une génération complète triée ;
  vérifié aussi à 1 M d'URL (26 fichiers identiques).
- Le coût mesuré confirme la conséquence prévue : `delta` ne gagne que si
  peu de partitions sont touchées. Le planner ne choisit pas encore
  automatiquement entre `delta` et `skip-unchanged` : c'est la commande
  qui décide.

## Acceptation (2026-11-22)

Accepté par le mainteneur le 2026-11-22, après une revue qui a vérifié la
décision contre le code et les tests (R18 du registre des risques). Depuis la dernière mise à jour :

- `explain --changes` (lot B) compare `apply` (`delta`) et une
  régénération pour un flux donné, avec les préconditions de chacun. Il
  n'y a toujours pas de bascule automatique de l'un vers l'autre : un flux
  refusé n'est jamais ignoré en silence, c'est voulu ;
- le flux tenu en mémoire est borné (`state.maxFeedOperations`, lot C) ;
- `doctor` (lot E) vérifie hors exécution que les fichiers publiés
  correspondent au manifeste et au snapshot, condition d'un `apply`.
