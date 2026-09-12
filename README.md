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
| Commandes Tauri | câblées et documentées — voir [`docs/api/`](docs/api/commands.md) |
| Interface Vue | trois écrans — bibliothèque, périphériques, éditeur ; décisions dans [`docs/design/`](docs/design/studio.md) |
| Moteur d'effets utilisateur | moteur unique `rquickjs` côté Rust ; stockage, réglages et **quatre effets livrés** faits |

---

## Origine du protocole

Le protocole a été établi **par observation du matériel** : énumération PnP
Windows, interrogation d'un serveur SDK par son protocole réseau, capture du bus
USB avec USBPcap et Wireshark, puis écriture et lecture directes via l'API HID.

Chaque fait porte la date où il a été établi et **la version de micrologiciel
contre laquelle il l'a été** — v1.5, les 11 et 12/09/2026. C'est ce qui permet de
diagnostiquer un comportement inattendu.

Les faits relatifs à un protocole ne relèvent pas du droit d'auteur, et leur
relevé aux fins d'interopérabilité est prévu par l'**article L.122-6-1 IV du
Code de la propriété intellectuelle** (transposition de la directive 2009/24/CE,
article 6).

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
├── packaging/
│   └── linux/             règle udev, livrée par les paquets deb et rpm
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
- **Fermer la fenêtre replie candeo dans la zone de notification.** C'est la
  seconde moitié de la phrase précédente, et elle n'était pas tenue jusqu'à
  l'issue #46 : un fil indépendant de la fenêtre ne survit pas au processus, et
  le processus s'arrêtait avec sa dernière fenêtre. L'icône le retient — elle
  intercepte `RunEvent::ExitRequested` — et donne de quoi piloter sans la
  fenêtre : effet courant et bibliothèque par appareil piloté, envoi au clavier
  en bascule, extinction. **« Quitter candeo » y est la seule sortie franche**,
  et quitter laisse l'éclairage tel quel. Voir
  [`src/tray.rs`](apps/desktop/src-tauri/src/tray.rs).
- **Le simulateur dessine le vrai clavier**, disposition ISO pleine taille. Le
  périphérique ne déclare que sa grille logique 6 × 22 ; la géométrie physique est
  écrite à la main.

Le cycle de vie complet d'un effet — transpilation, stockage, boucle de rendu,
remontée des images — est décrit dans
[`docs/design/effects-runtime.md`](docs/design/effects-runtime.md).

---

## Portabilité

Développé sous Windows, écrit pour ne pas s'y enfermer. Personne n'a de machine
Linux dans la boucle, alors la CI tient le rôle : le job `linux` construit
l'espace de travail complet sous `ubuntu-latest`, joue les tests, empaquette en
`.deb` et en `.rpm`, et **lit les paquets produits** pour vérifier que la règle
udev s'y trouve.

D'où la distinction que tient cette section : ce qui est **compilé** et ce qui
reste **supposé**.

### Compilé et vérifié

- **Accès matériel.** La dorsale `hidraw` d'`hidapi` se construit sous Linux, et
  rien dans les trois crates ne dépend de Windows pour compiler.
- **Empaquetage.** `.deb` et `.rpm` sont produits, et la règle udev est présente
  dans les deux à `/usr/lib/udev/rules.d/60-candeo.rules` — vérifié par lecture
  des paquets, pas par relecture de la configuration. ⚠️ **Cette vérification ne
  tourne plus sur les propositions de fusion**, seulement sur `main` et à la
  demande : voir [Vérifier Linux en local](#vérifier-linux-en-local).
- **Protocole.** `candeo-protocol` n'a aucune dépendance système : il construit
  des octets, et ses tests tournent partout.
- **Chemins.** Aucun chemin n'est écrit en dur : l'API de Tauri applique la
  convention du système. Les effets vont dans `app_data_dir()`
  (`%APPDATA%\com.oorabona.candeo` ou `~/.local/share/…`), les réglages dans
  `app_config_dir()`. **Jamais dans `Program Files`** — lecture seule pour un
  compte standard, et commun à tous les comptes.

### Supposé, faute de matériel Linux

Il n'y a pas d'USB derrière un coureur GitHub. Trois points attendent un clavier
branché sur une machine Linux : que l'écriture de rapport de fonctionnalité
aboutisse par hidraw, que `interface_number` distingue les interfaces du
composite comme sous Windows, et que les dossiers résolus par Tauri tombent bien
dans `~/.local/share` et `~/.config`.

### Vérifier Linux en local

Le job `linux` de la CI coûte un quart d'heure, dont une dizaine de minutes à
recompresser un `.rpm` de débogage. Il ne tourne donc **pas** sur les
propositions de fusion : seulement sur `main`, et à la demande via
**Actions → CI → Run workflow** en choisissant la branche.

Entre-temps, WSL fait le même travail sans attendre un coureur :

```bash
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev \
  libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev \
  libudev-dev build-essential pkg-config file rpm

cargo check --workspace --all-targets
cargo test --workspace
```

Et si le changement touche l'empaquetage, les I/O HID ou les dépendances
système, la vérification complète — c'est l'étape lente, à ne lancer que dans ce
cas :

```bash
pnpm install --frozen-lockfile
pnpm --filter @candeo/desktop exec tauri build --debug --bundles deb,rpm

regle='usr/lib/udev/rules.d/60-candeo.rules'
dpkg-deb -c "$(ls target/debug/bundle/deb/*.deb | head -1)" | grep -F "$regle"
rpm -qpl "$(ls target/debug/bundle/rpm/*.rpm | head -1)" | grep -F "/$regle"
```

Les deux `grep` sont la preuve : la règle est bien **dans les paquets**, et pas
seulement déclarée dans `tauri.conf.json`.

### Règle udev

`/dev/hidraw*` est créé en `0600 root:root`. Le fichier est dans
[`packaging/linux/`](packaging/linux/60-candeo.rules) ; les paquets `deb` et
`rpm` l'installent. Depuis les sources, il faut le copier soi-même :

```bash
sudo cp packaging/linux/60-candeo.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules && sudo udevadm trigger
```

Le détail — pourquoi `uaccess` plutôt qu'un groupe, pourquoi le préfixe 60,
pourquoi les dorsales `*-native` d'`hidapi` ont été évaluées puis écartées — est
dans [`docs/design/effects-runtime.md`](docs/design/effects-runtime.md) §6.

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
import { defineEffect, hsv } from '@candeo/effects-api'

// Un export par défaut, et rien d'autre : c'est tout ce que le moteur cherche.
// `defineEffect` ne fait rien à l'exécution — elle donne un type contextuel,
// ce qui type `layout`, `time`, `frame`, et `params` d'après sa déclaration.
export default defineEffect({
  name: 'Onde',
  params: {
    speed: { kind: 'number', label: 'Vitesse', min: 0, max: 400, default: 120 },
  },
  render({ layout, time, frame, params }) {
    const cx = (layout.cols - 1) / 2
    const cy = (layout.rows - 1) / 2
    // `layout.keys` : les 106 positions éclairées, pas les 132 cases.
    for (const key of layout.keys) {
      const d = Math.hypot(key.col - cx, key.row - cy)
      // `params.speed` est un `number`, déduit de sa déclaration ci-dessus.
      frame.set(key, hsv(time * params.speed + d * 18, 1, 1))
    }
  },
})
```

Les quatre effets livrés avec l'application — onde radiale, respiration,
balayage, dégradé fixe — sont écrits contre cette même API, dans
[`apps/desktop/src-tauri/src/builtins/`](apps/desktop/src-tauri/src/builtins/).
Ils sont compilés dans le binaire, pas écrits en Rust : le premier exemple qu'on
ouvre doit être exactement ce qu'on peut écrire soi-même.

---

## Développement

```bash
pnpm install
pnpm dev            # application en mode developpement
pnpm check          # clippy + tests Rust
cargo test -p candeo-protocol   # tests du protocole, sans materiel
```

### Prérequis

Rust stable, Node 22+, pnpm 10+.

Sous **Windows**, le runtime WebView2 — présent par défaut sur Windows 11.

Sous **Debian / Ubuntu**, les dépendances système de Tauri 2, plus `libudev-dev`
qu'`hidapi` résout par `pkg-config` :

```bash
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev \
  libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev \
  libudev-dev build-essential pkg-config file
```

C'est la liste qu'installe le job `linux` de la CI — si elle se périme, la CI le
dit. Elle y ajoute `rpm`, qui ne sert qu'à relire le paquet produit.

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
