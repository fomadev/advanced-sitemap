# Revue de la Fordima Public License (FPL) v1.0 pour `advanced-sitemap`

Statut : **décisions prises le 2026-11-22 (section 8).** La licence reste
la FPL v1.0 sans modification ; les clarifications de la section 3 ne sont
pas adoptées et restent une référence. Ce document n'est pas un avis
juridique. Il relève les ambiguïtés
du texte et propose des clarifications. Une relecture par un juriste est
recommandée avant une version 1.1 et avant toute publication de package.

Le fichier [`LICENSE`](../../LICENSE) contient la FPL v1.0 **telle que
fournie**, sans modification. Les clarifications ci-dessous ne s'appliquent
pas tant qu'elles n'ont pas été approuvées et intégrées dans une nouvelle
version du texte.

## 1. Nature de la licence

La FPL est une licence **source-available**, pas une licence open source :

- l'Open Source Definition de l'OSI interdit de discriminer des usages ou
  des domaines d'activité (critères 5 et 6) et exige la libre
  redistribution (critère 1). Les sections 4 et 5 de la FPL restreignent
  certains usages commerciaux, la redistribution et les forks ;
- elle n'est donc pas approuvée par l'OSI et n'a pas d'identifiant SPDX.

Conséquences pratiques :

| Public | Effet probable |
|---|---|
| Développeurs individuels, agences, sites commerciaux | Aucun frein : la section 1 autorise l'usage gratuit, y compris commercial |
| Entreprises avec une politique « OSI uniquement » | Revue juridique interne nécessaire, parfois refus |
| Distributions Linux (Debian, Fedora), Homebrew core | Refus probable dans les dépôts principaux (critères DFSG / OSI) |
| Frameworks qui voudraient intégrer le moteur dans leur cœur | Bloqués par la section 4 d), sauf licence commerciale |

Le projet doit donc se présenter comme **« source-available »** et jamais
comme « open source ». Le README le dit désormais explicitement.

## 2. Ambiguïtés relevées

### 2.1 Usage commercial (section 1) et produit concurrent (section 4 a)

La section 1 autorise l'intégration « inside commercial ... products or
services ». La section 4 a) exige une licence payante pour un « tool, or
service that is similar to ... Advanced Sitemap ».

Cas incertain : un CMS commercial, une plateforme e-commerce ou un outil SEO
qui génère des sitemaps pour ses clients **avec** le moteur. Fait-il un
« service similaire » ? Le texte actuel permet les deux lectures.

### 2.2 Bindings et adaptateurs (sections 3 et 5)

La section 3 autorise les wrappers et bindings « for your own use ». La
section 5 interdit de publier « a competing package or binding » sur un
registre. Un adaptateur communautaire gratuit pour Laravel, Django ou Spring
qui **appelle** le moteur officiel n'est ni clairement autorisé ni
clairement interdit. C'est le point le plus bloquant pour l'objectif
polyglotte du projet.

### 2.3 Forks (sections 2 et 5)

La section 2 limite les forks à la contribution. La section 5 interdit les
forks « permanents ». Plusieurs usages légitimes tombent dans la zone grise :
un fork interne d'entreprise qui applique un correctif de sécurité en
attendant la version officielle, un fork d'audit, un fork qui maintient une
version ancienne pour un client.

### 2.4 Produit dérivé (section 4 b) et intégration interne (section 3)

« Port or rewrite of its logic in another language » est très large : toute
réimplémentation d'un algorithme public (protocole sitemaps.org,
partitionnement) pourrait être revendiquée. Le droit d'auteur protège
l'expression du code, pas les idées ni les spécifications publiques. Une
clause trop large est fragile et peut décourager des contributeurs.

### 2.5 Contributions (section 2)

Les contributions sont reçues « under this same license ». Or Fordima vend
des licences commerciales (section 9) qui accordent **plus** de droits que
la FPL. Pour inclure le code d'un contributeur dans une licence commerciale,
il faut que ce contributeur ait accordé un droit de sous-licence, ce que la
section 2 n'accorde pas explicitement. Sans CLA (Contributor License
Agreement) ou clause équivalente, le modèle commercial est fragilisé dès la
première contribution externe.

### 2.6 Titularité des droits

Le texte indique « Copyright (c) 2026 Fordima Inc. ». Le droit d'auteur
appartient à l'auteur du code (ou à son employeur), sauf cession écrite. Si
Fordima Inc. n'est pas encore une personne morale constituée, ou si aucune
cession des droits de l'auteur vers elle n'a été signée, la mention est
inexacte et la société ne peut pas valablement accorder de licence
commerciale. À vérifier par le titulaire ; ce dépôt ne peut pas le
déterminer.

Les dépôts existants mélangent aussi « Fordima Inc. », « Fordi / FomaDev » et
« FomaDev Public License » (voir
[licensing-review.md](../research/licensing-review.md)).

### 2.7 Éléments absents

- **Marque** : le nom « Advanced Sitemap » n'est pas protégé par la licence.
  Les licences source-available courantes protègent le nom par une clause de
  marque plutôt qu'en interdisant les forks.
- **Brevets** : aucune licence de brevet n'est accordée ni réservée.
- **Droit applicable et juridiction** : non précisés.
- **Dépendances tierces** : la licence ne dit pas que les dépendances
  restent sous leur propre licence (c'est le cas par la loi, mais le dire
  évite des malentendus).

## 3. Clarifications proposées (projet de FPL v1.1)

Le texte ci-dessous est un **projet**. Il conserve les cinq principes
demandés : usage gratuit y compris commercial, code public et contributions,
protection contre les moteurs concurrents dérivés, licence payante pour
certaines redistributions et offres SaaS, protection des droits de Fordima.

### 3.1 Ajouter des définitions (nouvelle section 0)

> **"Software"** means the source code, compiled forms, libraries,
> command-line tools and official language bindings published by the
> Licensor in the official repository.
>
> **"Competing Offering"** means a product or service, offered to third
> parties, whose **primary purpose** is to provide sitemap generation,
> validation, optimization or publication as a standalone engine, library,
> tool or hosted service, **and** which includes the Software or a modified
> version of it, or is derived from its source code.
>
> **"Adapter"** means software that calls the unmodified Software, through
> its public API, command-line interface or JSON protocol, without copying
> or re-implementing its source code.

Justification : 2.1, 2.2 et 2.4. Le critère « primary purpose » distingue un
composant d'une application (autorisé) d'un moteur concurrent (sous
licence).

### 3.2 Section 1 : préciser l'usage commercial autorisé

Ajouter :

> Using the Software as a component of a larger product or service whose
> primary purpose is not sitemap generation (for example a CMS, an
> e-commerce platform, a website builder or an SEO suite that generates
> sitemaps for its own users) is free use under this Section and is not a
> Competing Offering.

### 3.3 Section 3 : autoriser les adaptateurs publiés

Remplacer la dernière phrase par :

> You may write, use, publish and distribute Adapters, free of charge or
> commercially, including on package registries, provided that they (a) are
> not presented as official, (b) do not bundle a modified version of the
> Software, and (c) do not use "Advanced Sitemap" as their name, except in
> a descriptive form such as "Laravel adapter for Advanced Sitemap".

### 3.4 Section 4 : resserrer sur les offres concurrentes

- a) remplacer « similar to, or competes with » par « a Competing
  Offering » ;
- b) remplacer « including a port or rewrite of its logic in another
  language » par « including a translation of its source code into another
  programming language ». Une réimplémentation indépendante d'une
  spécification publique n'est pas visée ;
- c) peut être supprimé : il est couvert par a), b) et d) ;
- e) préciser : « a hosted service whose primary purpose is sitemap
  generation, validation or publication for third parties, based on the
  Software ».

### 3.5 Section 5 : distinguer forks privés et distributions concurrentes

Remplacer par :

> You may keep private or internal forks for development, security fixes,
> audits, testing, or maintenance of your own deployments. You may not
> publish or distribute a fork, or a package built from a fork, as a
> substitute for the official Software, on any registry or marketplace
> (such as crates.io, npm or PyPI), without a commercial license. Forks
> made to prepare a contribution to the official repository are always
> allowed.

### 3.6 Section 2 : licence des contributions

Ajouter :

> By submitting a contribution, you grant the Licensor a perpetual,
> worldwide, non-exclusive, royalty-free, irrevocable license to use,
> modify, sublicense and distribute your contribution, including under
> commercial licenses, and you confirm that you have the right to grant
> it.

Alternative : un CLA séparé signé à la première Pull Request.

### 3.7 Nouvelles sections

- **Trademarks** : la licence n'accorde aucun droit sur le nom « Advanced
  Sitemap », sauf l'usage descriptif nécessaire pour indiquer la
  compatibilité.
- **Third-party components** : les dépendances restent soumises à leurs
  propres licences, listées dans les notices fournies avec chaque binaire.
- **Governing law** : à choisir par le titulaire.

### 3.8 Mention de copyright

Proposition, selon la situation réelle :

- si Fordima Inc. est constituée et a reçu la cession des droits :
  `Copyright (c) 2026 Fordima Inc.` (inchangé) ;
- sinon : `Copyright (c) 2026 <nom légal de l'auteur>`, avec « FomaDev »
  comme nom de projet ou d'éditeur dans le README, et cession à Fordima Inc.
  le jour où elle existe.

## 4. Dépendances Rust

Toutes les dépendances du graphe sont sous licence permissive, contrôlées
par `cargo deny` (liste `allow` dans [`deny.toml`](../../deny.toml)) :
MIT, Apache-2.0 (avec ou sans LLVM-exception), Unicode-3.0, BSD-3-Clause,
Zlib.

- Ces licences autorisent l'usage dans un projet source-available ou
  propriétaire : **aucune incompatibilité** avec la FPL.
- Obligation : conserver leurs notices de copyright et de licence **dans les
  binaires distribués** (CLI, bindings Node et Python). À faire au moment de
  la première release : générer un fichier de notices tiers (par exemple
  avec `cargo about`) et l'inclure dans chaque archive et package.
- Aucune dépendance copyleft (GPL, LGPL, MPL, AGPL) n'est autorisée par la
  configuration actuelle. Une dépendance copyleft forte serait
  **incompatible** avec la section 4 de la FPL.

## 5. Distribution sur les registres

Aucun package n'est publié ; toutes les crates ont `publish = false`.

| Registre | Déclaration de licence possible | Remarques |
|---|---|---|
| crates.io | `license-file = "LICENSE"` (déjà en place via le workspace) | Le champ `license` attend une expression SPDX ; la FPL n'en a pas. crates.io accepte `license-file` pour une licence non SPDX. L'affichage sera « non-standard ». Les politiques de crates.io ne réservent pas le registre à l'open source, mais à vérifier au moment de publier |
| npm | `"license": "SEE LICENSE IN LICENSE"` | Forme documentée par npm pour une licence non SPDX. Corrige l'incohérence actuelle de `next-advanced-sitemap` (`"FomaDev Public License"`) |
| PyPI | `license = "LicenseRef-FPL-1.0"` et `license-files = ["LICENSE"]` (PEP 639) | `LicenseRef-` est la forme SPDX prévue pour une licence personnalisée. Ne pas utiliser de classifier « OSI Approved » |

Point d'attention commun : la section 5 interdit de publier un package
concurrent. Les packages officiels doivent donc être publiés **uniquement**
depuis les comptes de l'organisation, et leur description doit indiquer
« source-available, Fordima Public License ».

## 6. Ce qui a été appliqué dans le dépôt

- `LICENSE` : FPL v1.0, texte fourni, sans modification.
- `Cargo.toml` : `license-file = "LICENSE"` hérité par toutes les crates,
  `publish = false` conservé.
- `README.md` : section License mise à jour (source-available, pas OSI).
- `deny.toml` : commentaire mis à jour, les crates du workspace restent
  ignorées par la vérification de licences (elles sont privées).

Rien d'autre ne sera modifié tant que les clarifications de la section 3 et
la question de la titularité (2.6) ne sont pas tranchées.

## 7. Décisions attendues du titulaire

1. Approuver, modifier ou refuser chacune des clarifications 3.1 à 3.7.
2. Confirmer le titulaire exact des droits (2.6).
3. Choisir le mécanisme pour les contributions : clause dans la licence
   (3.6) ou CLA séparé.
4. Choisir le droit applicable.
5. Décider si une relecture juridique a lieu avant la v1.1.

Pour comparaison seulement (aucun remplacement n'est proposé) : PolyForm
Shield 1.0.0 et Elastic License 2.0 sont des licences source-available
publiées et relues par des juristes, qui visent le même objectif
(usage libre sauf concurrence). Leur rédaction peut servir de référence
pour formuler les définitions de la section 3.1.

## 8. Décisions du titulaire (2026-11-22)

| Question | Décision | Conséquence |
|---|---|---|
| Titulaire des droits (2.6) | **Fordima Inc.**, société constituée, à qui les droits de l'auteur ont été cédés | La mention `Copyright (c) 2026 Fordima Inc.` reste inchangée |
| Clarifications 3.1 à 3.7 (FPL v1.1) | **Non adoptées : la v1.0 est conservée telle quelle** | Les ambiguïtés de la section 2 restent ; elles sont documentées ici. Les adaptateurs publiés par des tiers (2.2) et les forks internes (2.3) restent en zone grise |
| Licence des contributions (2.5) | **À décider plus tard**, avant d'accepter la première contribution externe | Suivi par R26 du registre des risques ; d'ici là, le code est entièrement écrit pour Fordima |
| Droit applicable et juridiction | **Non précisés** | Déterminés selon les règles générales en cas de litige |

La licence est donc définitive pour une première distribution (R19 clos).
Reste une obligation indépendante de la FPL : joindre aux binaires les
notices des dépendances (section 4), suivie par R27.

