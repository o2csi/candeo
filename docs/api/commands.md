# Commandes Tauri — surface exposée au front

Définies dans [`apps/desktop/src-tauri/src/lib.rs`](../../apps/desktop/src-tauri/src/lib.rs).

Les types sérialisés vivent dans la couche Tauri, **pas** dans les crates :
`candeo-protocol` et `candeo-device` restent ainsi sans dépendance à serde ni à
Tauri, donc réutilisables hors application et testables en intégration continue.

L'état est un `Mutex<Option<Keyboard>>` : un seul périphérique ouvert à la fois,
ce qui suffit tant que l'interface n'en pilote qu'un.

---

## Découverte et connexion

### `list_devices() -> DeviceInfo[]`

Liste **tous les gabarits connus**, branchés ou non.

```ts
{ name: string, vid: number, pid: number, present: boolean }
```

`present` est vrai si le VID, le PID **et** le numéro d'interface correspondent à
un périphérique énuméré. L'interface doit afficher un gabarit absent comme absent,
pas l'omettre — c'est ce qui permet de dire « branchez votre clavier » plutôt que
de montrer une liste vide.

### `connect(vid, pid) -> LayoutInfo`

Ouvre le périphérique et renvoie son gabarit. Échoue si aucun gabarit connu ne
correspond, ou si l'ouverture HID échoue.

### `disconnect()` · `is_connected() -> boolean`

Libération explicite, et interrogation de l'état.

---

## Gabarit

### `get_layout() -> LayoutInfo`

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

### `set_brightness(level: number)`

`level` de 0 à 255.

### `set_effect(effect: EffectDto)`

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

### `present(frame: number[])`

Image complète : suite plate de triplets RGB, **`frameLen × 3` octets exactement**
(396 pour le DeathStalker). Une taille différente est refusée avec un message
explicite plutôt que d'écrire partiellement.

En interne : six transferts `0x0f`/`0x03`, un par rangée, puis une bascule en
effet `custom`.

### `write_row(row, col_start, colors: number[])`

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
app_data_dir()/effects/<id>/     source.ts · effect.js · manifest.json
app_config_dir()/settings.json
```

### `install_effect(source_ts, js, manifest) -> string`

Écrit les trois fichiers et renvoie l'`id` retenu.

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
  name: string,
  description: string,
  params: Record<string, ParamSpec>,
  apiVersion: number
}
```

Effets intégrés **et** installés, dans une seule liste : les intégrés sont
compilés dans le binaire et n'ont pas de dossier, `kind` les distingue. Ils
viennent en tête, les installés ensuite, triés par `id`.

Un dossier dont le manifeste est illisible est ignoré, pas propagé en erreur :
une bibliothèque de vingt effets ne doit pas disparaître à cause d'un seul.
L'ordre est stable — le système de fichiers n'en garantit aucun.

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
  device: { vid: number, pid: number } | null
}
```

Au premier lancement il n'y a pas de fichier : `get_settings` renvoie les
**défauts**, ce n'est pas une erreur. Un champ absent d'un fichier écrit par une
version antérieure reprend lui aussi son défaut, plutôt que de rendre
l'application muette au démarrage.

L'écriture passe par un fichier temporaire suivi d'un renommage : une coupure en
cours d'écriture laisserait sinon des réglages tronqués.

---

## Erreurs

Toutes les commandes faillibles renvoient `Result<T, String>`. Le message est
destiné à être **affiché tel quel** : il doit rester lisible par un humain, pas
devenir un code à traduire côté front.

---


## Moteur d'effets

Un fil de rendu Rust, **indépendant de la fenêtre** : fermer l'application
n'éteint pas l'effet. C'est le seul endroit où du code d'effet s'exécute — le
front n'en exécute jamais, ce qui lui retire au passage tout accès au DOM et à
l'API Tauri. Conception dans
[`../design/effects-runtime.md`](../design/effects-runtime.md) §4 et §5.

### `start_effect(id, params)`

Charge le JavaScript de l'effet — celui d'un intégré, sinon
`effects/<id>/effect.js` — et démarre la boucle. Le moteur ne fait aucune
différence entre les deux : un effet livré est un module chargé exactement
comme celui qu'on vient d'écrire.

Remplace l'effet en cours, s'il y en avait un — l'arrêt précédent est
**attendu**, sans quoi deux boucles écriraient un instant sur le même clavier.

Une erreur de syntaxe ou un module mal formé est signalé **à l'appel**, pas
découvert plus tard dans un état : l'appel attend le verdict du chargement.

Le gabarit vient du périphérique connecté ; à défaut, du gabarit par défaut.
Délibéré : on doit pouvoir écrire et prévisualiser un effet **sans posséder le
clavier**.

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

### `stop_effect()` · `set_effect_params(params)`

`set_effect_params` ajuste **à chaud** : la boucle relit les paramètres à chaque
image, elle ne redémarre pas.

### `set_output_to_keyboard(on)`

Coupe ou rétablit l'écriture vers le clavier **sans toucher au simulateur**. Les
deux sorties de la boucle sont indépendantes, et chacune peut être absente :

- fenêtre fermée → seule l'écriture HID subsiste, aucune image n'est sérialisée ;
- sortie clavier coupée → seul le simulateur est alimenté ;
- les deux actives → l'aperçu montre exactement les octets envoyés.

### `subscribe_frames(channel)` · `unsubscribe_frames()`

Une commande répond **une fois** ; un effet produit 60 images par seconde. La
remontée passe donc par `tauri::ipc::Channel`, créé côté front et passé en
argument. Les images y circulent en binaire (`InvokeResponseBody::Raw`) :
396 octets, contre plus de 1,5 Ko sérialisées en tableau JSON d'entiers.

Se désabonner arrête le flux **sans arrêter l'effet**, qui continue d'alimenter
le clavier.

### `engine_status() -> EngineStatus`

```ts
{ running: boolean, effectId: string | null, error: string | null, toKeyboard: boolean }
```

Interrogé plutôt que poussé : une erreur survenue fenêtre fermée doit se lire à
la réouverture, ce qu'un événement ponctuel ne permet pas.

Une exception dans un effet **ne fait pas tomber l'application** : elle est
rattrapée par image, exposée ici, et effacée dès que l'effet se rétablit. Après
trente images consécutives en échec, la boucle s'arrête — un effet qui lève à
chaque image ne se rétablira pas tout seul.
