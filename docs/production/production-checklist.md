# Liste de contrôle d'exploitation

Pour mettre Advanced Sitemap en production et l'exploiter. Chaque point
renvoie à la documentation qui le détaille.

## Avant la première exécution

- [ ] Écrire un document de configuration versionné (`schemaVersion`
      `1.0`), plutôt que des options en ligne de commande : il est validé
      en entier, et un champ mal orthographié est refusé
      ([docs/cli](../cli/README.md#configuration-rules)).
- [ ] Fixer `site` et `publicBaseUrl` (obligatoire dès qu'un index est
      produit).
- [ ] Choisir la stratégie de partitionnement : `consistent-hash` si le
      nombre de partitions doit pouvoir grandir sans tout déplacer, et si
      `apply` est envisagé ; `explain` propose un nombre de partitions.
- [ ] Fixer `referenceTime` si les dates futures doivent être signalées
      (le moteur ne lit jamais l'horloge).
- [ ] Prévoir un dossier d'état (`state.directory`) hors du dossier publié
      si `apply` ou le rapport de changements sont utilisés.
- [ ] Vérifier la mémoire disponible : environ 17 à 34 octets par URL avec
      la déduplication (415 Mio à 10 M d'URL), 5 Mio sans
      ([limites](../concepts/limits.md)).
- [ ] Exclure des déploiements `.advanced-sitemap.lock`, `*.lock` et
      `.advanced-sitemap-staging-*`.
- [ ] `advanced-sitemap explain` puis `advanced-sitemap validate` sur les
      données réelles, avant le premier `generate`.

## À chaque exécution

- [ ] Lancer `generate` (ou `apply`) avec `--format json` et lire `status`
      et le code de sortie : 0 succès, 1 entrée invalide, 2 configuration ou
      état changé, 3 E/S ou dossier occupé (`busy`), 4 erreur interne.
- [ ] Sur `busy` ou `state-changed` (`EngineError::is_retryable`) :
      relancer plus tard, ou relancer `explain`.
- [ ] Ne jamais lancer deux exécutions sur le même dossier par un autre
      moyen que le binaire : le verrou ne protège que contre lui-même.
- [ ] Pour une exécution planifiée : `explain` puis `generate
      --expect-state <stateToken>` refuse de publier si l'état a changé
      entre-temps.

## Surveillance

- [ ] `advanced-sitemap doctor --config ... --deny-warnings` dans la
      supervision : il ne modifie rien et sort en code 1 si un fichier
      publié ne correspond plus au manifeste, si un snapshot est périmé ou
      si une exécution interrompue a laissé du staging.
- [ ] Suivre les diagnostics `STALE_ARTIFACT` (fichiers qui ne sont plus
      produits, jamais supprimés automatiquement) et `SNAPSHOT_STALE`.

## Après un incident

- [ ] Arrêt brutal, coupure, erreur d'E/S pendant la publication :
      lancer `doctor`. Un ensemble mixte (`files` en erreur) est réparé par
      le `generate` suivant ; un staging restant est supprimé par la
      prochaine exécution.
- [ ] Windows, « cannot replace ... another program may hold it open » :
      identifier le programme (serveur web, antivirus), relancer une fois
      le fichier libéré.
- [ ] `apply` refusé (`delta-refused`) : lancer un `generate` complet ;
      le message dit pourquoi.

## Mise à jour du moteur

- [ ] Lire le CHANGELOG (section **Breaking** pour les appels Rust).
- [ ] Le premier `generate` après la mise à jour réécrit tous les fichiers
      et remplace le snapshot ; `apply` est refusé jusque-là
      ([semver-policy.md](semver-policy.md)).
- [ ] Vérifier le binaire téléchargé avec `SHA256SUMS`.

## Sauvegarde

- [ ] Le dossier publié se reconstruit à partir des données sources.
- [ ] Le dossier d'état aussi (un `generate` complet le réécrit) ; le
      sauvegarder évite seulement le coût de cette régénération.
