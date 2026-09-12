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
  frame_len: number,     // 132 — taille d'une image
  keys: {
    index: number,       // rang dans une image
    row: number, col: number,   // position dans la matrice logique
    name: string,        // « Échap », « Maj gauche », « Pavé + »…
    x: number, y: number, w: number, h: number   // rectangle physique
  }[]                    // 106 entrées
}
```

> **`frame_len` et `keys.length` diffèrent, et c'est voulu.**
> `frame_len` vaut **132** — toutes les cases de la matrice, trous compris : c'est
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

Image complète : suite plate de triplets RGB, **`frame_len × 3` octets exactement**
(396 pour le DeathStalker). Une taille différente est refusée avec un message
explicite plutôt que d'écrire partiellement.

En interne : six transferts `0x0f`/`0x03`, un par rangée, puis une bascule en
effet `custom`.

### `write_row(row, col_start, colors: number[])`

Écrit un segment de rangée sans toucher au reste — l'écriture partielle est prise
en charge par l'appareil, vérifié sur le matériel. Utile aux effets localisés,
qui évitent ainsi de réémettre les 132 positions.

---

## Erreurs

Toutes les commandes faillibles renvoient `Result<T, String>`. Le message est
destiné à être **affiché tel quel** : il doit rester lisible par un humain, pas
devenir un code à traduire côté front.

---

## Surface prévue — effets utilisateur

Rien de ce qui suit n'est encore implémenté. La conception est figée dans
[`../design/effects-runtime.md`](../design/effects-runtime.md) ; c'est ici que
la forme des commandes sera consignée au fur et à mesure.

### Bibliothèque

| Commande | Rôle |
|---|---|
| `install_effect(source_ts, js, manifest)` | écrit `effects/<id>/` et renvoie l'`id` |
| `list_effects()` | effets intégrés **et** utilisateur, avec leur nature |
| `delete_effect(id)` | supprime le dossier |

Le front envoie le JavaScript déjà transpilé : le transpileur est celui de
Monaco. Il envoie **aussi** la source TypeScript, sans quoi l'effet ne serait
plus modifiable.

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
