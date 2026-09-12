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
- [ ] Commandes `start_effect`, `stop_effect`, `set_effect_params`
- [ ] Commande d'abonnement renvoyant les images par `Channel`
- [ ] Fil de rendu `rquickjs` + module interne `@candeo/effects-api`
- [x] Lecture et écriture de `settings.json`
- [ ] Reprise de l'effet actif au démarrage
- [ ] Règle udev et vérification de la dorsale `hidraw` sous Linux
