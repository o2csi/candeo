# candeo — application de bureau

La fenêtre et son hôte Tauri. Le reste de l'espace de travail vit ailleurs :
`crates/candeo-protocol` construit les rapports HID, `crates/candeo-device` les
envoie, `packages/effects-api` décrit l'API offerte à l'auteur d'effet.

```
src/         la fenêtre — Vue 3, TypeScript, l'éditeur Monaco
src-tauri/   l'hôte Rust — commandes, moteur d'effets, journal, stockage
```

## Lancer

Depuis la **racine** du dépôt, pas d'ici — les scripts passent par le filtre
pnpm et les chemins de `tauri.conf.json` sont relatifs à `src-tauri/` :

```sh
pnpm dev             # fenêtre en développement, rechargement à chaud
pnpm build           # vue-tsc puis vite build
pnpm tauri build     # application empaquetée
pnpm check           # clippy et tests Rust
```

## Où lire la suite

- [`README.md`](../../README.md) à la racine — ce que fait candeo, et pourquoi.
- [`docs/design/studio.md`](../../docs/design/studio.md) — les décisions
  d'architecture de la fenêtre et de l'éditeur.
- [`docs/design/effects-runtime.md`](../../docs/design/effects-runtime.md) — le
  moteur d'effets, la boucle de rendu et le chemin des images.
- [`docs/api/commands.md`](../../docs/api/commands.md) — chaque commande Tauri,
  ses arguments et ses modes d'échec.
- [`docs/protocol/`](../../docs/protocol/) — le relevé du protocole, d'où tout
  le reste dérive.
