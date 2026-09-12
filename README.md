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
└── docs/protocol/         relevé du protocole et captures
```

La séparation `protocol` / `device` est délibérée : la construction des rapports
et la somme de contrôle se testent **sans matériel**, en intégration continue.

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

- L'image doit couvrir **toutes** les positions de la matrice (132 pour le
  DeathStalker), pas seulement celles portant une LED. En envoyer moins laisse
  les dernières rangées figées sur leur valeur précédente.
- La chaîne de variante du périphérique **change avec le micrologiciel**
  (`v1.4 / Unkown Variant` → `v1.5 / Quartz`). Identifier sur VID / PID / série.
- Le tampon `HidD_SetFeature` fait **91 octets** : identifiant de rapport, puis
  les 90 octets du rapport.
- Sur un composite USB, ouvrir la mauvaise interface donne un handle valide sur
  lequel toute écriture échoue.
