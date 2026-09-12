# Commandes Tauri — surface exposée au front

Définies dans [`apps/desktop/src-tauri/src/lib.rs`](../../apps/desktop/src-tauri/src/lib.rs).

Les types sérialisés vivent dans la couche Tauri, **pas** dans les crates :
`candeo-protocol` et `candeo-device` restent ainsi sans dépendance à serde ni à
Tauri, donc réutilisables hors application et testables en intégration continue.

L'état est une **table d'appareils ouverts**, indexée comme l'adoption les
identifie. La décision, la poignée ouverte, la boucle de rendu, l'effet en cours
et l'état d'erreur sont tous portés **par appareil** : plus rien n'est implicite,
plus rien n'est unique.

---

## Désigner un appareil

Toute commande qui agit sur un appareil en prend un, sous la forme d'un objet à
deux champs :

```ts
type DeviceRef = { vid: number, pid: number }
```

**VID et PID, rien d'autre.** C'est ce qui identifie un appareil à l'adoption, et
c'est la clé des trois tables : appareils ouverts, échecs d'ouverture, boucles de
rendu. Le numéro de série départage deux exemplaires du même modèle dans
`settings.json`, mais il ne peut pas servir de clé ici — une énumération muette
(hidraw sans règle udev) n'en déclare aucun, et l'appareil deviendrait
indésignable.

Un objet plutôt que deux entiers côte à côte : la même forme part en argument et
revient dans l'état que rend le moteur, et intervertir deux `number` ne se verrait
qu'à l'exécution.

> Les commandes d'adoption — `adopt_device`, `ignore_device`, `connect` — gardent
> `vid` et `pid` séparés : elles désignent un **gabarit du catalogue**, pas un
> appareil ouvert, et l'une d'elles est justement ce qui le fait exister.

### L'ordre de prise des verrous

Un interblocage a déjà été attrapé sur cette base : `list_devices` prenait le
verrou du clavier puis celui des échecs, `ignore_device` l'inverse. Avec une table
d'appareils et N boucles de rendu, la règle est explicite :

> **Aucun code ne tient deux verrous en même temps.** Une table est verrouillée le
> temps d'y lire ou d'y poser un pointeur partagé — jamais le temps d'une écriture
> HID, d'un démarrage de boucle ni d'une attente de fin.

Là où deux deviendraient inévitables, l'ordre est celui de la déclaration dans
`AppState` : table des appareils → moteur → table des échecs → poignée d'un
appareil → état partagé d'une boucle. Le fil de rendu, lui, ne connaît que les
deux derniers : il n'a aucun moyen de prendre un verrou de l'application, donc
aucun moyen d'en bloquer une commande. La seule attente tenue verrou en main est
celle de `stop` — et ce verrou est **propre à l'appareil**, ce qui est exactement
ce qui empêche l'arrêt de l'un de retenir les commandes visant les autres.

---

## Découverte et adoption

### `list_devices() -> DeviceInfo[]`

Liste **tous les gabarits connus**, branchés ou non, et dans quel état.

```ts
{
  name: string,
  vid: number, pid: number,
  present: boolean,
  state: 'detected' | 'adopted' | 'ignored',
  open: boolean,
  error: string | null
}
```

`present` est vrai si le VID, le PID **et** le numéro d'interface correspondent à
un périphérique énuméré. L'interface doit afficher un gabarit absent comme absent,
pas l'omettre — c'est ce qui permet de dire « branchez votre clavier » plutôt que
de montrer une liste vide.

`present` dit ce que voit le système, `state` ce que l'utilisateur a décidé :
**les deux sont indépendants**. Un appareil piloté peut être débranché, un
appareil branché peut être ignoré. Les fondre en un seul champ rendrait
« piloté mais débranché » indicible.

`error` porte le dernier échec d'ouverture **de cet appareil**. Une table indexée
par VID/PID, pas un message global : c'est ce qui fait qu'un appareil en échec
n'en entraîne aucun autre. Un champ unique obligerait à choisir lequel afficher,
et le suivant effacerait le précédent.

### Les trois états, décidés une fois et retenus

| `state` | Au lancement |
|---|---|
| `adopted` | ouvert automatiquement, sans rien demander |
| `detected` | listé, mais **pas** ouvert |
| `ignored` | laissé tranquille, et il le reste |

**Le défaut est `detected`.** Un appareil jamais vu est listé, pas piloté :
écrire sur un périphérique USB qu'on comprend mal n'est pas anodin, et à
l'échelle d'un catalogue qui grandit — claviers, souris, mémoire, ventilateurs —
adopter par défaut est la façon de casser le matériel de quelqu'un. Il y a aussi
les appareils qu'on ne *veut* pas voir pilotés : un pilote constructeur déjà en
place, ou un relevé incertain.

### `adopt_device(vid, pid) -> LayoutInfo | null`

Retient `adopted` pour cet appareil, puis l'ouvre s'il est branché.

La décision est écrite **avant** l'ouverture, et elle tient même si celle-ci
échoue : c'est une décision, pas le compte rendu d'une tentative. Le prochain
démarrage la rejouera — ce qui est précisément ce qu'on veut d'un clavier qu'un
concentrateur n'a pas fini d'énumérer.

Rend le gabarit quand l'appareil a été ouvert, `null` quand il est adopté mais
débranché : ce n'est pas une erreur, il sera ouvert au branchement suivant. Une
ouverture qui échoue, elle, remonte son message — et le laisse dans `error`.

Les autres appareils ouverts le restent : adopter celui-ci n'est pas un choix à
leur place.

### `ignore_device(vid, pid)`

Retient `ignored`, et **referme** l'appareil s'il était ouvert : on ne garde pas
ouvert ce qu'on s'engage à ne plus toucher. La poignée est vidée, pas retirée —
la boucle qui l'alimentait en tient une copie, elle s'en aperçoit à l'image
suivante et cesse d'écrire, sans que les autres appareils soient touchés.

Ne passe pas par HID, volontairement — ignorer un appareil doit rester possible
quand c'est justement l'accès HID qui pose problème.

> Il n'y a pas de commande pour revenir à `detected`. Les deux décisions qui
> comptent sont « pilote-le » et « laisse-le tranquille » ; un troisième bouton
> pour revenir à l'indécision ne répond à aucune question qu'on se pose devant
> l'écran.

### `connect(vid, pid) -> LayoutInfo`

Ouverture **ponctuelle**, sans rien décider : ne touche pas à `settings.json`,
donc ne survit pas au redémarrage. C'est ce qu'on veut pour essayer un appareil
sans s'engager ; `adopt_device` est ce qu'on veut pour ne plus avoir à le faire.

Échoue si aucun gabarit connu ne correspond, ou si l'ouverture HID échoue.

### `disconnect(device)` · `is_connected(device) -> boolean`

Libération explicite d'**un** appareil, et interrogation de son état. Les autres
ne sont pas touchés.

### Au démarrage

L'application ouvre elle-même **tous** les appareils `adopted` et présents, avant
d'afficher la fenêtre. Chaque tentative est isolée : une ouverture qui échoue
n'interrompt pas la boucle, laisse son message sur son appareil, et les suivants
s'ouvrent normalement.

Des réglages illisibles ou un HID indisponible n'empêchent pas le démarrage — ce
serait retirer le seul moyen de corriger la situation. Rien n'est ouvert, la
raison part sur la sortie d'erreur, la fenêtre s'affiche.

---

## Gabarit

### `get_layout(device) -> LayoutInfo`

Échoue si cet appareil n'est pas ouvert.

```ts
{
  name: string,
  rows: number,          // 6
  cols: number,          // 22
  frameLen: number,      // 132 — taille d'une image
  keys: {
    index: number,       // rang dans une image
    row: number, col: number,   // position dans la matrice logique
    name: string,        // « Échap », « Maj gauche », « Pavé + »…
    x: number, y: number, w: number, h: number   // rectangle physique
  }[]                    // 106 entrées
}
```

> Les DTO portent `#[serde(rename_all = "camelCase")]` : le champ Rust `frame_len`
> arrive donc en `frameLen`. Le nommage Rust ne doit pas filtrer jusque dans
> l'interface — c'est une fuite d'abstraction qui ne se verrait qu'à l'exécution.

> **`frameLen` et `keys.length` diffèrent, et c'est voulu.**
> `frameLen` vaut **132** — toutes les cases de la matrice, trous compris : c'est
> ce qu'une image doit couvrir. `keys` n'en contient que **106**, celles portant
> une LED physique : c'est ce que le simulateur dessine et ce qu'un effet itère.
>
> Confondre les deux est le piège de ce matériel. Voir
> [`../protocol/deathstalker-v2-pro.md`](../protocol/deathstalker-v2-pro.md) §6.

### La géométrie n'est pas une lecture du périphérique

`x` / `y` / `w` / `h` sont en **unités de pas de clavier** — 1 u = la largeur
d'une touche alphabétique — origine en haut à gauche, `y` vers le bas. Le dessin
complet fait 22,5 u × 6,5 u.

Ces rectangles **ne viennent pas du matériel** : celui-ci n'expose que la grille
6 × 22 et ne déclare aucune dimension. Ils sont une transcription à la main de la
disposition ISO pleine taille, écrite dans
[`crates/candeo-device/src/layout.rs`](../../crates/candeo-device/src/layout.rs).
Une erreur de dessin ne casse aucun test de cohérence : elle ne se voit qu'à
l'œil, sur le simulateur.

Deux singularités du matériel affleurent ici :

- **L'Entrée ISO porte deux LED** (index 57 et 79) et apparaît donc dans `keys`
  **deux fois**, sous le même `name`. Les deux rectangles sont les deux bras
  jointifs du L — ils ne se recouvrent pas, et un rendu qui les peint séparément
  reproduit le dégradé vertical visible sur l'appareil.
- **La barre d'espace n'en porte qu'une** (index 116), pour 6,25 u de large.

Un rendu qui suppose « une touche = une LED » se trompe donc dans les deux sens.

---

## Éclairage

Les quatre commandes écrivent sur **un** appareil, qu'elles prennent en premier
argument, et échouent s'il n'est pas ouvert.

### `set_brightness(device, level: number)`

`level` de 0 à 255.

### `set_effect(device, effect: EffectDto)`

Étiquetage serde sur le champ `kind` :

```ts
{ kind: 'off' }
{ kind: 'spectrumCycle' }
{ kind: 'wave', direction: number, speed: number }
{ kind: 'custom' }
```

Les trois premiers sont exécutés **par le micrologiciel** : coût processeur nul,
et ils survivent à la fermeture de l'application. `custom` bascule le clavier en
mode piloté par l'hôte, ce qui suppose une poussée d'images continue.

### `present(device, frame: number[])`

Image complète : suite plate de triplets RGB, **`frameLen × 3` octets exactement**
(396 pour le DeathStalker). Une taille différente est refusée avec un message
explicite plutôt que d'écrire partiellement.

En interne : six transferts `0x0f`/`0x03`, un par rangée, puis une bascule en
effet `custom`.

### `write_row(device, row, col_start, colors: number[])`

Écrit un segment de rangée sans toucher au reste — l'écriture partielle est prise
en charge par l'appareil, vérifié sur le matériel. Utile aux effets localisés,
qui évitent ainsi de réémettre les 132 positions.

---

## Bibliothèque d'effets

Les emplacements et le format sont figés dans
[`../design/effects-runtime.md`](../design/effects-runtime.md) §2 et §3. Aucun
chemin n'est écrit en dur : `app_data_dir()` porte le contenu, `app_config_dir()`
la configuration — identiques sous Windows, distincts sous Linux.

```
app_data_dir()/effects/<id>/     source.ts · effect.js · manifest.json · swatch.json
app_config_dir()/settings.json
```

### `install_effect(source_ts, js, manifest) -> string`

Écrit les trois fichiers, prélève le repère de couleurs dans un quatrième, et
renvoie l'`id` retenu.

```ts
manifest: {
  name: string,
  description?: string,
  params?: Record<string, ParamSpec>,   // tel que déclaré côté TypeScript
  apiVersion: number                   // version de l'API d'effets à l'écriture
}
```

Les paramètres sont stockés **tels quels** : leur forme est celle de `ParamSpec`
dans `@candeo/effects-api`, elle évolue avec l'éditeur, et le Rust ne les
interprète pas. Les retyper en Rust créerait une seconde source de vérité.

`apiVersion` est obligatoire. Un effet écrit pour une version que cette
application ne connaît pas est refusé à l'installation, avec un message qui le
dit — plutôt que d'échouer plus tard à la première image.

Le front envoie le JavaScript déjà transpilé par Monaco, **et** la source
TypeScript : sans elle l'effet ne serait plus modifiable, sans le `.js` il ne
pourrait plus démarrer sans ouvrir la fenêtre.

**L'`id` est dérivé du nom, jamais repris tel quel.** Seuls `a-z`, `0-9` et le
tiret subsistent ; tout le reste devient un tiret. C'est une liste blanche, donc
`..`, les séparateurs de chemin et les noms réservés de Windows (`CON`, `NUL`,
`COM1`…) ne peuvent pas en sortir. Deux effets de même nom obtiennent le même
`id` : réenregistrer depuis l'éditeur **met à jour** au lieu de dupliquer.

### `list_effects() -> EffectEntry[]`

```ts
{
  id: string,
  kind: 'builtin' | 'user',
  swatch: string[],               // couleurs « #rrggbb », prélevées sur le rendu
  name: string,
  description: string,
  params: Record<string, ParamSpec>,
  apiVersion: number
}
```

Effets intégrés **et** installés, dans une seule liste : les intégrés sont
compilés dans le binaire et n'ont pas de dossier, `kind` les distingue. Ils
viennent en tête, les installés ensuite, triés par `id`.

Le repère voyage avec l'entrée, et non derrière un second appel : sinon une
bibliothèque de vingt effets demanderait vingt allers-retours pour afficher
vingt vignettes.

Un dossier dont le manifeste est illisible est ignoré, pas propagé en erreur :
une bibliothèque de vingt effets ne doit pas disparaître à cause d'un seul.
L'ordre est stable — le système de fichiers n'en garantit aucun.

### Le repère de couleurs

Chaque entrée porte quelques couleurs qui aident à retrouver un effet sans le
lancer. **Elles sont obtenues en exécutant l'effet**, jamais déclarées dans le
manifeste ni dessinées à la main.

Deux raisons, et la seconde pèse plus que la première. L'auteur n'a rien à
fournir : on écrit son effet, il a son repère — aucun champ, aucun mode avancé.
Et surtout, **le repère ne peut pas mentir**. Déclaré, il dériverait dès la
première modification du code, et un effet devenu bleu garderait sa vignette
rouge.

#### Comment il est prélevé

Quatre images sont rendues par le moteur, sans toucher au matériel, à quatre
instants : 0 s, 0,37 s, 1,13 s et 2,61 s. L'effet tourne avec les **valeurs par
défaut que son module déclare** — celles avec lesquelles la galerie le lancerait,
et non un objet vide, qui donnerait du noir à tout effet ne se repliant sur rien.
Le gabarit est celui **par défaut**, jamais celui du clavier branché : un repère
qui dépendrait du matériel présent à l'installation ne serait comparable ni d'un
effet à l'autre, ni d'une machine à l'autre.

De chaque image on tire **une** couleur : la moyenne d'une bande diagonale du
clavier, la bande avançant d'une image à la suivante. Trois choix, trois raisons :

| Choix | Pourquoi pas autrement |
|---|---|
| des instants **irrégulièrement espacés** | régulièrement espacés, ils se caleraient sur la période d'un effet cyclique et rendraient quatre fois la même couleur |
| une bande **diagonale** | un dégradé horizontal ne varie que selon la colonne, un balayage vertical que selon la rangée : découper selon l'une des deux rendrait l'autre parfaitement uniforme |
| une **bande**, et non une touche | un effet peut laisser presque tout le clavier éteint — `balayage` est exactement cela — et une touche isolée tomberait sur du noir par hasard |

Le résultat : un effet uniforme rend quatre fois sa couleur, un dégradé rend
quatre couleurs échelonnées, un effet majoritairement sombre rend un repère
sombre. Un effet spatial et un effet uniforme ne peuvent pas se ressembler.

Les quatre effets livrés, tels que le moteur les rend :

| `id` | Repère |
|---|---|
| `onde-radiale` | `#58f14a` `#38dc5b` `#3e71df` `#b4a209` |
| `respiration` | `#803000` `#b94600` `#fe5f00` `#6e2900` |
| `balayage` | `#0072a2` `#005072` `#004f6f` `#002332` |
| `degrade-fixe` | `#c51c9c` `#a02faf` `#833dbd` `#475bdb` |

#### Quand il est calculé, et où il est rangé

**Une fois à l'installation**, dans `effects/<id>/swatch.json`, à côté du
manifeste — jamais à l'affichage de la liste, qui reste une lecture de disque :
échantillonner là ferait dépendre l'ouverture de la galerie du comportement de
tous les effets installés, pour des vignettes qui ne bougent pas. Réenregistrer
un effet repasse par `install_effect`, donc le recalcule.

Les effets **intégrés** n'ont pas de dossier : leur repère vit **en mémoire**,
calculé à la première lecture de la bibliothèque et retenu pour la durée du
processus. Il est une propriété du binaire et non de la bibliothèque de
l'utilisateur : l'écrire dans le dossier de données créerait un cache à invalider
à chaque mise à jour de l'application — une version à comparer, un fichier à
réécrire, et une occasion de montrer le repère de la version précédente — pour
quatre effets dont l'échantillonnage coûte quelques millisecondes. L'écrire à la
main dans le Rust est exclu par le principe même du repère.

Un effet installé par une version antérieure n'a donc pas de repère tant qu'il
n'est pas réenregistré. C'est le prix de cette règle, et il se paie en pastille
neutre, pas en erreur.

#### Ce qui peut mal se passer

C'est du code utilisateur : il peut lever, ne pas charger, ou boucler sans fin.
Un repère qu'on n'arrive pas à calculer **n'empêche jamais l'installation** —
`swatch` est alors un tableau vide et l'interface montre une pastille neutre.
L'échantillonnage est borné dans le temps, sans quoi un `while (true)`
empêcherait un effet de s'installer pour toujours. Et un repère précédent est
**effacé** plutôt que conservé : montrer les couleurs d'une version qui n'existe
plus serait pire que n'en montrer aucune.

Un effet qui rend du noir partout, lui, n'est pas un échec : son repère est noir,
et c'est la vérité sur ce qu'il fait.

#### Ce que le format ne fige pas

`swatch` est une **liste**, pas un quadruplet, et rien dans le stockage n'en fixe
la longueur. Le jour où la galerie voudra des vignettes animées, il suffira de ne
pas s'arrêter à quatre images : ni le fichier ni le type exposé n'ont à changer.

### Les effets intégrés

Quatre sont livrés, écrits en **JavaScript contre la même API** que les effets
de l'utilisateur et chargés par le même moteur. Un effet intégré écrit en Rust
natif serait plus rapide et ne prouverait rien : le premier exemple qu'on ouvre
doit être exactement ce qu'on peut écrire soi-même. Ils vivent dans
[`apps/desktop/src-tauri/src/builtins/`](../../apps/desktop/src-tauri/src/builtins/).

| `id` | Nom | Ce qui le distingue |
|---|---|---|
| `onde-radiale` | Onde radiale | teinte en mouvement, propagée depuis le centre |
| `respiration` | Respiration | une seule couleur, aucune variation dans l'espace |
| `balayage` | Balayage | une rangée éclairée, le reste éteint |
| `degrade-fixe` | Dégradé fixe | deux couleurs, immobile — son `render` ignore `time` |

**Un identifiant intégré est réservé.** `install_effect` refuse un nom qui
dérive vers l'un d'eux, en le disant. Et si un dossier portant un tel `id`
apparaît malgré tout — copie manuelle, bibliothèque héritée —, c'est l'intégré
qui est lu et affiché : une entrée marquée `builtin` exécute le code livré, et
rien d'autre. Le dossier usurpateur n'est pas listé (la liste est indexée par
`id`, elle ne peut pas en montrer deux) mais reste supprimable.

### `delete_effect(id)`

Supprime le dossier. Un `id` hors de la liste blanche est refusé avant tout
accès au disque. Un effet intégré n'a pas de dossier et ne se supprime pas ; le
disque est consulté d'abord, ce qui laisse retirer un dossier qui usurperait un
identifiant intégré.

Emporte aussi les **réglages retenus** pour cet effet, sur tous les appareils
(voir §Réglages). L'oubli vient après la suppression : si celle-ci échoue,
l'effet est toujours là et ses réglages doivent l'être aussi.

**Trois temps, et l'ordre fait partie du contrat :**

1. **le refus**, avant tout — un effet intégré ou un identifiant qui ne désigne
   rien s'entend dire non sans que rien n'ait été arrêté ni effacé ;
2. **l'arrêt des boucles** qui font tourner cet effet, sur **tous** les appareils,
   et avant l'effacement. Le moteur charge `effect.js` une fois au démarrage et le
   garde en mémoire : une boucle laissée en vie continuerait sans la moindre
   erreur visible, sur un dossier qui n'existe plus, et l'appareil resterait
   piloté par un effet absent de la bibliothèque ;
3. **l'effacement**, puis l'oubli des réglages.

L'arrêt est fait **côté Rust**, pas dans la fenêtre : c'est le seul endroit qui le
garantisse quel que soit l'appelant. La ligne d'`engine_status()` de l'appareil
subsiste, mais elle cesse de nommer l'effet — l'identifiant ne désigne plus rien.

L'interface ne propose pas le geste sur un effet intégré, plutôt que de le
proposer et de le laisser échouer.

### `read_effect_source(id) -> string`

La source, pour la rouvrir dans l'éditeur. Un effet intégré rend son JavaScript,
qui **est** sa source : il n'y a pas de `.ts` à transpiler. C'est l'usage prévu —
on part d'un effet qui marche, on le modifie, on l'enregistre sous un autre nom.

---

## Réglages

### `get_settings() -> Settings` · `set_settings(settings)`

```ts
{
  activeEffect: string | null,   // id à reprendre au démarrage
  brightness: number,             // 0-255
  device: { vid: number, pid: number } | null,
  devices: {
    vid: number,
    pid: number,
    serial?: string,             // absent quand le système n'en déclare pas
    state: 'detected' | 'adopted' | 'ignored'
  }[],
  effectParams: {
    vid: number,
    pid: number,
    effect: string,              // identifiant de l'effet réglé
    values: Record<string, ParamValue>
  }[]
}
```

Au premier lancement il n'y a pas de fichier : `get_settings` renvoie les
**défauts**, ce n'est pas une erreur. Un champ absent d'un fichier écrit par une
version antérieure reprend lui aussi son défaut, plutôt que de rendre
l'application muette au démarrage.

### `reset_settings()`

Réécrit `settings.json` avec les **valeurs par défaut**, et repose les appareils.
C'est la seule façon de revenir à un état connu sans aller éditer le fichier à la
main — la première chose qu'on cherche quand quelque chose se comporte mal, et ce
qui rend un rapport de bogue exploitable.

Ce qui part : les décisions d'adoption — tout repasse en `detected` — et les
réglages retenus par paire appareil / effet.

**Aucun effet n'est touché.** Les effets écrits vivent dans
`app_data_dir()/effects/`, pas dans `settings.json` ; les retirer est une autre
action, `delete_effect`, une par effet. C'est la distinction que tout le stockage
tient — un effet est du contenu, le choix de l'effet actif est de la
configuration — et la confondre ferait perdre du code écrit à la main à qui
voulait seulement désadopter un clavier. La commande n'en a d'ailleurs pas les
moyens : elle n'écrit que dans le fichier de configuration.

Les appareils sont reposés **avant** l'écriture, dans cet ordre :

1. **les boucles s'arrêtent**, et l'arrêt est attendu — remettre la table des
   appareils à zéro pendant qu'un effet tourne laisserait des boucles que plus
   aucune décision ne désigne, et l'image suivante rallumerait ce qu'on est sur le
   point d'éteindre ;
2. **le rétroéclairage s'éteint** (`Effect::Off`). Arrêter une boucle laisse le
   clavier sur sa dernière image, et une image figée ressemble à un effet qui
   tourne encore ; l'extinction est exécutée par le micrologiciel, elle ne coûte
   rien ;
3. **les poignées sont refermées** et les échecs d'ouverture oubliés : on ne garde
   pas ouvert un appareil que plus aucune décision ne désigne, et un message
   d'échec décrivant une adoption qui n'existe plus n'apprend rien.

Un clavier qui refuse de s'éteindre — débranché entre-temps, accès perdu —
n'interrompt pas la remise à zéro : l'extinction est un agrément, pas le geste.

Ce n'est **pas** un endroit où libérer des ressources côté effets. Chaque boucle
porte son `Runtime` et son `Context` QuickJS, tous deux détruits avec elle : tout
le tas JavaScript part avec. Aucun point d'entrée `dispose()` n'est souhaitable,
il mettrait du code utilisateur sur le chemin de l'arrêt.

La fenêtre, elle, garde ce qu'elle avait lu : c'est à elle d'oublier les réglages
tenus en mémoire après l'appel, sans quoi le premier mouvement de curseur les
réécrirait.

### `remember_effect_params(device, effect, params)`

Retient les réglages d'un effet **pour un appareil**, et rien d'autre du
fichier. À ne pas confondre avec `set_effect_params`, plus bas, qui ajuste la
boucle en cours : celle-ci écrit sur disque et ne change rien à ce qui tourne. Les deux n'ont ni la même cadence — des dizaines d'appels par seconde
d'un côté, un seul quand le curseur s'arrête de l'autre — ni la même destination.

Une commande dédiée plutôt qu'un `set_settings` depuis la fenêtre : la lecture,
la modification et l'écriture se font côté Rust, d'un seul tenant.

Ce n'est pas une précaution contre un entrelacement — les commandes synchrones
s'exécutent sur le fil principal, elles ne se chevauchent pas. C'est une
précaution contre une **copie périmée** : la fenêtre lit les réglages une fois,
au montage de l'écran, et un `set_settings` posté au premier mouvement de curseur
renverrait cet instantané tel quel, effaçant ce qui aurait été décidé depuis. Ce
n'est pas un cas d'école — `adopt_device` écrit `settings.json`, et adopter un
appareil est justement ce qu'on fait entre deux réglages.

Une table `params` **vide** efface l'entrée : c'est « rétablir les valeurs
déclarées ». L'effet repart alors de son manifeste, y compris si une version
ultérieure en change les défauts.

Elle est appelée **à la fin du geste** — curseur relâché, case cochée — et non
après une temporisation : fermer la fenêtre détruit la vue web sans exécuter ses
crochets de sortie, or fermer la fenêtre pendant qu'un effet tourne est le mode
d'emploi de l'application. Une minuterie ne serait donc jamais qu'un filet.

`delete_effect` emporte les réglages retenus pour l'effet supprimé, sur tous les
appareils. Sans quoi le fichier garderait des entrées désignant un identifiant
que plus rien ne nomme, et un effet réinstallé sous le même nom hériterait en
silence des réglages de son homonyme disparu.

### La clé d'un réglage : l'appareil et l'effet, sans le numéro de série

Le même effet n'a aucune raison de tourner à la même vitesse sur deux claviers,
et deux effets du même clavier n'ont pas les mêmes paramètres : la clé est donc
la paire. Changer d'effet puis revenir retrouve ses réglages, et l'écran les
relit au démarrage suivant.

**Sans la série, contrairement à `devices`.** Toutes les commandes du moteur
visent un `DeviceRef`, c'est-à-dire un VID et un PID ; deux exemplaires du même
modèle partagent déjà leur boucle de rendu. Les distinguer ici promettrait une
séparation que le reste de l'application ne tient pas, et le réglage semblerait
perdu une fois sur deux. L'adoption, elle, décide d'ouvrir un exemplaire précis :
elle a besoin de la série, et c'est pourquoi elle la porte.

Comme `devices`, `effectParams` ne contient que ce qui **diffère du défaut** :
un paramètre laissé à sa valeur déclarée n'y figure pas, et suivra le manifeste
si l'effet est réenregistré avec d'autres défauts. Une entrée n'apparaît donc que
si quelqu'un a déplacé un curseur.

### L'identité d'un appareil : VID / PID / numéro de série

**Ni la variante, ni le micrologiciel.** Le même clavier s'est déclaré
`v1.4 / Unkown Variant` puis `v1.5 / Quartz` pendant le relevé du protocole : une
liaison qui apparie sur ces champs se rompt à la mise à jour, et l'appareil
adopté redevient un inconnu du jour au lendemain.

La série n'est comparée que si **les deux côtés** en portent une, et cet
arbitrage tient dans les deux sens :

- elle départage deux exemplaires du même modèle — sans elle, adopter l'un
  adopterait l'autre ;
- mais une énumération muette — hidraw sans règle udev, un concentrateur qui ne
  relaie rien — ne doit pas désapparier un appareil déjà adopté, sans quoi la
  décision serait à reprendre à chaque branchement.

Une entrée apprise sans série se complète dès qu'on la connaît ; elle ne s'efface
jamais.

`devices` ne contient que les décisions qui **diffèrent du défaut** : un appareil
absent de la liste est `detected`, ce qui est exactement l'état d'un appareil
jamais rencontré. Le fichier ne grossit donc pas d'une entrée à chaque
périphérique branché une fois.

L'écriture passe par un fichier temporaire suivi d'un renommage : une coupure en
cours d'écriture laisserait sinon des réglages tronqués.

---

## Erreurs

Toutes les commandes faillibles renvoient `Result<T, String>`. Le message est
destiné à être **affiché tel quel** : il doit rester lisible par un humain, pas
devenir un code à traduire côté front.

---


## Moteur d'effets

**Un fil de rendu Rust par appareil**, indépendants de la fenêtre : fermer
l'application n'éteint pas les effets. C'est le seul endroit où du code d'effet
s'exécute — le front n'en exécute jamais, ce qui lui retire au passage tout accès
au DOM et à l'API Tauri. Conception dans
[`../design/effects-runtime.md`](../design/effects-runtime.md) §4 et §5.

**Un appareil, un effet.** Chacun porte sa boucle, donc sa cadence, ses
paramètres, son état d'erreur et sa sortie. Rien n'est partagé entre deux
appareils, et c'est ce qui fait qu'un appareil en panne n'en affecte aucun autre.

### `start_effect(device, id, params)`

Charge le JavaScript de l'effet — celui d'un intégré, sinon
`effects/<id>/effect.js` — et démarre la boucle **de cet appareil**. Le moteur ne
fait aucune différence entre les deux : un effet livré est un module chargé
exactement comme celui qu'on vient d'écrire.

Remplace l'effet en cours **sur cet appareil**, s'il y en avait un — l'arrêt
précédent est **attendu**, sans quoi deux boucles écriraient un instant sur le
même appareil. Les autres appareils ne sont pas touchés, et l'attente ne les
retient pas : le verrou attendu est propre à l'appareil visé.

Une erreur de syntaxe ou un module mal formé est signalé **à l'appel**, pas
découvert plus tard dans un état : l'appel attend le verdict du chargement.

Le gabarit vient de **l'appareil visé**, ouvert ou non. Délibéré, et à double
titre : le gabarit d'un appareil ne dépend pas de sa présence, et on doit pouvoir
écrire et prévisualiser un effet **sans posséder le clavier**. Viser un appareil
débranché lance donc l'effet, alimente le simulateur, et laisse
`reachingKeyboard` à faux jusqu'à l'ouverture.

#### Ce qu'un module d'effet doit exposer

```ts
import { hsv } from '@candeo/effects-api'

export default {
  name: 'Mon effet',
  render({ layout, time, frameIndex, frame, params }) { … },
}
```

**Un export par défaut, et rien d'autre.** L'import de `@candeo/effects-api` est
résolu vers un module interne fourni par l'hôte : pas de bundler, pas de
`node_modules`, pas de résolution de chemins.

Chaque image repart du noir. Un effet qui n'écrit qu'une partie du clavier
n'hérite donc pas en silence de l'image précédente — une image est complète par
définition.

### `stop_effect(device)` · `set_effect_params(device, params)`

`set_effect_params` ajuste **à chaud** : la boucle relit les paramètres à chaque
image, elle ne redémarre pas. Les deux ne touchent qu'à l'appareil visé ; un
appareil sur lequel rien n'a jamais été lancé les ignore silencieusement.

`params` **remplace** la table entière, il ne la fusionne pas : l'appelant envoie
l'état complet, pas le seul champ qu'il vient de changer.

L'appel est bon marché mais pas gratuit, et un curseur en produit des dizaines
par seconde. La fenêtre les ramène donc à **25 par seconde au plus, un seul en
vol à la fois**, en écrasant les états intermédiaires : la boucle ne lit que le
dernier, et une file d'attente ne ferait que le lui livrer en retard. Le dernier
état demandé part toujours — c'est la seule garantie qui compte, puisque c'est
celui qu'on voit.

Retenir ces valeurs d'un lancement à l'autre est l'affaire de
`remember_effect_params` (§Réglages), qui n'écrit que sur disque.

### `set_output_to_keyboard(device, on)`

Coupe ou rétablit l'écriture vers **cet** appareil, **sans toucher au
simulateur** ni aux autres appareils. Les deux sorties d'une boucle sont
indépendantes, et chacune peut être absente :

- fenêtre fermée → seule l'écriture HID subsiste, aucune image n'est sérialisée ;
- sortie clavier coupée → seul le simulateur est alimenté ;
- les deux actives → l'aperçu montre exactement les octets envoyés.

### `subscribe_frames(device, channel)` · `unsubscribe_frames(device)`

Une commande répond **une fois** ; un effet produit 60 images par seconde. La
remontée passe donc par `tauri::ipc::Channel`, créé côté front et passé en
argument. Les images y circulent en binaire (`InvokeResponseBody::Raw`) :
396 octets, contre plus de 1,5 Ko sérialisées en tableau JSON d'entiers.

Un canal **par appareil** : le simulateur suit celui qu'on a sélectionné, et
changer de sélection ferme un canal pour en ouvrir un autre. Un abonnement laissé
ouvert sur l'appareil précédent alimenterait le même simulateur en parallèle.

Se désabonner arrête le flux **sans arrêter l'effet**, qui continue d'alimenter
le clavier.

### `engine_status() -> DeviceEngineStatus[]`

```ts
{
  device: { vid: number, pid: number },
  running: boolean,
  effectId: string | null,
  error: string | null,
  deviceError: string | null,
  reachingKeyboard: boolean,
  toKeyboard: boolean
}[]
```

**Une entrée par appareil**, `reachingKeyboard` compris. Un état global
obligerait à choisir lequel afficher, et le suivant effacerait le précédent —
exactement ce que la table des échecs d'ouverture évite déjà côté adoption.

La liste couvre les appareils sur lesquels un effet a été lancé depuis le
démarrage, pas seulement ceux qui en portent un en ce moment : un appareil arrêté
garde sa ligne, `running` à faux. « Cet appareil ne fait rien » et « je ne sais
rien de cet appareil » ne se disent pas pareil, et l'interface doit pouvoir les
distinguer. L'ordre est stable, trié par VID puis PID.

Interrogé plutôt que poussé : une erreur survenue fenêtre fermée doit se lire à
la réouverture, ce qu'un événement ponctuel ne permet pas.

Une exception dans un effet **ne fait pas tomber l'application** : elle est
rattrapée par image, exposée ici, et effacée dès que l'effet se rétablit. Après
trente images consécutives en échec, la boucle s'arrête — un effet qui lève à
chaque image ne se rétablira pas tout seul. Et elle n'arrête que **sa** boucle :
les autres appareils continuent.
