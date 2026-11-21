# Analyse des algorithmes existants

## 1. Pipeline par entrée (`core/pipeline.ts`)

Ordre exact des étapes dans `processEntry()` :

1. Copie superficielle de l'entrée ; alias `video` → `videos`.
2. Exclusion staging (si activée) : l'entrée est **supprimée** de la sortie.
3. Normalisation du slash final sur `url` et `alternates[].href`.
4. Suppression/expiration : `isDeleted` ou `expiresAt <= Date.now()` →
   priority 0, changefreq `never`, images/vidéos retirées, commentaire XML.
   Si le chemin est `/`, exception (garde-fou page d'accueil).
5. Validation croisée (vidéo live avec durée, abonnement + achat `own`,
   news > 48 h) : exception.
6. Mode strict : exception à la première violation.

La validation des URL (`sanitizeAndValidateUrl`) n'a lieu **qu'au moment de
la sérialisation**, dans les builders. Conséquences :

- en streaming, une URL invalide fait échouer le flux **après** l'envoi
  partiel du document au client : la réponse HTTP est tronquée ;
- l'utilisateur ne reçoit qu'une erreur à la fois.

**Invariant à conserver** : une sortie produite est toujours un document
dont chaque `loc` a été validé. **Invariant à ajouter** : aucun octet n'est
publié tant que la validation de l'ensemble n'est pas terminée (ou alors la
publication passe par une zone temporaire).

## 2. Validation d'URL

`sanitizeAndValidateUrl` : `trim()`, préfixe `http://` ou `https://`, refus
des espaces, `URL.canParse`. La chaîne d'origine (trimée) est émise : les
caractères non ASCII ne sont pas percent-encodés, alors que sitemaps.org
demande des URL échappées. La longueur maximale (2 048 caractères) n'est pas
contrôlée.

**Choix Rust** : analyse WHATWG via la crate `url`, émission de la forme
sérialisée WHATWG (ASCII, IDNA, percent-encodage), refus des espaces et
caractères de contrôle dans l'entrée brute, longueur maximale contrôlée après
sérialisation. La normalisation WHATWG ne fusionne pas deux URL
sémantiquement différentes (elle ne touche ni à la casse du chemin, ni à la
requête, ni au slash final) ; voir ADR-0004.

## 3. Sérialisation XML

Concaténation de chaînes avec indentation fixe (2 espaces), `\n`, sans
déclaration conditionnelle des namespaces. Échappement en une passe des 5
caractères réservés. Les caractères interdits par XML 1.0 (U+0000–U+0008,
U+000B, U+000C, U+000E–U+001F, U+FFFE, U+FFFF) ne sont pas rejetés : un titre
contenant `\u0001` produit un document mal formé.

Autre écart relevé pendant l'implémentation : `buildUrlBaseXml` émet les
`<xhtml:link>` immédiatement après `<loc>`, avant `<lastmod>`. Le schéma
XSD de sitemaps.org (`xsd:sequence` loc, lastmod, changefreq, priority, puis
`xsd:any ##other`) place les éléments d'extension **après** `priority` : un
validateur XSD strict refuse donc la sortie actuelle. La tolérance des
moteurs de recherche à cet ordre n'a pas été vérifiée ici ; le moteur suit
l'ordre du schéma.

**Choix Rust** : écrivain en flux sur `io::Write` avec compteur d'octets,
échappement qui renvoie une erreur sur caractère interdit, sortie
déterministe (aucune horloge, ordre d'entrée préservé).

## 4. Garde-fous de volume et découpage

- 50 000 entrées maximum par document (exception), 50 000 sitemaps par index.
- `chunkSitemapEntries(entries, size)` découpe un tableau. La taille en octets
  n'est jamais calculée côté génération ; seule la CLI avertit au-delà de
  50 Mo **après** coup.

**Choix Rust — partitionnement séquentiel exact** : chaque bloc `<url>` est
d'abord sérialisé dans un tampon réutilisé ; on connaît donc sa taille exacte
avant de l'écrire. On ouvre une nouvelle partition si
`octets_courants + bloc + pied > max_bytes` ou si `compte == max_urls`.
Complexité O(n) en temps, O(1) en mémoire (hors ensemble de déduplication),
aucune estimation.

**Partitionnement stable par hachage** : `partition = fnv1a64(loc) mod N`.
Une URL reste dans la même partition tant que N ne change pas, ce qui limite
les fichiers à régénérer lors d'une mise à jour incrémentale. Inconvénient :
remplissage inégal ; un dépassement de limite dans une partition est une
erreur explicite (augmenter N). Le hachage FNV-1a 64 bits est implémenté
dans le dépôt (aucune dépendance) et figé par des vecteurs de test : il ne
doit jamais changer, sinon toutes les partitions persistées se déplacent.

## 5. Diff (`utils/diff.ts`)

Indexation par `url` dans deux `Map`, comparaison de `lastmod` (normalisé en
ISO), `priority`, `changefreq`, **nombre** d'images et de vidéos,
`isDeleted`. Une image remplacée par une autre n'est pas détectée.
Complexité O(n + m), mémoire O(n + m).

**Choix Rust (phase 4)** : empreinte FNV-1a 64 de la forme canonique
sérialisée de chaque entrée (mêmes octets que la sortie XML), comparaison
d'empreintes par `loc`, puis projection sur les partitions pour savoir
lesquelles régénérer.

## 6. Analyse (`utils/analyze.ts`)

Score = 100 − min(30, % doublons) − min(20, % lastmod manquants × 0,2).
Heuristique non documentée ailleurs, non reliée aux exigences des moteurs.
Remplacée par des diagnostics codés et des compteurs ; un score éventuel
sera une vue dérivée documentée.

## 7. Robots.txt

Concaténation des groupes, normalisation des chemins (`/` préfixé),
déduction de `Host` depuis le premier sitemap, nettoyage final (CRLF → LF,
espaces de fin, au plus une ligne vide). Le commentaire d'en-tête contient
l'heure courante.

## 8. Scanner App Router

Parcours récursif synchrone, détection de `page.(tsx|ts|jsx|js)`, groupes
`(x)` retirés du chemin, segments `[x]` marqués dynamiques, expansion de
`[lang]`/`[locale]`, exclusion par préfixe ou joker final, extraction de
`title`/`description` par regex, cache de 5 s en développement. Purement
spécifique à Next.js : il reste dans l'adaptateur et alimentera le moteur via
le contrat de source.

## 9. Lecture de sitemaps (backend)

Le backend est la seule implémentation robuste : taille bornée avant et
après gunzip (`maxOutputLength`), validation XML jusqu'à 20 Mo, parsing en
mémoire, suivi de 200 sitemaps enfants maximum, déduplication, codes
d'anomalies. C'est la référence conceptuelle pour le futur `inspect` en flux.

## 10. Horloge et déterminisme

Lectures de l'horloge : `autoLastmod`, `expiresAt`, règle news 48 h,
commentaire robots, `autoLastmod` de l'index. **Décision** : le moteur Rust
ne lit jamais l'horloge implicitement. Les fonctionnalités qui en ont besoin
reçoivent une valeur explicite (`now` fourni par l'appelant), ce qui rend les
tests et la sortie reproductibles.
