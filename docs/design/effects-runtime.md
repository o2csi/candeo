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
app_data_dir()/effects/<id>/     source.ts · effect.js · manifest.json
app_config_dir()/settings.json   effet actif, luminosité, périphérique choisi
```

L'effet est du **contenu** (`data`), le choix de l'effet actif est de la
**configuration** (`config`). Distinction sans objet sous Windows, exacte sous
Linux — et gratuite dans les deux cas.

> Les effets **intégrés** ne sont pas sur disque : ils sont compilés dans le
> binaire. Seuls les effets écrits par l'utilisateur ont un dossier.

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

## 4. Le moteur — une boucle, deux sorties

Un fil Rust, indépendant de toute fenêtre. Il instancie un contexte `rquickjs`,
charge `effect.js`, et appelle sa fonction de rendu à cadence fixe.

```
        ┌──────────────────────────┐
        │  rquickjs — render(ctx)  │  60 Hz
        └────────────┬─────────────┘
                     │  image : 132 triplets RGB
          ┌──────────┴───────────┐
          ▼                      ▼
   écriture HID            canal → simulateur
   (si connecté et         (si la fenêtre écoute)
    « envoyer » actif)
```

Les deux sorties sont **indépendantes**, et chacune peut être absente :

- fenêtre fermée → seule l'écriture HID subsiste ; aucune image n'est sérialisée ;
- bascule « envoyer au clavier » désactivée → seul le simulateur est alimenté,
  ce qui permet d'écrire un effet **sans posséder le clavier** ;
- les deux actives → l'aperçu montre exactement les octets envoyés.

C'est cette dernière propriété qui justifie le moteur unique : le simulateur
n'interprète pas le code, il affiche le résultat.

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
passé en argument d'une commande d'abonnement.

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
| Dorsale par défaut | hidapi C (`hid.dll`) | `linux-static-hidraw` (compile du C) |
| Dorsale pur Rust | `windows-native` | `linux-native` (udev + nix) |
| Accès non privilégié | immédiat | **règle udev requise** |

Sous Linux, `/dev/hidraw*` n'est pas accessible à l'utilisateur par défaut. Il
faudra livrer une règle :

```udev
# /etc/udev/rules.d/60-candeo.rules
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="1532", MODE="0660", TAG+="uaccess"
```

Les dorsales `*-native` retireraient toute dépendance à un compilateur C, ce qui
simplifierait la compilation croisée. À évaluer au premier build Linux — pas
avant : elles ne sont pas la valeur par défaut du greffon, et le défaut
fonctionne.

---

## 7. Reste à faire

- [x] Commandes `install_effect`, `list_effects`, `delete_effect`
- [x] Commandes `start_effect`, `stop_effect`, `set_effect_params`
- [x] Commande d'abonnement renvoyant les images par `Channel`
- [x] Fil de rendu `rquickjs` + module interne `@candeo/effects-api`
- [x] Lecture et écriture de `settings.json`
- [x] Effets intégrés, écrits contre l'API publique
- [ ] Reprise de l'effet actif au démarrage
- [ ] Règle udev et vérification de la dorsale `hidraw` sous Linux

### Ce que l'implémentation a précisé

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
