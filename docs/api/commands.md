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
compilés dans le binaire et n'ont pas de dossier, `kind` les distingue. Aucun
n'est encore livré, le champ existe pour que l'interface n'ait pas à changer
quand ce sera le cas.

Un dossier dont le manifeste est illisible est ignoré, pas propagé en erreur :
une bibliothèque de vingt effets ne doit pas disparaître à cause d'un seul.
L'ordre est stable — le système de fichiers n'en garantit aucun.

### `delete_effect(id)`

Supprime le dossier. Un `id` hors de la liste blanche est refusé avant tout
accès au disque.

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

## Surface prévue — effets utilisateur

Le stockage est en place (voir ci-dessus) ; l'exécution ne l'est pas. La
conception est figée dans
[`../design/effects-runtime.md`](../design/effects-runtime.md) ; c'est ici que
la forme des commandes sera consignée au fur et à mesure.

### Exécution

| Commande | Rôle |
|---|---|
| `start_effect(id, params)` | démarre le fil de rendu |
| `stop_effect()` | l'arrête |
| `set_effect_params(params)` | ajuste à chaud, sans redémarrer |
| `subscribe_frames(channel)` | ouvre le flux d'images vers le simulateur |

### Le flux d'images ne passe pas par une commande

Une commande répond **une fois** ; un effet produit 60 images par seconde. La
remontée se fait donc par `tauri::ipc::Channel`, créé côté front et passé en
argument de `subscribe_frames`. Les images y circulent en binaire
(`InvokeResponseBody::Raw`) : 396 octets, contre plus de 1,5 Ko si on les
sérialisait en tableau JSON d'entiers.

Libérer le canal suffit à arrêter le flux — sans arrêter l'effet, qui continue
d'alimenter le clavier fenêtre fermée.
