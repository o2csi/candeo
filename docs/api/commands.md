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

> Il n'y a pas de commande pour ramener **un** appareil à `detected`. Les deux
> décisions qui comptent sont « pilote-le » et « laisse-le tranquille » ; un
> troisième bouton pour revenir à l'indécision ne répond à aucune question qu'on
> se pose devant l'écran. `reset_settings` (§Réglages) les ramène **tous** à la
> fois, et c'est une autre question : celle de repartir d'un état connu.

### `connect(vid, pid) -> LayoutInfo`

Ouverture **ponctuelle**, sans rien décider : ne touche pas à `settings.json`,
donc ne survit pas au redémarrage. C'est ce qu'on veut pour essayer un appareil
sans s'engager ; `adopt_device` est ce qu'on veut pour ne plus avoir à le faire.

Échoue si aucun gabarit connu ne correspond, ou si l'ouverture HID échoue.

### `disconnect(device)`

Libération explicite d'**un** appareil. Les autres ne sont pas touchés.

Avec `connect`, la paire qui ouvre et referme **sans décider**, là où
`adopt_device` et `ignore_device` écrivent dans `settings.json`. Les deux sont
enveloppées par `useDevice` et aucun écran ne les appelle encore : ce qui manque
est un bouton, pas une commande.

> **`is_connected` a été retirée** (audit #65). Elle répondait ce que
> `list_devices` porte déjà dans le champ `open` de chaque appareil, et que la
> fenêtre lit par là — une seconde source de vérité pour un état que le Rust est
> seul à connaître. Interroger appareil par appareil ce qu'une seule commande
> énumère n'apportait rien, et faisait diverger les deux réponses le jour où
> l'une des deux aurait été rafraîchie sans l'autre.

### Au démarrage

L'application ouvre elle-même **tous** les appareils `adopted` et présents, avant
d'afficher la fenêtre. Chaque tentative est isolée : une ouverture qui échoue
n'interrompt pas la boucle, laisse son message sur son appareil, et les suivants
s'ouvrent normalement.

Des réglages illisibles ou un HID indisponible n'empêchent pas le démarrage — ce
serait retirer le seul moyen de corriger la situation. Rien n'est ouvert, la
raison part au journal (`tracing::error!`, donc dans le fichier du jour), la
fenêtre s'affiche.

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

`level` de 0 à 255. Écrit **sur le clavier**, et rien d'autre : c'est une commande
distincte du protocole (`0x0f`/`0x04`), sans rapport avec l'effet en cours.

Retenir ce niveau d'un lancement à l'autre est l'affaire de `remember_brightness`
(§Réglages), qui n'écrit que sur disque. Même partage que `set_effect_params` /
`remember_effect_params`, et pour la même raison : un curseur qu'on glisse produit
des dizaines d'écritures HID et une seule écriture disque, quand il s'arrête.

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

Emporte aussi tout ce que `settings.json` retenait de lui : les **réglages**, sur
tous les appareils, et son **application** (`activeEffects`). L'oubli vient après
la suppression : si celle-ci échoue, l'effet est toujours là et ses réglages
doivent l'être aussi.

La purge de `activeEffects` tranche le piège relevé par l'issue #48 : sans elle,
supprimer l'effet appliqué laisserait un **identifiant pendant**, que la reprise au
démarrage tenterait de lancer. Deux réponses étaient possibles — purger à la
suppression, ou se replier en silence au démarrage. La première est retenue :
l'invariant « le fichier ne contient jamais un identifiant que la bibliothèque ne
connaît pas » se vérifie sans rien faire tourner, là où un silence au lancement est
exactement le genre de panne qui coûte une session. Le repli reste nécessaire en
**seconde** barrière — un dossier d'effet retiré à la main ne passe pas par ici —
mais il n'est plus le seul.

**Trois temps, et l'ordre fait partie du contrat :**

1. **le refus**, avant tout — un effet intégré ou un identifiant qui ne désigne
   rien s'entend dire non sans que rien n'ait été arrêté ni effacé ;
2. **l'arrêt des boucles** qui font tourner cet effet, sur **tous** les appareils
   et dans l'aperçu, avant l'effacement. Le moteur charge `effect.js` une fois au
   démarrage et le garde en mémoire : une boucle laissée en vie continuerait sans
   la moindre erreur visible, sur un dossier qui n'existe plus, et l'appareil
   resterait piloté — ou l'écran animé — par un effet absent de la bibliothèque ;
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
  preferences: {
    logLevel?: 'error' | 'warn' | 'info' | 'debug' | 'trace'
  },
  devices: {
    vid: number,
    pid: number,
    serial?: string,             // absent quand le système n'en déclare pas
    state: 'detected' | 'adopted' | 'ignored',
    brightness?: number          // absent = pleine (255)
  }[],
  activeEffects: {
    vid: number,
    pid: number,
    effect: string               // l'effet appliqué sur cet appareil
  }[],
  effectParams: {
    vid: number,
    pid: number,
    effect: string,              // identifiant de l'effet réglé
    values: Record<string, ParamValue>
  }[]
}
```

**Une préférence globale dans `preferences`, tout ce qui dépend d'un clavier dans
une liste indexée.** C'est la règle que ce fichier tient, et elle vaut pour tout ce
qu'on y ajoutera : la langue ira dans `preferences`, sans rien avoir à arbitrer.

Chaque liste ne porte que ce qui **diffère du défaut** : un appareil absent de
`devices` est `detected` et à pleine luminosité, un appareil absent
d'`activeEffects` ne s'est vu appliquer aucun effet, et une entrée d'appareil qui
ne retient plus rien — `detected` sans luminosité — est retirée plutôt que gardée
vide.

Au premier lancement il n'y a pas de fichier : `get_settings` renvoie les
**défauts**, ce n'est pas une erreur. Un champ absent d'un fichier écrit par une
version antérieure reprend lui aussi son défaut, plutôt que de rendre
l'application muette au démarrage.

#### Ce qui a disparu en v2.1, et pourquoi

`activeEffect`, `device` et `brightness` étaient trois scalaires à la racine. Les
deux premiers n'étaient lus ni écrits par personne ; le troisième l'était. Le
problème n'était pas leur valeur, c'était leur **forme** : ils décrivaient *un*
effet actif, *un* appareil choisi et *un* niveau de luminosité, alors que le moteur
fait tourner un effet par appareil depuis l'issue #26 — et que
`set_brightness(device, level)` prenait déjà un `DeviceRef`.

- `activeEffect` → `activeEffects[]`, une entrée par appareil ;
- `device` → **retiré**. `devices` porte déjà les décisions appareil par
  appareil ; un « appareil choisi » global n'a plus de sens depuis qu'il n'y a
  plus d'appareil implicite ;
- `brightness` → `devices[].brightness`. Deux claviers n'ont aucune raison de
  partager un niveau.

Un fichier antérieur se relit sans erreur, et ces trois clés sont simplement
ignorées : les récupérer aurait demandé de choisir *quel* appareil elles
désignaient, question sans réponse. Seul `logLevel`, qui a changé de place sans
changer de sens, est **récupéré** depuis la racine et versé dans `preferences` à
la première lecture — le retomber au défaut aurait ramené au silence celui qui
était justement en train de chercher une panne.

`activeEffects` est écrit par `start_effect` et effacé par `stop_effect` ; l'aperçu
n'y touche jamais. Supprimer un effet purge son entrée partout — voir
`delete_effect`.

### `reset_settings()`

Réécrit `settings.json` avec les **valeurs par défaut**, et repose les appareils.
C'est la seule façon de revenir à un état connu sans aller éditer le fichier à la
main — la première chose qu'on cherche quand quelque chose se comporte mal, et ce
qui rend un rapport de bogue exploitable.

Ce qui part : les décisions d'adoption — tout repasse en `detected` —, la
luminosité retenue de chaque appareil, l'effet appliqué sur chacun, et les
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

### `remember_brightness(device, level)`

Retient la luminosité de **cet** appareil, sans toucher au clavier — le pendant
disque de `set_brightness`. Elle est réappliquée à l'ouverture de l'appareil, au
démarrage comme à l'adoption : un niveau retenu qui ne se réappliquerait pas au
branchement ne servirait à rien, et le protocole relevé sait écrire la luminosité
mais pas la relire.

Le maximum (255) **efface** l'entrée au lieu d'y écrire le défaut, exactement
comme une table de paramètres vide efface les réglages d'un effet. Un appareil
dont c'était la seule décision disparaît alors de `devices`.

La série est relevée si elle se donne, comme pour `ignore_device` : elle fait
atterrir le niveau sur le bon exemplaire quand il y en a deux du même modèle, et
son absence ne bloque rien.

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

Le nom de ce temporaire est **fixe**, et il peut l'être parce que les commandes
synchrones s'exécutent sur le fil principal : deux séquences lire-modifier-écrire
ne s'entrelacent pas. Le raisonnement vaut *à l'intérieur* d'un processus — entre
deux, rien ne les sérialiserait, et l'un renommerait ce que l'autre est en train
d'écrire. C'est donc l'**instance unique**
([`single_instance.rs`](../../apps/desktop/src-tauri/src/single_instance.rs)) qui
rend ce nom fixe sûr : les deux décisions se tiennent, et ne se défont pas l'une
sans l'autre.

---

## Erreurs

Toutes les commandes faillibles renvoient `Result<T, String>`. Le message est
destiné à être **affiché tel quel** : il doit rester lisible par un humain, pas
devenir un code à traduire côté front.

---

## Journal

`tracing` + `tracing-subscriber` + `tracing-appender`, initialisés en tête du
`setup` de l'application — **avant que le magasin ne soit résolu**, sinon un échec
de résolution du dossier de configuration arriverait avant qu'il n'y ait de quoi
l'écrire. Conception et arbitrages dans `src-tauri/src/journal.rs`.

Le fichier tourne **par jour**, sept au plus, dans `app_log_dir()` — par l'API
Tauri, jamais un chemin en dur : les journaux ne sont ni des données ni de la
configuration, et sous Linux les trois dossiers diffèrent.

| Niveau | Ce que ça veut dire |
|---|---|
| `error` | l'éclairage de l'utilisateur est cassé |
| `warn` | dégradé mais fonctionnel — micrologiciel inattendu (#35), exclusion d'instance inopérante (#45) |
| `info` | cycle de vie : appareil adopté, effet démarré, effet arrêté |
| `debug` / `trace` | par image, **éteint par défaut** |

**Les transitions, jamais les occurrences.** À 30 images par seconde, une écriture
qui échoue produirait trente lignes par seconde et enterrerait la seule qui
compte. Le journal suit exactement le modèle du moteur — « a commencé à échouer
(raison) », « rétabli », « arrêté après 30 échecs » — et rien par image.

**Un span par boucle de rendu**, portant l'appareil et l'effet. C'est la raison
d'avoir pris `tracing` plutôt que `tauri-plugin-log` : il y a une boucle par
appareil, et « écriture refusée » ne sert à rien sans savoir laquelle.

**Le numéro de série ne figure nulle part.** Une empreinte stable (FNV-1a, 16
chiffres hexadécimaux) le remplace : elle distingue deux exemplaires du même
modèle sans divulguer lequel. Une énumération muette — hidraw sans règle udev —
se dit `aucune`, ce qui n'est pas la même chose.

### Priorité du niveau

1. **`CANDEO_LOG` l'emporte**, toujours — c'est ce qui permet de diagnostiquer
   une application qui ne va pas assez loin pour lire ses réglages. Elle accepte
   un niveau seul (`debug`) ou une directive `EnvFilter` complète
   (`candeo_desktop_lib::runtime=trace,warn`) ;
2. sinon `settings.json`, champ `logLevel` ;
3. sinon `info`.

`CANDEO_LOG` et non `RUST_LOG` : cette dernière est partagée par tout l'outillage
Rust, et quelqu'un qui l'a posée pour `cargo` changerait sans le vouloir le
journal de l'application.

### `get_journal() -> JournalStatus`

```ts
{
  level: LogLevel | null,     // null : CANDEO_LOG porte une directive qu'aucun niveau ne résume
  setting: LogLevel | null,   // ce que retient settings.json ; null = le défaut
  forcedByEnv: boolean,
  dir: string | null,         // null : le journal n'écrit pas sur disque
  verbose: boolean            // le niveau actif porte du par-image
}
```

### `set_log_level(level) -> JournalStatus`

Change le niveau **sans redémarrer** (`tracing_subscriber::reload`), et le
retient. Le défaut qu'on cherche peut ne pas survivre au redémarrage : un clavier
qui décroche après deux heures, un appareil qui disparaît par intermittence —
« relancez en mode détaillé » revient à demander de reproduire ce qu'on vient
d'observer.

**Il survit au redémarrage**, et c'est un arbitrage : le retour automatique au
défaut protégerait du disque plein, la persistance sert celui qui traque un défaut
au lancement. Le prix est payé par `verbose`, que l'interface affiche.

Quand `CANDEO_LOG` est posée, le réglage est **écrit mais pas appliqué** — la
priorité vaut pendant toute l'exécution, pas seulement au démarrage. Il vaudra au
prochain lancement sans la variable, et `forcedByEnv` dit à l'interface de
l'annoncer.

`reset_settings()` ramène aussi le niveau au défaut, et tout de suite : il vient
d'être effacé du fichier, le laisser appliqué ferait mentir l'écran.

### `open_log_dir()`

Ouvre le dossier des journaux dans le gestionnaire de fichiers du système. Un
journal que personne ne sait trouver ne sert à rien, et le chemin dépend du
système : le donner à lire ne suffit pas.

### `diagnostic() -> string`

Le texte à coller dans un rapport de bogue : version de l'application, système,
appareils connus — branché, décision retenue, empreinte de série, gabarit — et
état du moteur appareil par appareil. La version du micrologiciel y est annoncée
comme non lue tant que #35 n'est pas fait, plutôt que passée sous silence.

Ne peut pas échouer sur un appareil : ne pas pouvoir énumérer l'USB ou relire les
réglages est exactement ce qu'un diagnostic doit **dire**, pas ce qui doit
l'interrompre.

### `log_from_webview(level, source, message)`

Consigne dans le même fichier ce que voit la fenêtre : `app.config.errorHandler`
et les refus de compilation d'effet, qui partaient jusqu'ici dans une console que
personne n'ouvre — et qui n'existe pas en `release`, le binaire étant compilé
`windows_subsystem = "windows"`. Cible `candeo_webview`, origine en champ.

`level` exclut `trace` : le par-image vient du moteur, pas de la fenêtre.

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

Une commande répond **une fois** ; un effet produit 30 images par seconde. La
remontée passe donc par `tauri::ipc::Channel`, créé côté front et passé en
argument. Les images y circulent en binaire (`InvokeResponseBody::Raw`) :
396 octets, contre plus de 1,5 Ko sérialisées en tableau JSON d'entiers.

Un canal **par appareil** : le simulateur suit celui qu'on a sélectionné, et
changer de sélection ferme un canal pour en ouvrir un autre. Un abonnement laissé
ouvert sur l'appareil précédent alimenterait le même simulateur en parallèle.

Se désabonner arrête le flux **sans arrêter l'effet**, qui continue d'alimenter
le clavier.

### L'aperçu : `start_preview` · `stop_preview` · `set_preview_params`

```ts
start_preview(device: { vid, pid } | null, id: string, params: object)
```

Le pendant exact de `start_effect`, **moins tout ce qui engage** : aucune sortie
matérielle, rien d'écrit dans `settings.json`, et surtout **aucune boucle
d'appareil arrêtée**.

C'est ce qui rend « sélectionner un effet lance l'aperçu » possible. Le moteur est
à un effet par appareil (issue #26, délibéré) : prévisualiser Y sur un clavier qui
exécute X l'aurait arrêté, autrement dit **parcourir la galerie aurait éteint
l'éclairage en cours**. La boucle d'aperçu est donc distincte, et sa sortie
matérielle est celle qui n'écrit nulle part — `DeviceOut::present` rendant `None`
signifie déjà « aucun appareil ouvert, ce n'est pas un échec ».

`device` désigne l'appareil dont l'aperçu **emprunte le gabarit** : il n'est ni
ouvert, ni piloté, ni forcément branché. `null` retombe sur le gabarit par défaut
— on prévisualise sans posséder de clavier, et sans en avoir adopté aucun.

**Il n'y en a qu'un.** Appeler à nouveau remplace le précédent, et chaque
remplacement détruit un contexte QuickJS pour en construire un autre. La cadence
est donc bornée **du côté du geste** — la fenêtre attend que la sélection se pose
(180 ms) — et non dans le moteur, qui aurait dû choisir entre faire attendre la
dernière sélection et la perdre.

`set_preview_params` ajuste à chaud, comme `set_effect_params` pour un appareil :
la boucle relit son JSON à chaque image.

L'aperçu s'arrête : quand on quitte l'écran, **quand la fenêtre se replie** (c'est
le Rust qui le fait, la vue web n'étant pas détruite), quand l'effet qu'il fait
tourner est supprimé, et avec `stop_all` — remise à zéro de la configuration,
sortie de l'application. L'effet appliqué, lui, survit à tout cela : c'est toute la
différence entre ce que le clavier fait et ce qu'on regarde.

### `subscribe_preview_frames(channel)` · `unsubscribe_preview_frames()`

Comme `subscribe_frames`, pour la boucle d'aperçu. Un canal **distinct** de celui
des appareils : les deux flux existent en même temps, et la fenêtre choisit lequel
elle dessine.

⚠️ Le canal vit dans l'état de la boucle, et `start_preview` en construit une
neuve : **il faut se réabonner après chaque démarrage**, exactement comme pour
`start_effect`. Sans cela le simulateur reste figé sur la dernière image du
précédent, sans qu'aucune erreur ne le dise.

### `engine_status() -> EngineReport`

```ts
{
  devices: {
    device: { vid: number, pid: number },
    running: boolean,
    effectId: string | null,
    error: string | null,
    deviceError: string | null,
    reachingKeyboard: boolean,
    toKeyboard: boolean
  }[],
  preview: {
    layoutOf: { vid: number, pid: number },   // gabarit emprunté
    running: boolean,
    effectId: string | null,
    error: string | null
  } | null
}
```

**Deux champs, et non une liste avec un drapeau.** Ce qui tourne sur le matériel
et ce qu'on regarde ne doivent pas pouvoir se confondre : c'est la quatrième fois
dans ce projet qu'un état qui ment coûte une session de diagnostic — le clavier non
adopté, l'écriture « acceptée », l'image figée après arrêt automatique, et
maintenant l'aperçu. Un drapeau à filtrer se filtre mal : il suffit d'un appelant
qui l'oublie pour annoncer comme tournant sur le clavier un effet qu'on ne fait que
regarder. Ici il n'y a rien à filtrer.

**La zone de notification, le journal et la galerie ne lisent que `devices`.**

`preview` ne porte ni `toKeyboard`, ni `reachingKeyboard`, ni `deviceError` :
une boucle d'aperçu n'a aucune sortie matérielle, et ces champs à faux
décriraient une panne là où il n'y a qu'un choix. Il vaut `null` dès que l'aperçu
est arrêté — « le dernier effet que vous avez regardé » n'est une information pour
personne. Un aperçu qui s'est coupé **tout seul**, après trente images en échec,
reste en revanche visible avec son erreur : c'est la seule façon de savoir
pourquoi l'écran s'est figé.

**Une entrée par appareil** dans `devices`, `reachingKeyboard` compris. Un état
global obligerait à choisir lequel afficher, et le suivant effacerait le précédent
— exactement ce que la table des échecs d'ouverture évite déjà côté adoption.

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

---

## Le seul événement : `candeo://etat-change`

```ts
listen('candeo://etat-change', () => { /* charge utile vide */ })
```

Tout le reste de cette page est **interrogé**. Celui-ci est poussé, et il l'est
pour une raison précise : depuis l'icône de zone de notification
([`src/tray.rs`](../../apps/desktop/src-tauri/src/tray.rs)), l'état peut changer
**sans la fenêtre** — un effet lancé, une sortie coupée, un clavier éteint — et
la fenêtre ne meurt plus quand on la ferme, elle se replie. Son instantané peut
donc vieillir des jours.

Ce qu'elle réinterroge déjà chaque seconde — `engine_status` — n'a pas besoin de
cet événement. Ce qu'elle ne lit qu'**une fois**, au montage, en a besoin : la
liste des appareils, et `settings.json`. Sonder le disque et l'USB en boucle pour
couvrir quelques changements par session serait le mauvais échange.

Émis dans deux cas, et la charge utile est vide dans les deux : rien ne dit *ce*
qui a changé, parce que le destinataire relit de toute façon.

1. après chaque action du menu de l'icône ;
2. quand la fenêtre est ramenée au premier plan — c'est le même chemin que le
   second lancement de l'application, voir `single_instance::reveal`. Une fenêtre
   qui vient d'être **rouverte** ne l'entend pas : son JavaScript n'est pas
   encore chargé, et elle lit tout au montage.

Aucune permission supplémentaire : `core:event:default`, que `core:default`
comprend, accorde déjà `listen`.

Le nom est écrit des deux côtés — `tray::ETAT_CHANGE` et `src/api/candeo.ts` — et
un test Rust confronte les deux : rien d'autre ne les relie, et les désaccorder
donnerait une fenêtre qui ne se resynchronise plus, sans une erreur nulle part.
