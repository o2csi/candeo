# Conception de l'interface — validée pour la v1

**Maquette de référence** : non publiée

Ce document fige les décisions prises avant d'écrire la moindre ligne de Vue. Il
existe pour que l'implémentation ait une cible, pas pour décrire ce qui existe.

---

## 1. La galerie est l'écran d'accueil

L'application n'ouvre **pas** sur un éditeur. Elle ouvre sur la liste des effets
disponibles, et un bouton « ＋ » entre en édition. Cette liste est devenue un
écran à trois colonnes : voir le §8.

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

> ⚠️ **Sortir le rendu du WebView est nécessaire, et ne suffit pas.** Un fil ne
> survit pas au processus, et le processus s'arrêtait avec sa dernière fenêtre :
> pendant tout ce temps, la contrainte n° 1 était un argument tenu par la
> conception et démenti par l'exécution. Ce qui la tient réellement, c'est
> l'icône de zone de notification (issue #46) : elle intercepte
> `RunEvent::ExitRequested`, et la croix de la fenêtre **replie** au lieu de
> quitter. Voir [`src/tray.rs`](../../apps/desktop/src-tauri/src/tray.rs).
>
> Les deux moitiés ne se défont pas l'une sans l'autre. Retirer l'icône, c'est
> rendre à nouveau faux tout ce que ce paragraphe justifie.

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
- [x] Écran principal — trois colonnes, lancement d'un effet depuis la liste,
      aperçu dans le panneau de droite (§8). Le repère de couleurs remplace la
      vignette animée : il est prélevé sur le rendu de l'effet, il ne peut donc
      pas décrire autre chose que ce que l'effet fait.
- [x] Réglage des paramètres déclarés — un contrôle par sorte, engendré depuis
      le manifeste, ajusté à chaud et retenu par appareil et par effet (§9)

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
- **Le canal d'images n'existe que pendant un effet, et il vise un appareil.** Il
  est déposé dans l'état de la boucle en cours **de cet appareil**, et chaque
  `start_effect` en crée un neuf : l'éditeur se réabonne après chaque lancement,
  pas une fois pour toutes à l'ouverture. Changer d'appareil ferme le canal
  précédent — le moteur ne le remplacerait pas, et les deux flux alimenteraient
  le même simulateur.
- **Quitter l'éditeur n'arrête pas l'effet.** Le canal libéré coupe le flux
  d'images ; la boucle continue d'alimenter le clavier, fenêtre fermée comprise —
  la croix replie l'application dans la zone de notification, elle ne la quitte
  pas. La quitter, c'est « Quitter candeo » dans le menu de l'icône, et c'est le
  seul geste qui arrête les effets.

---

## 8. L'écran principal est à trois colonnes

**Appareils · Effets · Réglages.** La hiérarchie est celle dans laquelle on
pense : on choisit un appareil, puis son effet, puis ses réglages.
**L'affectation cesse d'être une case à cocher en bas de panneau — elle devient
la structure de l'écran.**

La grille de vignettes qui occupait cet écran a disparu avec elle : elle
présentait des effets sans dire sur quoi ils s'appliqueraient, ce qui n'avait de
sens que tant qu'il n'y avait qu'un appareil possible.

### Les deux premières colonnes se replient, la troisième non

Avec un seul appareil piloté, une colonne entière serait une bande morte
permanente, et c'est l'aperçu qui a besoin de la largeur. La troisième ne se
replie pas : c'est le contenu, il ne resterait rien.

> **Le piège, et la forme qui y résiste.** Repliée, une entrée ne doit montrer
> que son icône — ou son repère de couleurs. La façon naturelle de l'écrire,
> énumérer ce qu'on cache (« cacher le nom, cacher l'effet »), a déjà produit ici
> une collision de spécificité : une règle ajoutée ailleurs pour le nom
> l'emportait sur le masquage, et le texte revenait déborder dans 40 px.
>
> La forme retenue est l'inverse : **masquer tous les enfants au sélecteur
> universel, puis rétablir explicitement le seul qui reste**, et garder par
> `:not(.shut)` toute règle qui pourrait les concurrencer — elle ne *s'applique
> pas* en état replié, au lieu de gagner ou perdre un arbitrage. Un enfant ajouté
> demain est donc masqué sans que personne ait à y penser. La même forme sert
> deux fois : sur les enfants d'une entrée, et sur les enfants du corps de
> colonne.
>
> Les largeurs repliées, elles, sont des **variables** et non des règles
> concurrentes : chaque état écrit la sienne, et le point de rupture étroit
> redéfinit la grille une bonne fois.

### La colonne des appareils

Elle liste les appareils **pilotés**. L'adoption reste dans la vue
Périphériques : choisir ce qu'on configure et choisir ce que candeo a le droit de
piloter sont deux gestes différents, et les fondre ferait d'un clic de sélection
une prise de contrôle.

Elle affiche le **nom du produit**, pas une catégorie — c'est ce qui distingue
deux claviers de la même marque. Il passe à la ligne plutôt que d'être tronqué.
En dessous, l'effet que l'appareil fait tourner.

**Pas de logo de fabricant** : ce sont des marques protégées, elles n'aident pas
à distinguer un clavier d'une souris, et le nom porte déjà l'information. Un
pictogramme de type suffit — et il n'est pas deviné sur le nom : un seul gabarit
est connu, c'est un clavier ; le jour où le Rust déclarera un type, il viendra de
là.

### Un seul effet « actif », celui de l'appareil sélectionné

Marquer actifs les effets de tous les appareils dans une liste qui décrit ce que
fait *un* appareil n'est pas une simplification, c'est une information fausse.
Ce que cette session a posé est donc retenu **par appareil**, pas dans un champ
global.

### L'aperçu suit l'appareil

Le simulateur est dans le panneau de droite : liste à gauche / rendu à droite
ici, code à gauche / rendu à droite dans l'éditeur. Même grammaire, et **un seul
dessin** — `KeyboardSimulator` est le même composant des deux côtés, il n'y a pas
deux tracés à tenir d'accord. Le gabarit vient de l'appareil sélectionné :
`get_layout` s'il est ouvert, `get_default_layout` sinon.

Il suit **l'appareil**, pas la sélection : il montre les images que le moteur
produit pour lui. Sélectionner un effet sans l'appliquer ne change donc rien au
dessin, et la légende le dit. Lui faire montrer l'effet *sélectionné* exigerait
de l'exécuter dans la fenêtre — un second moteur, exactement ce que le §3 refuse.

### La pastille dit l'état, pas l'identité

« 2 appareils pilotés », « aucun appareil piloté ». Le nom est dans la colonne ;
le répéter serait un doublon, et il deviendrait faux au second appareil. Elle
reste nécessaire, et c'est pourquoi elle est un composant et non un morceau de la
barre : depuis l'éditeur, où cette colonne n'existe pas, c'est le seul endroit
qui signale une perte. Un appareil adopté mais débranché y est compté **et** dit
injoignable — fondre les deux rendrait « piloté mais absent » indicible.

### Trois écarts assumés avec la maquette

| Maquette | Ici | Pourquoi |
|---|---|---|
| Un dessin de souris | Le seul gabarit connu | La maquette l'utilise pour illustrer qu'un effet reçoit *un gabarit*, pas un clavier. Dessiner une souris qu'aucun relevé ne décrit serait inventer du matériel. |
| Un repère de couleurs pour les effets matériels | Pastille sourde | Le repère est **prélevé en exécutant l'effet**. Le micrologiciel exécute ceux-là : l'application ne voit jamais leurs images, et quatre couleurs plausibles décriraient un effet qu'on n'a pas regardé. |
| « L'aperçu tourne quand même » sans appareil | Aucun aperçu animé | L'aperçu est alimenté par la boucle du moteur, qui vise un appareil. Sans appareil piloté il n'y a pas de boucle — et en animer une sur un appareil que l'utilisateur n'a pas autorisé est exactement ce que l'adoption interdit. |

---

## 9. Les réglages sont un formulaire, pas un éditeur

C'est **le morceau qui sert le public qui n'écrira jamais de code**. Celui qui
veut « la vague, mais plus lente » n'a pas besoin d'ouvrir Monaco : il lui faut
un curseur. Tout le reste de l'application a servi celui qui écrit des effets ;
cette colonne sert l'autre.

Les contrôles sont **engendrés depuis le manifeste**, jamais écrits pour un effet
en particulier. Le formulaire connaît les quatre sortes de `ParamSpec`, et rien
d'autre : un effet installé demain obtient ses réglages sans que rien ne change
ici.

| Sorte | Contrôle | Ce qui l'accompagne |
|---|---|---|
| `number` | curseur `min`/`max`/`step` | la valeur, avec les décimales que le pas demande — point et non virgule, comme dans le manifeste |
| `color` | sélecteur de couleur | le code `#rrggbb`, en toutes lettres |
| `boolean` | case à cocher | « activé » / « désactivé » |
| `choice` | liste | l'option retenue |

Un sélecteur de couleur *est* une couleur — c'est le seul endroit de
l'application où elle est le sujet et non un rôle d'interface, et c'est
l'exception admise à la règle des jetons de style : aucune couleur n'est écrite
en dur hors de celles que le clavier **émet**. Le code hexadécimal l'accompagne
donc toujours : il se lit, se relève et se dicte, ce qu'une pastille ne permet
pas — et aucune information de ce formulaire n'est portée par la seule couleur.

### Trois destinations pour un geste

Bouger un curseur écrit à trois endroits, qui n'ont ni la même cadence ni la même
durée de vie :

| Destination | Quand | Ce que c'est |
|---|---|---|
| mémoire de la fenêtre | immédiat | ce que le formulaire affiche |
| boucle de rendu | 25 fois par seconde au plus | `set_effect_params`, à chaud |
| `settings.json` | **à la fin du geste** | `remember_effect_params` |

**Le débit vers le moteur est borné, et les états intermédiaires sont écrasés.**
Un glissement de souris produit des dizaines d'événements par seconde ; la boucle
relit les paramètres à chaque image, soit soixante fois par seconde. Envoyer plus
vite qu'elle ne lit, c'est remplacer un JSON que personne n'a encore regardé. Un
seul envoi est en vol à la fois, et le dernier état demandé repart toujours — ce
qu'on voit à l'écran est le seul qui compte, et il n'est jamais perdu.

**L'écriture disque part à la fin du geste, pas après un repos.** Un curseur
émet `input` pendant qu'on le glisse et `change` quand on le relâche : le premier
alimente la boucle, le second écrit. Un glissement de deux secondes produit donc
**une** écriture, celle de la valeur à laquelle on s'arrête — `settings.json`
s'écrit par fichier temporaire puis renommage, c'est un geste complet.

La distinction n'est pas cosmétique. Une simple temporisation — « 600 ms sans
mouvement » — perdrait le dernier réglage à chaque fois qu'on **ferme la
fenêtre** dans la foulée : fermer détruisait la vue web sans passer par les
crochets de Vue, et c'est le mode d'emploi de l'application, pas un cas limite —
un effet continue de tourner fenêtre fermée. La temporisation reste, en filet
pour les cas où `change` n'arrive pas, doublée d'un `pagehide` ; mais aucun des
deux n'est le chemin nominal, et aucun des deux ne pouvait l'être.

> **Depuis l'issue #46, la croix replie au lieu de détruire** : la vue web
> survit, ses minuteries avec elle, et `pagehide` ne se déclenche donc plus à la
> fermeture de la fenêtre — seulement à la sortie de l'application. Le filet
> perd de sa portée et la temporisation en gagne autant ; ce qui ne change pas,
> c'est que le chemin nominal reste l'écriture à la fin du geste. Le seul reste
> est une écriture en attente au moment où l'on choisit « Quitter candeo », et
> `change` l'a presque toujours déjà fait partir.
>
> Une fenêtre qui survit repliée a un second effet, et il porte plus loin :
> **son instantané vieillit**. Ce qu'elle ne lit qu'au montage — la liste des
> appareils, `settings.json` — peut dater de plusieurs jours quand elle revient,
> et le menu de l'icône a commandé les effets entre-temps. D'où l'événement
> `candeo://etat-change`, émis par l'icône et à la réouverture de la fenêtre :
> c'est la seule chose qui soit poussée plutôt qu'interrogée, et elle l'est
> parce que sonder le disque chaque seconde pour quelques changements par
> session serait le mauvais échange.

La fin d'un geste n'est d'ailleurs pas toujours rare : une flèche du clavier
maintenue enfoncée sur un curseur émet `change` **à chaque répétition**. Deux
écritures d'une même paire restent donc séparées d'au moins 250 ms ; en deçà, la
temporisation reprend la main et écrit le dernier état à la relâche. On ne perd
rien, on décale. Un geste qui ne se répète pas — le clic sur « rétablir » — lève
cet écart : il n'a aucune raison d'attendre parce qu'un curseur vient d'être
relâché.

### Pourquoi le disque, et pas la seule session

Retenir les réglages en mémoire suffirait à la lettre de l'issue — changer
d'effet puis revenir ne perd rien. Mais celui que ce formulaire sert règle « la
vague, mais plus lente » **une fois** ; le lui refaire régler à chaque lancement
reviendrait à livrer un réglage qu'on ne peut pas garder, c'est-à-dire une
démonstration. Celui qui écrit du code itère et n'a rien à retenir — c'est
l'autre public qui paie une mémoire de session, et c'est justement celui que
cette colonne vise. L'adoption d'un appareil est persistante pour la même raison :
une décision prise une fois ne se redemande pas.

La clé est la paire **appareil / effet**, et seul ce qui **diffère du manifeste**
est écrit ; les détails et le sort du numéro de série sont dans
[`../api/commands.md`](../api/commands.md) §Réglages.

### Régler un effet qu'on n'a pas appliqué

Un paramètre ne change quelque chose que dans la boucle en cours. Bouger un
curseur pour un effet qui ne tourne pas sur cet appareil ne peut donc rien
produire — et le laisser bouger sans effet serait pire que de l'interdire.

Le formulaire est **inerte, et il dit pourquoi** : « ces réglages agissent sur
l'effet en cours sur l'appareil ; *Appliquer* lance celui-ci avec les valeurs
ci-dessous ». Les valeurs restent visibles et retenues — ce sont exactement
celles avec lesquelles « Appliquer » démarrera l'effet.

L'autre réponse possible — appliquer l'effet au premier mouvement de curseur — a
été écartée : lancer une boucle sur un clavier est un geste qu'on décide, c'est
tout le sens de « Appliquer » et de l'adoption avant lui. Qu'un glissement de
souris s'en charge à la place ferait d'un réglage une prise de contrôle.

> **Un `fieldset`, et son piège.** L'état inerte se décide une fois, sur le
> groupe : `<fieldset disabled>` neutralise tous les contrôles descendants, le
> bouton de rétablissement compris, et un champ ajouté demain l'est sans que
> personne y pense — même forme que le repliement des colonnes au §8.
>
> Mais un `fieldset` porte une largeur minimale implicite (`min-width:
> min-content`) qu'aucune remise à plat ne supprime. Sans `min-width: 0`, le plus
> long libellé — écrit par l'effet, donc quelconque — impose sa largeur au groupe
> et la colonne déborde au lieu de se comprimer.

### Un effet sans paramètre le dit

Et il ne le dit pas de la même façon selon sa nature : un effet hôte « n'en
déclare aucun », un effet matériel « n'en expose aucun à l'application » — le
micrologiciel l'exécute, ses réglages ne passent pas par ici. Un cadre vide
laisserait chercher ce qui n'a pas chargé.

---

## 10. Retirer un effet, remettre la configuration au défaut

Deux gestes destructeurs arrivent ensemble (issues #41 et #42), et **la seule
chose qui compte est qu'on ne puisse pas les confondre.**

| Geste | Où | Ce qui part |
|---|---|---|
| Supprimer un effet | bibliothèque, troisième colonne, un effet à la fois | `effects/<id>/`, source comprise, et les réglages retenus pour lui |
| Remettre la configuration au défaut | écran Périphériques, tout en bas | `settings.json` : adoptions, réglages d'effets |

### Ils ne vivent pas au même endroit, et ce n'est pas une commodité de rangement

Supprimer un effet retire du **code écrit à la main**, que rien ne réinstalle.
Remettre la configuration au défaut ne touche à **aucun** effet — c'est la
distinction data / config que le stockage tient depuis le début, et c'est ici
qu'elle protège quelque chose : si l'interface laissait croire l'inverse,
quelqu'un qui voulait seulement désadopter un clavier perdrait son travail.

D'où le placement. La remise à zéro est sur l'écran où l'on **remplit**
`settings.json` — on y décide ce qui est piloté, ignoré, laissé tranquille — et
non dans la bibliothèque, où le geste voisin efface du code. La suppression est
dans la bibliothèque, sur l'effet qu'on regarde, une par effet.

### Ce qui est proposé, et à qui

**Seuls les effets écrits.** Un intégré vit dans le binaire, un effet matériel
dans le micrologiciel : il n'y a rien à retirer. Le bouton n'apparaît pas pour
eux, plutôt que d'apparaître et d'échouer — un bouton qui échoue toujours
n'apprend que sa propre inutilité.

### Les deux demandent confirmation, et la confirmation dit ce qui part

Pas une boîte modale : un encart dans la colonne, à côté de ce qu'il décrit. La
confirmation de la remise à zéro **énumère** — les appareils repassent en
`detected`, les effets s'arrêtent, le rétroéclairage s'éteint, les réglages sont
oubliés — et sa dernière ligne est la plus importante : *vos effets ne sont pas
touchés*.

La question porte sur un **identifiant**, pas sur un drapeau : l'écran n'est pas
figé pendant qu'elle est posée, et un booléen se retrouverait à confirmer la
suppression d'un autre effet que celui qu'on avait désigné. Changer de sélection
retire la question plutôt que de la laisser resurgir au retour.

### Les boucles s'arrêtent avant l'écriture, et c'est le Rust qui s'en charge

Un effet supprimé dont la boucle survivrait continuerait d'exécuter un
`effect.js` **chargé en mémoire** : aucune erreur, aucun signe, et un appareil
piloté par un effet absent de la bibliothèque. Une remise à zéro qui viderait la
table des appareils pendant qu'un effet tourne laisserait des boucles que plus
aucune décision ne désigne.

L'arrêt est donc fait côté Rust, dans la commande, pas orchestré depuis la
fenêtre : c'est le seul endroit où l'invariant tienne quel que soit l'appelant.
L'ordre exact des deux commandes est dans
[`../api/commands.md`](../api/commands.md) §Bibliothèque d'effets et §Réglages.

La fenêtre a malgré tout quelque chose à oublier : elle tient en mémoire les
réglages lus au démarrage et ce que la session a posé comme effet matériel. Sans
cet oubli, le premier mouvement de curseur réécrirait ce que le Rust vient
d'effacer.

### Éteindre plutôt que laisser la dernière image

Arrêter une boucle laisse le clavier sur sa dernière image, et une image figée
ressemble à un effet qui tourne encore. La remise à zéro pose donc `Effect::Off` :
c'est le micrologiciel qui l'exécute, le coût est nul, et l'appareil se retrouve
dans un état qui se lit. Un clavier qui refuse de s'éteindre n'interrompt rien —
l'extinction est un agrément, pas le geste.

### Ce que ce n'est pas : un endroit où libérer des ressources

`prepare()` crée un `Runtime` et un `Context` QuickJS **par boucle**, et les deux
sont détruits avec elle : tout le tas JavaScript part avec. Il n'y a rien à
libérer à la main, et aucun point d'entrée `dispose()` côté effets n'est
souhaitable — il mettrait du code utilisateur sur le chemin de l'arrêt.
