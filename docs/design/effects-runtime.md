# Cycle de vie d'un effet — de l'éditeur au clavier

Ce document décrit ce qui se passe entre le moment où l'on tape du TypeScript et
celui où une touche s'allume. Les décisions d'**interface** sont dans
[`studio.md`](studio.md) ; celles-ci concernent l'exécution.

---

## 1. Vue d'ensemble

```
  ┌─ WebView ──────────────────────┐        ┌─ Rust ────────────────────────────┐
  │                                │        │                                   │
  │  Monaco (TypeScript)           │        │                                   │
  │      │ ts.transpileModule()    │        │                                   │
  │      ▼                         │        │                                   │
  │  JavaScript ───────────────────┼─ ①  ──▶│  écriture disque   effects/<id>/  │
  │                                │commande│        │                          │
  │                                │        │        ▼                          │
  │                                │        │  ┌── rquickjs ──┐                 │
  │  simulateur  ◀─────────────────┼─ ② ────┼──│  boucle 60 Hz │──▶ HID ──▶ 🖮   │
  │              (images)          │ canal  │  └──────────────┘                 │
  └────────────────────────────────┘        └───────────────────────────────────┘
```

Le sens descendant ① et le sens montant ② **ne sont pas le même mécanisme**, et
c'est important : voir §4 et §5.

---

## 2. Descente — de l'éditeur au disque

1. **Transpilation dans le front.** `ts.transpileModule()`, fourni par Monaco
   (voir [`studio.md`](studio.md) §2), retire les types. Rien de plus : pas de
   regroupement, pas de résolution d'imports.
2. **Commande `install_effect`.** Le front envoie la **source TypeScript**, le
   **JavaScript produit** et un manifeste (nom, paramètres déclarés).
3. **Écriture sur disque**, sous le dossier décrit au §3.

### Pourquoi stocker les deux

| Fichier | Rôle | Indispensable ? |
|---|---|---|
| `source.ts` | rouvrir l'effet dans l'éditeur | oui, sinon l'effet n'est plus modifiable |
| `effect.js` | ce que le moteur exécute | **oui** |
| `manifest.json` | nom, description, paramètres, version de l'API | oui |
| `swatch.json` | le repère de couleurs de la bibliothèque | non — son absence donne une pastille neutre |

Le `swatch.json` est le seul des quatre que **personne n'écrit** : il est prélevé
en exécutant l'effet à l'installation. Déclaré, il dériverait dès la première
modification du code. Le mécanisme est décrit dans
[`../api/commands.md`](../api/commands.md), § « Le repère de couleurs ».

Le `.js` n'est pas un cache que l'on pourrait régénérer à la demande : le
transpileur vit dans le front, donc **régénérer exigerait d'ouvrir la fenêtre**.
Or un effet doit pouvoir démarrer sans interface — à l'ouverture de session, ou
après un redémarrage. Le `.js` est donc un livrable, pas un artefact jetable.

---

## 3. Où vivent les effets

> **Pas dans `Program Files`.** Ce dossier est en lecture seule pour un compte
> standard, et son contenu est commun à tous les comptes de la machine. Un effet
> est du contenu **écrit par l'utilisateur, propre à l'utilisateur**.

On ne construit pas ces chemins à la main : l'API de Tauri applique la convention
de chaque système.

| Appel Tauri | Windows | Linux | macOS |
|---|---|---|---|
| `app_data_dir()` | `%APPDATA%\com.oorabona.candeo` | `~/.local/share/com.oorabona.candeo` | `~/Library/Application Support/…` |
| `app_config_dir()` | `%APPDATA%\com.oorabona.candeo` | `~/.config/com.oorabona.candeo` | `~/Library/Application Support/…` |

Sous Windows les deux se confondent ; sous Linux non, d'où l'intérêt de passer
par l'API plutôt que par une constante.

**Répartition retenue :**

```
app_data_dir()/effects/<id>/     source.ts · effect.js · manifest.json · swatch.json
app_config_dir()/settings.json   préférences · appareils · effet appliqué · réglages
```

L'effet est du **contenu** (`data`), le choix de l'effet actif est de la
**configuration** (`config`). Distinction sans objet sous Windows, exacte sous
Linux — et gratuite dans les deux cas.

L'état d'adoption de chaque appareil (§7) est de la configuration au même titre :
c'est une décision de l'utilisateur sur sa machine, pas du contenu qu'on
emporterait ailleurs.

> Les effets **intégrés** ne sont pas sur disque : ils sont compilés dans le
> binaire. Seuls les effets écrits par l'utilisateur ont un dossier. Leur repère
> de couleurs vit donc en mémoire, calculé une fois par exécution : il est une
> propriété du binaire, pas de la bibliothèque de l'utilisateur.

### Les effets intégrés sont du JavaScript, pas du Rust

Ils vivent dans `apps/desktop/src-tauri/src/builtins/`, un fichier `.js` chacun,
embarqués par `include_str!` et chargés par le moteur comme n'importe quel effet.

Les écrire en Rust natif les rendrait plus rapides — et ne prouverait rien. Ils
sont là pour être lus : le premier exemple qu'on ouvre doit être **exactement**
ce qu'on peut écrire soi-même, même API, même `export default`. Un exemple qu'on
ne peut pas reproduire n'est pas un exemple, c'est une démonstration.

Conséquence assumée : ils n'ont pas de `.ts`. Leur JavaScript est leur source,
donc rien à transpiler à la compilation, et `read_effect_source` les rend tels
qu'ils s'exécutent.

Le manifeste, lui, est écrit deux fois — en Rust pour que lister la bibliothèque
n'instancie aucun moteur, et dans le module parce que c'est le contrat de l'API.
Un test charge chaque effet et compare les deux ; le module fait foi.

### Un identifiant intégré est réservé

Les intégrés partagent l'espace de noms des effets utilisateur : même validation,
même `id` dans `settings.json`. Deux garde-fous, dans cet ordre :

1. `install_effect` **refuse** un nom qui dérive vers un identifiant intégré ;
2. la résolution `id → JavaScript` consulte les intégrés **d'abord**.

Le second n'est utile que si le premier a été contourné — un dossier copié à la
main, une bibliothèque héritée d'une version où l'identifiant était libre. Le
sens de la priorité découle de ce qu'on refuse : une entrée marquée `builtin`
dans la galerie doit exécuter le code livré. La priorité inverse laisserait un
effet utilisateur se glisser sous un nom connu, le manifeste de l'intégré affiché
et un autre code exécuté.

---

## 4. Le moteur — une boucle par appareil, deux sorties chacune

Un fil Rust par appareil, indépendant de toute fenêtre. Chacun instancie son
contexte `rquickjs`, charge son `effect.js`, et appelle sa fonction de rendu à
cadence fixe.

```
   appareil A                              appareil B
        ┌──────────────────────────┐            ┌───────────────┐
        │  rquickjs — render(ctx)  │  60 Hz     │  rquickjs — … │
        └────────────┬─────────────┘            └───────┬───────┘
                     │  image : 132 triplets RGB        │
          ┌──────────┴───────────┐                      ▼
          ▼                      ▼                   sortie A'
   écriture HID            canal → simulateur
   (si ouvert et           (si la fenêtre écoute
    « envoyer » actif)      **cet** appareil)
```

Les deux sorties sont **indépendantes**, et chacune peut être absente :

- fenêtre fermée → seule l'écriture HID subsiste ; aucune image n'est sérialisée ;
- bascule « envoyer au clavier » désactivée → seul le simulateur est alimenté,
  ce qui permet d'écrire un effet **sans posséder le clavier** ;
- les deux actives → l'aperçu montre exactement les octets envoyés.

C'est cette dernière propriété qui justifie le moteur unique — au sens : un seul
endroit où le code d'effet s'exécute. Le simulateur n'interprète pas le code, il
affiche le résultat.

> ⚠️ **« Fenêtre fermée » n'a rien de gratuit, et ne l'a pas toujours été.** Un
> fil indépendant de la fenêtre ne survit pas au processus, et le processus
> s'arrêtait avec sa dernière fenêtre — rien n'empêchait
> `RunEvent::ExitRequested`. Ce que ce paragraphe décrit n'a donc été vrai qu'à
> partir de l'issue #46 : l'icône de zone de notification retient la sortie, et
> la croix de la fenêtre **replie** au lieu de quitter. « Quitter candeo », dans
> le menu de l'icône, est la seule chose qui arrête une boucle de rendu par la
> fin du processus — et elle laisse l'éclairage tel quel plutôt que de
> l'éteindre. Voir [`src/tray.rs`](../../apps/desktop/src-tauri/src/tray.rs).

### Un appareil, un effet

Chaque appareil porte sa boucle, donc sa cadence, ses paramètres, son état
d'erreur, sa sortie et son `reachingKeyboard`. **Rien n'est partagé entre deux
appareils**, et c'est ce qui fait qu'un appareil en panne n'en affecte aucun
autre — l'invariant de l'adoption (§7), tenu cette fois en marche et non
seulement à l'ouverture.

L'autre modèle — un effet couvrant plusieurs appareils, avec un gabarit
composite — permettrait une vague traversant du clavier au tapis de souris. Il
exige d'abord de décrire où les appareils se trouvent les uns par rapport aux
autres, ce qu'on ne peut pas concevoir correctement avec un seul appareil sous la
main.

**Il n'est pas fermé pour autant.** Une boucle reçoit **un gabarit** et **une
sortie**, jamais « un clavier » : le trait `DeviceOut` est le seul point où elle
touche du matériel. Le jour où un gabarit couvrira plusieurs appareils, c'est là
que l'image se répartira — ni la boucle, ni le code des effets n'auront à changer.

C'est aussi ce joint qui rend l'invariant vérifiable : un test fait échouer
**toutes** les écritures d'un appareil et constate que le voisin garde sa boucle,
ses images, son état vierge — et qu'arrêter le premier n'arrête pas le second.
Avec le `Keyboard` en dur, cela n'aurait été vérifiable qu'avec deux claviers sur
le bureau, donc jamais.

### L'ordre de prise des verrous

Un interblocage a déjà été attrapé : `list_devices` prenait le verrou du clavier
puis celui des échecs, `ignore_device` l'inverse. Avec une table d'appareils et N
boucles, la règle est explicite :

> **Aucun code ne tient deux verrous en même temps.** Une table est verrouillée le
> temps d'y lire ou d'y poser un pointeur partagé — jamais le temps d'une écriture
> HID, d'un démarrage de boucle ni d'une attente de fin.

Là où deux deviendraient inévitables, l'ordre est celui de la déclaration dans
`AppState` : table des appareils → moteur → table des échecs → poignée d'un
appareil → état partagé d'une boucle.

Deux conséquences, et elles ne sont pas cosmétiques :

- **le fil de rendu ne connaît que les deux derniers.** Il n'a aucun moyen de
  prendre un verrou de l'application, donc aucun moyen d'en bloquer une commande ;
- **la seule attente tenue verrou en main est celle de `stop`, et ce verrou est
  propre à l'appareil.** L'arrêt attend la fin de la boucle — sans quoi enchaîner
  deux effets laisserait un instant deux boucles écrire sur le même appareil —
  mais cette attente ne retient aucune commande visant les autres. Une écriture
  HID bloquée sur l'un ne gèlerait pas l'autre, ce qu'un verrou global du moteur
  aurait réintroduit par la bande.

### Résolution des imports

`import { hsv, mix } from '@candeo/effects-api'` est résolu par le chargeur de
modules de `rquickjs` vers un module **interne**, fourni par l'hôte. Pas de
regroupement, pas de résolution de chemins, pas de `node_modules`.

Le `manifest.json` porte la version de l'API utilisée à l'écriture : c'est ce qui
permettra de refuser proprement un effet écrit contre une API disparue, plutôt
que de le laisser échouer à la première image.

---

## 5. Remontée — un canal, pas un événement global

Tauri offre deux directions, et elles n'ont pas les mêmes primitives :

| Sens | Primitive | Forme |
|---|---|---|
| front → Rust | **commande** (`invoke`) | requête / réponse, attendue |
| Rust → front | **événement** ou **canal** | poussée, sans réponse |

Une commande ne peut pas « rendre » 60 images par seconde : elle répond une fois.
La remontée passe donc par un canal — `tauri::ipc::Channel`, créé par le front et
passé en argument d'une commande d'abonnement, avec **l'appareil dont on veut les
images**. Le simulateur suit celui qui est sélectionné ; changer de sélection
ferme un canal et en ouvre un autre, plutôt que de multiplexer un flux unique.

**Canal plutôt qu'événement global** (`emit` / `listen`) pour trois raisons :

1. pas de diffusion à toutes les fenêtres ni de recherche dans un registre
   d'écouteurs — la destination est connue ;
2. la portée est explicite : le canal libéré, le flux s'arrête. Pas de fuite
   d'abonnement à la fermeture de l'éditeur ;
3. il transporte du binaire. `InvokeResponseBody::Raw` évite de sérialiser une
   image en tableau JSON d'entiers, qui la ferait passer de **396 octets à plus
   de 1,5 Ko de texte** — pour rien, 60 fois par seconde.

Rappel de proportion : 132 LED × 60 images/s = **7 920 couleurs par seconde**.
Le choix du canal n'est pas une optimisation nécessaire, c'est simplement la
primitive juste pour un flux ; autant la prendre.

---

## 6. Portabilité de l'accès matériel

`hidapi` couvre Windows, Linux, macOS et illumos — l'écriture de rapport de
fonctionnalité y est la même. Ce qui change d'un système à l'autre :

| | Windows | Linux |
|---|---|---|
| Dorsale par défaut | hidapi C (`hid.dll`) | `linux-static-hidraw` (compile du C, lie `libudev`) |
| Dorsale pur Rust | `windows-native` | `linux-native` (crate `udev` + `nix`) |
| Accès non privilégié | immédiat | **règle udev requise** |

> **Compilé, ou supposé ?** Le dépôt distingue les deux. Un job `linux` de la
> CI construit l'espace de travail complet sous `ubuntu-latest` : ce qui y passe
> est *compilé*. Le reste — tout ce qui exige un clavier branché sur une machine
> Linux — reste *supposé*, et est signalé comme tel ci-dessous.

### Ce que la CI établit

Le job `linux` (`.github/workflows/ci.yml`) fait, dans cet ordre : dépendances
système Debian de Tauri 2 plus `libudev-dev`, `cargo check --workspace
--all-targets`, `cargo test --workspace`, puis `tauri build --debug --bundles
deb,rpm` et lecture des deux paquets produits.

Il établit donc que :

- la dorsale `linux-static-hidraw` se compile et se lie ;
- aucune partie de l'espace de travail — `candeo-protocol`, `candeo-device`,
  `candeo-desktop` — ne dépend de Windows pour compiler ;
- les tests passent à l'identique sur un système de fichiers sensible à la
  casse ;
- l'application s'empaquette en `.deb` et en `.rpm` ;
- la règle udev est **réellement présente** dans les deux, à
  `/usr/lib/udev/rules.d/60-candeo.rules`. La vérification lit les paquets
  (`dpkg-deb -c`, `rpm -qpl`), elle ne relit pas la configuration — `deb` et
  `rpm` étant deux déclarations distinctes, n'en vérifier qu'une laisserait
  l'autre se tromper en silence.

### Ce qui reste supposé

Il n'y a pas d'USB derrière un coureur GitHub. Restent à confirmer sur une
machine Linux munie du clavier :

- que `send_feature_report` aboutisse par hidraw sur ce périphérique — l'API est
  la même, le chemin noyau ne l'est pas ;
- que `interface_number` distingue bien les interfaces du composite. Le code
  choisit son périphérique là-dessus (§ `Keyboard::open`), et ouvrir la mauvaise
  interface donne, sous Windows, un handle valide sur lequel toute écriture
  échoue. La dorsale hidraw lit l'attribut `bInterfaceNumber` du parent USB, ce
  qui devrait donner la même valeur — *devrait* ;
- que `app_data_dir()` et `app_config_dir()` tombent bien dans
  `~/.local/share/com.oorabona.candeo` et `~/.config/com.oorabona.candeo`. C'est
  ce que documente Tauri, et le code ne construit aucun chemin lui-même (§3),
  mais rien ici ne l'a observé.

### La règle udev, et qui la livre

`/dev/hidraw*` est créé en `0600 root:root`. La règle est dans le dépôt à
[`packaging/linux/60-candeo.rules`](../../packaging/linux/60-candeo.rules) :

```udev
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="1532", MODE="0660", TAG+="uaccess"
```

Trois points qui ne sont pas des détails :

1. **`uaccess` plutôt qu'un groupe.** systemd-logind pose une ACL pour
   l'utilisateur de la session locale active et la retire à la déconnexion. Un
   groupe fixe (`plugdev`) donnerait l'accès en permanence, y compris à une
   session distante.
2. **Le préfixe 60.** La règle qui applique l'ACL est `70-uaccess.rules` : un
   fichier numéroté au-dessus de 70 poserait le marqueur trop tard et ne ferait
   rien.
3. **`/usr/lib/udev/rules.d/`, pas `/etc/`.** Le paquet est un fournisseur ;
   `/etc/udev/rules.d/` appartient à l'administrateur, qui doit pouvoir nous
   contredire. C'est aussi là qu'il faut copier le fichier à la main quand on
   lance candeo depuis les sources.

La livraison passe par `bundle.linux.deb.files` et `bundle.linux.rpm.files` de
`tauri.conf.json`. La clé est le chemin **dans le paquet**, la valeur le chemin
de la source **relatif à `tauri.conf.json`** — pas l'inverse.

### Dorsales `*-native` : évaluées, non adoptées

L'intention était de retirer la dépendance à un compilateur C pour simplifier la
compilation croisée. La lecture du `build.rs` d'`hidapi` 2.6.7 ne la soutient
pas :

- **Le gain est partiel.** `linux-static-hidraw` fait
  `pkg_config::probe_library("libudev")` ; `linux-native` s'appuie sur la crate
  `udev`, c'est-à-dire une liaison vers cette même `libudev`. Passer à la
  dorsale native ne retire donc pas `libudev-dev` de la liste des dépendances,
  seulement l'appel à `cc`. Seule `linux-native-basic-udev` s'en affranchirait,
  via `basic-udev` — une crate en 0.1.
- **Le compilateur C reste requis de toute façon.** `rquickjs` compile les
  sources C de QuickJS ; `cc` est dans `Cargo.lock` pour cette raison seule. La
  dépendance qu'on voulait supprimer ne partirait pas.
- **La compilation croisée n'est pas simplifiée pour l'application.** Sous
  Linux, Tauri se lie à webkit2gtk, GTK 3 et libsoup par `pkg-config` : il faut
  déjà un sysroot complet. Le gain ne concernerait qu'un usage sans interface de
  `candeo-device` seul.
- **Le coût n'est pas nul.** Le `build.rs` d'`hidapi` s'arrête si deux dorsales
  Linux sont actives (« Exactly one linux hidapi backend must be selected »).
  Adopter `linux-native` impose donc `default-features = false` dans les deux
  manifestes qui déclarent `hidapi`, et de réénumérer à la main les défauts des
  autres systèmes.
- **`windows-native` est hors de question pour l'instant.** Windows est le seul
  système où le protocole a été validé sur le matériel. Changer sa dorsale
  échangerait un chemin vérifié contre un chemin non vérifié, pour un gain nul :
  MSVC est déjà là, le toolchain Rust l'exige.

**Décision : on garde les dorsales par défaut.** La CI prouve que le défaut
compile sous Linux. À rouvrir si l'on veut un binaire statique sans interface —
c'est le seul cas où le calcul changerait.

---

## 7. Adoption appareil par appareil

Il fallait cliquer « Connecter » à chaque lancement, et ne pas le faire ne
produisait **aucun** signe : le simulateur s'animait, la case « envoyer au
clavier » restait cochée, le clavier gardait son image. Ce silence a coûté une
session entière de diagnostic — on a soupçonné le protocole, la cadence, le
moteur, avant de trouver que rien n'était ouvert.

La réponse n'est pas d'ouvrir tout ce qu'on détecte.

### Pourquoi pas simplement tout connecter

Écrire sur un périphérique USB qu'on comprend mal n'est pas anodin, et à
l'échelle d'un catalogue qui grandit — claviers, souris, mémoire, ventilateurs —
adopter par défaut est la façon de casser le matériel de quelqu'un. Il y a aussi
les appareils qu'on ne *veut* pas voir pilotés : un pilote constructeur déjà en
place, ou un modèle dont le relevé est incertain.

### Trois états, décidés une fois et retenus

| État | Au lancement |
|---|---|
| `adopted` — piloté | ouvert automatiquement, sans rien demander |
| `detected` — détecté | listé, mais **pas** ouvert |
| `ignored` — ignoré | laissé tranquille, et il le reste |

Le défaut est `detected` : la cérémonie disparaît sans que rien ne soit pris en
main sans accord. La forme exacte dans `settings.json` est dans
[`../api/commands.md`](../api/commands.md).

### L'identité tient à VID / PID / série, à rien d'autre

Le même clavier s'est déclaré `v1.4 / Unkown Variant` puis `v1.5 / Quartz`
pendant le relevé du protocole. Une liaison qui apparie sur la variante ou le
micrologiciel se rompt donc à la mise à jour, et l'appareil adopté redevient un
inconnu — ce qui est exactement la cérémonie qu'on vient de supprimer.

La série n'est comparée que si **les deux côtés** en portent une : elle départage
deux exemplaires du même modèle, mais une énumération muette — hidraw sans règle
udev (§6) — ne doit pas désapparier un appareil déjà adopté.

### L'échec d'un appareil n'en entraîne aucun autre

C'est l'invariant du démarrage, et il est vérifié plutôt que supposé. La boucle
d'ouverture ne connaît ni Tauri ni HID : la présence et l'ouverture lui arrivent
en argument, ce qui rend l'invariant testable par un test ordinaire.
`un_appareil_en_echec_n_en_bloque_aucun_autre` fait échouer l'ouverture du
premier de deux appareils pilotés et vérifie trois choses :

1. la boucle est allée jusqu'au second, qui est bien ouvert ;
2. le message d'échec est resté sur le premier ;
3. le compte rendu du second est vierge — l'erreur n'a pas débordé.

En exploitation, ces messages vivent dans une table indexée par VID/PID, et
`list_devices` rend à chacun le sien. Un champ unique obligerait à choisir lequel
afficher, et le suivant effacerait le précédent.

**L'invariant vaut aussi en marche, pas seulement à l'ouverture.** Un appareil
qu'on a réussi à ouvrir peut très bien refuser toute écriture ensuite — débranché,
mis en veille, préempté par un pilote constructeur. `un_appareil_en_panne_n_en_affecte_aucun_autre`
lance deux boucles réelles, fait échouer toutes les écritures de l'une, et vérifie
que l'autre garde sa boucle, ses images et son état vierge — et qu'arrêter la
première n'arrête pas la seconde. Voir §4.

### Ce que cela suppose de l'état

`AppState` porte une **table** d'appareils ouverts, indexée comme l'adoption les
identifie (issue #26). L'adoption est multiple, la poignée ouverte l'est aussi :
tous les appareils pilotés et présents sont ouverts au démarrage, chacun avec sa
boucle et son effet.

Refermer un appareil — ignoré, débranché — vide sa poignée sans la retirer de la
table. La boucle qui l'alimentait en tient une copie : elle s'en aperçoit à
l'image suivante, cesse d'écrire, et le dit par `reachingKeyboard`. C'est ce qui
permet d'arrêter d'écrire sur un appareil sans arrêter l'effet qui tourne dessus.

---

## 8. Reste à faire

- [x] Commandes `install_effect`, `list_effects`, `delete_effect`
- [x] Commandes `start_effect`, `stop_effect`, `set_effect_params`
- [x] Commande d'abonnement renvoyant les images par `Channel`
- [x] Fil de rendu `rquickjs` + module interne `@candeo/effects-api`
- [x] Lecture et écriture de `settings.json`
- [x] Effets intégrés, écrits contre l'API publique
- [x] Repère de couleurs prélevé sur le rendu, à l'installation
- [x] Règle udev, livrée par les paquets `deb` et `rpm`
- [x] Compilation et empaquetage Linux vérifiés en intégration continue
- [x] Adoption appareil par appareil, et ouverture des pilotés au démarrage
- [x] Un effet par appareil : table d'appareils ouverts, une boucle chacun
- [x] Retrait d'un effet depuis la bibliothèque, et remise à zéro de la
      configuration (`reset_settings`) — deux gestes distincts, l'un sur le
      contenu et l'autre sur la configuration, qui arrêtent l'un comme l'autre
      les boucles concernées **avant** d'écrire
- [x] Bornes d'exécution d'un effet : un temps de calcul par image et au
      chargement, une mémoire par effet — un `while (true)` ou un tableau qui
      grandit à chaque image deviennent une erreur d'image, pas un gel
- [ ] Reprise de l'effet actif au démarrage
- [ ] Vérification de la dorsale `hidraw` **sur matériel** — écriture de rapport
      de fonctionnalité, filtrage par `interface_number`, chemins résolus par
      Tauri. Demande un clavier branché sur une machine Linux.

### Ce que l'implémentation a précisé

- **Le manifeste est relevé dans le code, pas en exécutant l'effet.** Le front
  n'exécute jamais de code utilisateur : `name`, `description` et `params` sont
  lus dans l'arbre syntaxique par le compilateur que Monaco embarque déjà. Ces
  trois champs doivent donc être des littéraux, ce qui est refusé à la
  validation plutôt que découvert à la première image.
- **Un effet exporte par défaut.** La colle importe l'espace de noms plutôt que
  l'export par défaut : `import effect from 'effect'` échoue à la *liaison* du
  module quand il manque, avec un message de QuickJS qu'on ne peut relier à
  aucune ligne de son propre code.
- **Chaque image repart du noir.** Un effet qui n'écrit qu'une partie du clavier
  n'hérite pas en silence de l'image précédente : une image est complète par
  définition.
- **Les couleurs sont bornées côté JavaScript**, pas seulement dans `rgb()` :
  rien n'oblige un effet à passer par l'API, il peut fabriquer `{r, g, b}` à la
  main. Sans cela, c'est la conversion côté Rust qui échoue — loin de la cause.
- **La boucle vise une échéance absolue**, pas `sleep(période)` : une image
  lente ne doit pas décaler toutes les suivantes. En cas de retard, on repart de
  maintenant plutôt que de rattraper en accéléré.
- **Une exception ne tue rien.** Elle est rattrapée par image et exposée par
  `engine_status`, puis effacée dès que l'effet se rétablit. Après trente images
  consécutives en échec, la boucle s'arrête.
- **Le repère de couleurs vient du moteur, pas du manifeste.** Quatre images à
  des instants irrégulièrement espacés, et dans chacune la moyenne d'une bande
  diagonale qui avance : c'est ce qui empêche un effet spatial et un effet
  uniforme de se ressembler. Un prélèvement toujours au même endroit les
  confondrait, une moyenne de l'image entière aussi.
- **Tout ce qui exécute du code d'effet est borné**, en temps comme en mémoire.
  C'était d'abord vrai du seul échantillonnage, parce qu'un `while (true)` y
  empêchait une installation d'aboutir. Ça l'est maintenant de la boucle de
  rendu, où l'absence de borne était pire : le drapeau `stop` est lu *entre* deux
  images, un rendu qui ne revient pas ne le relit jamais — et comme un effet
  tourne fenêtre fermée, la fermer ne sauve pas. Depuis que la croix replie au
  lieu de quitter, elle le sauve encore moins : le dernier recours est
  « Quitter candeo » dans le menu de l'icône, qui prend le processus entier.
- **La forme des deux bornes n'est pas la même**, parce que le gestionnaire
  d'interruption se pose sur le `Runtime` **une fois**. L'échantillonnage n'a
  besoin que d'une échéance, capturée par valeur ; la boucle en change à chaque
  image et partage donc une cellule qu'elle renouvelle avant chaque appel. Cette
  cellule porte aussi de quoi savoir que c'est bien l'échéance qui a coupé :
  QuickJS lève « InternalError: interrupted » quelle qu'en soit la raison.
- **Un dépassement est une erreur d'image ordinaire.** Il emprunte le chemin des
  exceptions ci-dessus, celui que trente images consécutives en échec
  transforment en arrêt propre — aucun chemin nouveau. Le moteur y ajoute
  seulement le nom de la cause, en français : « l'effet a dépassé son temps de
  calcul » se relie à son code, une erreur de QuickJS non.
- **Les budgets sont mesurés.** Un effet ordinaire coûte 0,23 ms par image en
  `release`, un champ de cinq mille particules avec une seconde de traînée
  1,1 ms. Le budget de 10 ms n'est donc pas une allocation de performance mais
  **un détecteur de gel avec de la marge pour l'à-coup machine** : l'échéance se
  mesure en temps réel, pas en temps de calcul, et un fil suspendu par
  l'ordonnanceur consomme son budget sans rien exécuter. Le plafond vient
  d'ailleurs : l'écriture HID prend 14,4 ms au pire dans la même période de
  33,3 ms, et l'échéance ne serait ratée en silence qu'à partir d'environ 19 ms
  de budget. La mémoire ne dépend pas du profil : 32 Mo, soit cinq fois l'effet
  à état le plus démesuré qu'on sache écrire pour 132 LED.
- **Le budget de débogage est distinct — 200 ms — et c'est un plafond, pas une
  cible.** QuickJS est du C compilé au niveau d'optimisation du profil : non
  optimisé, la même image passait de 0,23 ms à 6,3 ms. Depuis que
  `[profile.dev.package."*"]` passe les dépendances en `opt-level = 2`, QuickJS
  est optimisé en débogage aussi et l'écart devrait avoir fondu — **à
  re-mesurer** avant d'unifier les deux valeurs.
- **Un appareil a une ligne d'état dès qu'il a porté un effet, et la garde.**
  `engine_status()` rend une entrée par appareil visé, `running` à faux une fois
  l'effet arrêté. Retirer la ligne rendrait « cet appareil ne fait rien »
  indistinguable de « je ne sais rien de cet appareil ».
- **L'arrêt attend la fin, par appareil.** Le verrou attendu est celui de
  l'appareil visé, jamais celui du moteur : une écriture HID bloquée sur l'un ne
  doit pas retenir les commandes visant les autres, sans quoi l'invariant de §7
  serait perdu au niveau au-dessus.

### Le seul essai qui traverse toute la chaîne

Tout le reste se vérifie sans matériel : les rapports, la matrice, le moteur,
les effets intégrés. Reste qu'aucun de ces tests ne prouve qu'un octet atteint
le clavier.

```
cargo test -p candeo-desktop bout_en_bout -- --ignored --nocapture
```

`bout_en_bout_sur_le_vrai_clavier` ouvre le périphérique, fait tourner un effet
intégré trois secondes sur **cet appareil**, vérifie que la boucle tient, qu'aucune
image n'a levé et que les images atteignent bien le clavier (`reachingKeyboard`),
puis s'arrête. Marqué `#[ignore]` : il exige un clavier branché, il n'a donc rien
à faire en intégration continue.

**Il écrit vraiment sur le clavier** — c'est le but, et c'est visible.
