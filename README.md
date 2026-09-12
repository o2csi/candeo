# candeo

> *candeo*, verbe latin — « je brille, je rayonne ». La racine de *candela*,
> l'unité SI d'intensité lumineuse.

Contrôle de l'éclairage de claviers, par **accès HID direct**. Pas de runtime
constructeur, pas de service tiers, pas de couche d'abstraction : l'application
parle au périphérique.

---

## État

Preuve de concept fonctionnelle sur **Razer DeathStalker V2 Pro (filaire)**.
Le protocole a été relevé par capture du bus USB, puis **validé en écriture
directe** — couleurs unies, dégradé par rangée, écriture partielle de rangée et
bascule d'effet micrologiciel, le tout sans aucun logiciel tiers actif.

| Couche | État |
|---|---|
| `candeo-protocol` — rapports et somme de contrôle | fait, testé contre une trame capturée |
| `candeo-device` — transport HID et gabarits | fait, validé sur matériel |
| Commandes Tauri | 14 commandes câblées — voir [`docs/api/`](docs/api/commands.md) |
| Interface Vue | conçue, pas encore écrite — voir [`docs/design/`](docs/design/studio.md) |
| Moteur d'effets utilisateur | conçu : moteur unique `rquickjs` côté Rust ; stockage des effets et des réglages fait |

---

## Origine du protocole

Le protocole implémenté ici a été établi **uniquement par observation du
matériel** : énumération PnP Windows, interrogation d'un serveur SDK par son
protocole réseau, et capture du bus USB avec USBPcap et Wireshark.

**Aucun code source tiers n'a été consulté.** Ni OpenRGB, ni openrazer, ni les
greffons SignalRGB.

Les faits relatifs à un protocole ne relèvent pas du droit d'auteur, et leur
relevé aux fins d'interopérabilité est prévu par l'**article L.122-6-1 IV du
Code de la propriété intellectuelle** (transposition de la directive 2009/24/CE,
article 6). Ce projet n'est donc **pas** une œuvre dérivée d'OpenRGB et n'est pas
soumis à sa licence GPL-2.0-or-later.

La documentation complète du protocole, avec les trames commentées et la méthode
de capture reproductible, est dans [`docs/protocol/`](docs/protocol/).

---

## Organisation

```
candeo/
├── crates/
│   ├── candeo-protocol/   construction des rapports — pur, sans I/O, testable
│   └── candeo-device/     transport HID et gabarits de périphériques
├── apps/
│   └── desktop/           application Tauri (Vue 3 + TypeScript)
│       └── src-tauri/     liaison Rust, boucle de rendu
├── packages/
│   └── effects-api/       types TypeScript pour l'écriture d'effets
└── docs/
    ├── protocol/          relevé du protocole et méthode de capture
    ├── api/               commandes exposées au front
    └── design/            décisions d'interface et d'exécution, prises avant code
```

La séparation `protocol` / `device` est délibérée : la construction des rapports
et la somme de contrôle se testent **sans matériel**, en intégration continue.

---

## Conception de l'interface

Les décisions sont figées avant d'écrire du Vue, et documentées dans
[`docs/design/studio.md`](docs/design/studio.md). Maquette de référence :
non publiée

Les trois partis pris qui structurent le reste :

- **La galerie d'effets est l'écran d'accueil**, pas l'éditeur. Un « ＋ » entre en
  édition ; la plupart des lancements servent à choisir, pas à écrire.
- **Un seul moteur exécute les effets**, `rquickjs` côté Rust, dans un fil
  indépendant de la fenêtre. Le front n'exécute jamais de code utilisateur : il
  envoie la source et reçoit les images. L'aperçu **est** donc la production, et
  un effet continue de tourner fenêtre fermée. Le motif n'a jamais été la
  performance — 132 LED × 60 img/s = 7 920 couleurs/s, trivial.
- **Le simulateur dessine le vrai clavier**, disposition ISO pleine taille. Le
  périphérique ne déclare que sa grille logique 6 × 22 ; la géométrie physique est
  écrite à la main.

Le cycle de vie complet d'un effet — transpilation, stockage, boucle de rendu,
remontée des images — est décrit dans
[`docs/design/effects-runtime.md`](docs/design/effects-runtime.md).

---

## Portabilité

Développé sous Windows, écrit pour ne pas s'y enfermer.

- **Accès matériel.** `hidapi` couvre Windows, Linux, macOS et illumos ;
  l'écriture de rapport de fonctionnalité est identique partout. Sous Linux,
  `/dev/hidraw*` exige une règle udev, livrée avec le paquet.
- **Chemins.** Aucun chemin n'est écrit en dur : l'API de Tauri applique la
  convention du système. Les effets vont dans `app_data_dir()`
  (`%APPDATA%\com.oorabona.candeo` ou `~/.local/share/…`), les réglages dans
  `app_config_dir()`. **Jamais dans `Program Files`** — lecture seule pour un
  compte standard, et commun à tous les comptes.
- **Protocole.** `candeo-protocol` n'a aucune dépendance système : il construit
  des octets, et ses tests tournent partout.

---

## Le protocole en bref

Transfert de contrôle USB, `SET_REPORT` sur rapport de fonctionnalité, interface 3.

```
bmRequestType 0x21   bRequest 0x09   wValue 0x0300   wIndex 3   wLength 90
```

Rapport de 90 octets :

| Offset | Contenu |
|---|---|
| 1 | identifiant de transaction (`0x9f`) |
| 5 | taille des arguments |
| 6 | classe — `0x0f` = éclairage |
| 7 | commande — `0x02` effet, `0x03` rangée, `0x04` luminosité |
| 8+ | arguments |
| 88 | **XOR des octets 2 à 87** |

Couleurs en **RGB** — à ne pas confondre avec le SDK Chroma, qui est en `0x00BBGGRR`.

---

## Écrire un effet

Un effet est une fonction du temps et de la position vers une couleur. C'est
précisément pour cela que **YAML ne convient pas** : on y décrirait une
configuration, pas un comportement.

```ts
import { hsv, type EffectModule } from '@candeo/effects-api'

export const ripple: EffectModule = {
  name: 'Onde',
  params: {
    speed: { kind: 'number', label: 'Vitesse', min: 0, max: 400, default: 120 },
  },
  render({ layout, time, frame, params }) {
    const cx = (layout.cols - 1) / 2
    const cy = (layout.rows - 1) / 2
    for (const key of layout.keys) {
      const d = Math.hypot(key.col - cx, key.row - cy)
      frame.set(key, hsv(time * Number(params.speed) + d * 18, 1, 1))
    }
  },
}
```

---

## Développement

```bash
pnpm install
pnpm dev            # application en mode developpement
pnpm check          # clippy + tests Rust
cargo test -p candeo-protocol   # tests du protocole, sans materiel
```

### Prérequis

Rust stable, Node 22+, pnpm 10+, et le runtime WebView2 (présent par défaut sur
Windows 11).

---

## Pièges rencontrés, pour mémoire

- **132 et 106 ne sont pas la même chose.** La matrice fait 6 × 22 = **132**
  cases, et c'est ce qu'une image doit couvrir ; **106** seulement portent une
  touche. En envoyer 106 laisse les dernières rangées figées sur leur valeur
  précédente — symptôme vécu : la rangée du bas restée blanche.
- La chaîne de variante du périphérique **change avec le micrologiciel**
  (`v1.4 / Unkown Variant` → `v1.5 / Quartz`). Identifier sur VID / PID / série.
- Le tampon `HidD_SetFeature` fait **91 octets** : identifiant de rapport, puis
  les 90 octets du rapport.
- Sur un composite USB, ouvrir la mauvaise interface donne un handle valide sur
  lequel toute écriture échoue.
