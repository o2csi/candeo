# Conception de l'interface — validée pour la v1

**Maquette de référence** : non publiée

Ce document fige les décisions prises avant d'écrire la moindre ligne de Vue. Il
existe pour que l'implémentation ait une cible, pas pour décrire ce qui existe.

---

## 1. La galerie est l'écran d'accueil

L'application n'ouvre **pas** sur un éditeur. Elle ouvre sur la liste des effets
disponibles, et un bouton « ＋ » entre en édition.

Le motif : la plupart des lancements servent à choisir un effet, pas à en écrire
un. Faire de l'éditeur l'écran d'accueil imposerait un outil de développement à
quelqu'un qui veut juste changer de couleur.

### Trois natures d'effet, distinguées visuellement

| Nature | Origine | Coût | Survit à la fermeture |
|---|---|---|---|
| **Intégré** | Rust, livré avec l'application | boucle hôte | non |
| **À vous** | écrit dans l'éditeur, validé | boucle hôte | oui, via le service |
| **Matériel** | micrologiciel du clavier | **nul** | **oui, toujours** |

La distinction n'est pas cosmétique. Un effet matériel (`Spectrum Cycle`, `Wave`)
continue de tourner machine éteinte et ne coûte aucun temps processeur ; c'est
souvent le bon choix, et l'interface doit le dire.

---

## 2. L'éditeur est un mode, pas un écran séparé

Ouvrir « ＋ » remplace le contenu de la fenêtre : éditeur à gauche, simulateur de
clavier à droite, en permanence visible.

### Monaco, pas CodeMirror

Choisi pour son **service de langage TypeScript** : en chargeant le `.d.ts` de
`@candeo/effects-api`, on obtient types, autocomplétion et erreurs en ligne sans
rien écrire de spécifique. Le poids n'entre pas en compte — l'application est
empaquetée, il n'y a pas de téléchargement à l'usage.

Second bénéfice, découvert après coup et décisif pour le §3 : Monaco n'embarque
pas un analyseur maison mais **le compilateur TypeScript lui-même**. Son
`ts.worker.js` réexporte `typescriptServices` sous le nom `ts` — 6,6 Mo minifiés
de compilateur, déjà payés par l'éditeur. `ts.transpileModule()` est donc
disponible sans rien ajouter.

> L'import passe par le chemin direct de `typescriptServices` et non par
> `ts.worker.js` : ce dernier pose `self.onmessage`, ce qu'un module chargé dans
> la fenêtre n'a aucune raison de faire. Ses **types**, eux, viennent du paquet
> `typescript`, déjà là pour `vue-tsc` — seules ses déclarations sont lues, il ne
> pèse rien dans la construction.

### Comment la déclaration parvient au service de langage

`?raw` lit `packages/effects-api/src/index.ts` **à la construction** et l'inscrit
dans le paquet sous forme de chaîne ; `addExtraLib` la dépose à
`file:///node_modules/@candeo/effects-api/index.ts`, et `paths` y envoie le nom
du paquet. Le fichier reste donc la **seule source** : ni copie, ni `.d.ts`
généré, ni étape de construction à tenir à jour.

Monaco n'exige d'ailleurs pas une déclaration — `addExtraLib` accepte n'importe
quel TypeScript, et le service en tire la même chose. Corps des fonctions
compris, ce qui vaut mieux : l'infobulle de `hsv` montre alors le code réel.

Vérifié en reconstituant l'hôte de l'ouvrier avec le compilateur que Monaco
embarque : sur le modèle de départ, **aucun diagnostic** ; l'infobulle sur `hsv`
rend `(h: number, s: number, v: number): Rgb` ; une faute de frappe est
signalée. La chaîne entière est donc contrôlée, pas supposée.

### Ni DOM ni Node dans l'éditeur

`lib: ['es2020']`, et rien d'autre. Un effet s'exécute dans QuickJS : il n'y a ni
`document`, ni `fetch`, ni même `console`. Les proposer en autocomplétion serait
promettre ce que le moteur ne fournit pas, et l'erreur ne se verrait qu'à la
première image.

`strict` entier, `noImplicitAny` compris. Ni le contrôle ne plie, ni l'auteur
n'a d'incantation à écrire — c'est **`defineEffect` qui résout les deux**.

Un objet littéral n'a aucun type contextuel : écrit nu, ses `render({ layout,
time, frame, params })` sont implicitement `any`, et `strict` les refuse.
Quatre erreurs, sur la façon la plus naturelle d'écrire un effet.

Deux mauvaises réponses ont été essayées avant la bonne :

1. **Désactiver `noImplicitAny`.** L'effet passe, mais `layout` devient `any`
   en silence : on supprime l'autocomplétion dans le seul cas où elle manque,
   c'est-à-dire qu'on renonce à la raison d'avoir choisi Monaco pour éviter un
   message d'erreur.
2. **Exiger `satisfies EffectModule`.** Le typage est correct, mais c'est une
   cérémonie que l'auteur doit connaître, et dont l'oubli produit un message
   incompréhensible. Surtout, elle était **impossible** pour les effets
   intégrés : ce sont des fichiers JavaScript que le moteur exécute tels quels,
   où `satisfies` serait une erreur de syntaxe. Les ouvrir dans l'éditeur
   donnait donc quatre erreurs, sans recours — le chemin « partir d'un effet
   qui marche » était cassé d'origine.

`defineEffect` est l'identité à l'exécution : elle ne coûte rien, reste du
JavaScript valide, et donne le type contextuel. Mieux, elle **déduit le type de
chaque paramètre de sa déclaration** — un paramètre `color` arrive en `Rgb`,
un `number` en `number`, sans transtypage, ce qui serait de toute façon
impossible dans un fichier exécuté tel quel.

Vérifié avec le compilateur embarqué par Monaco : l'API, le modèle de départ et
les quatre effets intégrés rendent **zéro diagnostic** en `strict` complet.

### Les ouvriers sont empaquetés, pas téléchargés

Monaco charge ses ouvriers par `new Worker(...)`. `?worker` de Vite en fait des
actifs du projet, en développement comme dans l'application livrée. Aucun CDN :
l'application s'ouvre hors ligne, ce qui est la moindre des choses pour une
application de bureau.

### Ce que Monaco pèse

| | construction complète | morceau d'entrée |
|---|---|---|
| avant | 354 ko | 87,4 ko |
| après | 14,5 Mo | 89,7 ko |

L'essentiel du poids est l'ouvrier TypeScript (6,9 Mo) et le compilateur chargé
à la validation (3,5 Mo). Mais **le morceau d'entrée ne bouge pas** : l'éditeur
est derrière une route à chargement différé, la galerie ne paie rien. C'est le
seul des deux chiffres qui compte — l'application est empaquetée, il n'y a pas
de téléchargement à l'usage.

### Un effet en cours d'écriture ne se perd pas

Tant qu'il n'est pas validé, un effet n'existe nulle part : `install_effect` est
le seul chemin vers le disque, et il demande du code qui compile. Or on quitte
l'éditeur bien avant d'en être là. L'éditeur enregistre donc en continu dans le
stockage local de la vue web, sous une clé par effet, et restaure à l'ouverture.

Pas de boîte de dialogue « voulez-vous enregistrer ? » : elle pose une question
à laquelle on peut répondre de travers, et une seule fois. Un brouillon restauré
ne perd rien, ne demande rien, et se jette d'un bouton. Un texte identique à la
version enregistrée n'est d'ailleurs pas un brouillon : il est effacé, sans quoi
l'éditeur annoncerait une restauration qui ne restaure rien.

La copie durable, elle, reste le `source.ts` écrit à l'installation.

### Le simulateur est permanent

Une bascule « envoyer au clavier » sépare l'aperçu de l'écriture réelle. Itérer
sur un effet ne doit pas exiger de regarder le vrai clavier, ni d'en posséder un.

Le rendu du simulateur utilise le **dessin physique** des touches (voir §4), pas
la grille logique : un effet spatial ne se juge pas sur une grille régulière.

Mais le simulateur ne **calcule** rien : il reçoit des images déjà produites par
le moteur d'exécution, exactement celles qui partent vers le clavier. C'est tout
l'objet du §3.

---

## 3. Un seul moteur d'exécution, côté Rust

> Cette section dit **pourquoi**. Le cycle de vie détaillé — stockage sur disque,
> boucle de rendu, remontée des images — est dans
> [`effects-runtime.md`](effects-runtime.md).

### La question de départ n'était pas la bonne

Elle était : TypeScript à l'exécution tiendra-t-il la charge ?

**Oui, largement.** 132 LED × 60 images/s = **7 920 couleurs par seconde**. C'est
trivial pour n'importe quel moteur JavaScript, et le franchissement IPC par image
l'est tout autant. La performance n'est **pas** un sujet, et ne l'a jamais été.

Deux contraintes réelles, elles, commandent tout :

1. **Un effet doit tourner fenêtre fermée.** Tant que le rendu vit dans le
   WebView, fermer la fenêtre éteint le clavier.
2. **L'aperçu doit être la production, pas sa ressemblance.**

### La version écartée, et pourquoi

La première rédaction de ce document proposait ceci :

```
Monaco (TS) ─esbuild-wasm─▶ JS ─┬─▶ exécuté dans le WebView  ─▶ simulateur
                                └─▶ IPC ─▶ rquickjs (Rust)   ─▶ clavier
```

**Deux moteurs JavaScript différents exécutent le même effet** — V8 dans le
WebView pour l'aperçu, QuickJS en production. Le simulateur ne montre alors pas
ce que fait le clavier : il montre ce qu'un autre moteur ferait du même code.
L'écart se découvre tard, sur un effet précis, et il est pénible à diagnostiquer.

Ajouter `esbuild-wasm` pour servir ce schéma, c'était payer un transpileur
supplémentaire pour obtenir un défaut.

### Ce qu'on fait à la place

```
Monaco (TS)
   │  ts.transpileModule()          ← déjà embarqué dans Monaco, rien à ajouter
   ▼
  JS ──── IPC install_effect ────▶  rquickjs (Rust)   ← moteur unique
                                          │
                         ┌────────────────┴────────────────┐
                         ▼                                 ▼
              événement « frame » → simulateur      écriture HID → clavier
```

- **La transpilation est gratuite.** `ts.transpileModule()` retire les types,
  rien de plus ; c'est une fonction de texte vers texte, déterministe, sans
  sémantique d'exécution. Peu importe donc où elle a lieu — et Monaco la fournit
  déjà. **Ni `esbuild-wasm`, ni `oxc`, ni `swc` : aucun des trois n'est requis.**
- **`rquickjs`** exécute le JavaScript côté Rust, dans un fil indépendant de la
  fenêtre. Un seul moteur, donc l'aperçu **est** la production, par construction.
- **Le front n'exécute jamais de code utilisateur.** Bénéfice de sûreté qui vient
  gratuitement : un effet ne peut toucher ni le DOM ni l'API Tauri.
- Les imports depuis `@candeo/effects-api` (`hsv`, `mix`…) sont résolus par le
  chargeur de modules de `rquickjs` vers un module interne. **Pas de bundler.**

Le simulateur coûte 396 octets par image, 60 fois par seconde — le chiffre déjà
jugé trivial plus haut. Il transite par un **canal** `tauri::ipc::Channel` et non
par un événement global : la portée est explicite et le binaire passe brut, sans
détour par un tableau JSON d'entiers. Détails dans
[`effects-runtime.md`](effects-runtime.md) §5.

> `boa_engine` (pur Rust, sans C) serait plus simple à embarquer, mais QuickJS
> est nettement plus rapide et plus complet. Pour du code utilisateur arbitraire,
> la couverture du langage compte davantage que l'absence de FFI.

**AssemblyScript / WASM reste écarté** : ce n'est pas TypeScript mais un
sous-ensemble, et l'utilisateur en découvre les limites après avoir écrit son
effet. Le coût d'apprentissage n'est compensé par aucun gain dont on ait besoin.

---

## 4. Le clavier doit ressembler au clavier

La maquette dessine la disposition **ISO pleine taille réelle** : Entrée en L,
pavé numérique détaché, barre d'espace à 6,25 unités, flèches en T inversé.

> **Le périphérique ne déclare pas sa géométrie.** Il n'expose que la grille
> logique 6 × 22. Le dessin réaliste est donc **écrit à la main** à partir de la
> disposition ISO standard, et la correspondance case ↔ touche vient du relevé
> documenté dans [`../protocol/deathstalker-v2-pro.md`](../protocol/deathstalker-v2-pro.md).

Conséquence : un nouveau modèle de clavier demandera un dessin, pas seulement une
matrice. C'est assumé — l'alternative, une grille de carrés, rend le simulateur
inutile pour juger un effet.

---

## 5. Ce qui a été retiré

**L'affichage de l'image courante** (les 132 triplets en clair) a été supprimé de
la maquette. C'est un outil de mise au point, pas une information utile à
l'usage ; sa place est derrière un mode diagnostic, pas dans l'écran principal.

---

## 6. Choix du périphérique

Un seul modèle est pris en charge aujourd'hui, mais l'écran de sélection existe
dès la v1 : la commande `list_devices` renvoie déjà tous les gabarits connus avec
un indicateur de présence, et l'interface doit montrer un périphérique absent
comme absent, pas l'omettre.

---

## 7. Reste à faire pour la v1

- [x] Écran de sélection du périphérique
- [x] Éditeur Monaco + déclaration de `@candeo/effects-api`
- [x] Simulateur — tracé ISO pleine taille, alimenté par les images du moteur
- [x] Validation : `ts.transpileModule()` → commande `install_effect`
- [x] Moteur `rquickjs` côté Rust : fil de rendu indépendant de la fenêtre,
      module interne `@candeo/effects-api`, canal d'images
- [ ] Écran galerie — vignettes animées des effets, et lancement depuis la
      galerie. Les effets écrits sont listés sous « À vous » et s'y rouvrent ;
      il leur manque l'aperçu qui permettrait d'en choisir un sans le lancer.
- [ ] Réglage des paramètres déclarés — ils partent aujourd'hui à leur valeur
      par défaut, lue dans le manifeste

Aucune dépendance nouvelle côté Rust hors `rquickjs`, aucune côté front hors
`monaco-editor`.

### Ce que l'implémentation a précisé

- **Le manifeste est lu, pas exécuté.** `name`, `description` et `params` sont
  relevés dans l'arbre syntaxique, avec le compilateur déjà chargé. Les obtenir
  en évaluant le module reviendrait à exécuter du code d'effet dans la fenêtre,
  ce que le §3 s'interdit. Contrepartie explicite : ces trois champs doivent
  être des **littéraux**, et un nom calculé est refusé avec un message qui le
  dit.
- **La bascule est réappliquée à chaque lancement.** `start_effect` repart d'un
  état neuf, dont la sortie clavier est active ; sans cela, « ne pas envoyer »
  serait oublié au démarrage suivant.
- **Le canal d'images n'existe que pendant un effet.** Il est déposé dans l'état
  de la boucle en cours, et chaque `start_effect` en crée un neuf : l'éditeur se
  réabonne après chaque lancement, pas une fois pour toutes à l'ouverture.
- **Quitter l'éditeur n'arrête pas l'effet.** Le canal libéré coupe le flux
  d'images ; la boucle continue d'alimenter le clavier, fenêtre fermée comprise.
