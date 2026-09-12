# Conception de l'interface — validée pour la v1

**Maquette de référence** : non publiée

Ce document fige les décisions prises avant d'écrire la moindre ligne de Vue. Il
existe pour que l'implémentation ait une cible, pas pour décrire ce qui existe.

---

## 1. La galerie est l'écran d'accueil

L'application n'ouvre **pas** sur un éditeur. Elle ouvre sur la liste des effets
disponibles, et un bouton « ＋ » entre en édition.

Le motif : la plupart des lancements servent à choisir un effet, pas à en écrire
un. Faire de l'éditeur l'écran d'accueil imposerait un outil de développement à
quelqu'un qui veut juste changer de couleur.

### Trois natures d'effet, distinguées visuellement

| Nature | Origine | Coût | Survit à la fermeture |
|---|---|---|---|
| **Intégré** | Rust, livré avec l'application | boucle hôte | non |
| **À vous** | écrit dans l'éditeur, validé | boucle hôte | oui, via le service |
| **Matériel** | micrologiciel du clavier | **nul** | **oui, toujours** |

La distinction n'est pas cosmétique. Un effet matériel (`Spectrum Cycle`, `Wave`)
continue de tourner machine éteinte et ne coûte aucun temps processeur ; c'est
souvent le bon choix, et l'interface doit le dire.

---

## 2. L'éditeur est un mode, pas un écran séparé

Ouvrir « ＋ » remplace le contenu de la fenêtre : éditeur à gauche, simulateur de
clavier à droite, en permanence visible.

### Monaco, pas CodeMirror

Choisi pour son **service de langage TypeScript** : en chargeant le `.d.ts` de
`@candeo/effects-api`, on obtient types, autocomplétion et erreurs en ligne sans
rien écrire de spécifique. Le poids n'entre pas en compte — l'application est
empaquetée, il n'y a pas de téléchargement à l'usage.

Second bénéfice, découvert après coup et décisif pour le §3 : Monaco n'embarque
pas un analyseur maison mais **le compilateur TypeScript lui-même**. Son
`ts.worker.js` réexporte `typescriptServices` sous le nom `ts` — 6,6 Mo minifiés
de compilateur, déjà payés par l'éditeur. `ts.transpileModule()` est donc
disponible sans rien ajouter.

### Le simulateur est permanent

Une bascule « envoyer au clavier » sépare l'aperçu de l'écriture réelle. Itérer
sur un effet ne doit pas exiger de regarder le vrai clavier, ni d'en posséder un.

Le rendu du simulateur utilise le **dessin physique** des touches (voir §4), pas
la grille logique : un effet spatial ne se juge pas sur une grille régulière.

Mais le simulateur ne **calcule** rien : il reçoit des images déjà produites par
le moteur d'exécution, exactement celles qui partent vers le clavier. C'est tout
l'objet du §3.

---

## 3. Un seul moteur d'exécution, côté Rust

### La question de départ n'était pas la bonne

Elle était : TypeScript à l'exécution tiendra-t-il la charge ?

**Oui, largement.** 132 LED × 60 images/s = **7 920 couleurs par seconde**. C'est
trivial pour n'importe quel moteur JavaScript, et le franchissement IPC par image
l'est tout autant. La performance n'est **pas** un sujet, et ne l'a jamais été.

Deux contraintes réelles, elles, commandent tout :

1. **Un effet doit tourner fenêtre fermée.** Tant que le rendu vit dans le
   WebView, fermer la fenêtre éteint le clavier.
2. **L'aperçu doit être la production, pas sa ressemblance.**

### La version écartée, et pourquoi

La première rédaction de ce document proposait ceci :

```
Monaco (TS) ─esbuild-wasm─▶ JS ─┬─▶ exécuté dans le WebView  ─▶ simulateur
                                └─▶ IPC ─▶ rquickjs (Rust)   ─▶ clavier
```

**Deux moteurs JavaScript différents exécutent le même effet** — V8 dans le
WebView pour l'aperçu, QuickJS en production. Le simulateur ne montre alors pas
ce que fait le clavier : il montre ce qu'un autre moteur ferait du même code.
L'écart se découvre tard, sur un effet précis, et il est pénible à diagnostiquer.

Ajouter `esbuild-wasm` pour servir ce schéma, c'était payer un transpileur
supplémentaire pour obtenir un défaut.

### Ce qu'on fait à la place

```
Monaco (TS)
   │  ts.transpileModule()          ← déjà embarqué dans Monaco, rien à ajouter
   ▼
  JS ──── IPC install_effect ────▶  rquickjs (Rust)   ← moteur unique
                                          │
                         ┌────────────────┴────────────────┐
                         ▼                                 ▼
              événement « frame » → simulateur      écriture HID → clavier
```

- **La transpilation est gratuite.** `ts.transpileModule()` retire les types,
  rien de plus ; c'est une fonction de texte vers texte, déterministe, sans
  sémantique d'exécution. Peu importe donc où elle a lieu — et Monaco la fournit
  déjà. **Ni `esbuild-wasm`, ni `oxc`, ni `swc` : aucun des trois n'est requis.**
- **`rquickjs`** exécute le JavaScript côté Rust, dans un fil indépendant de la
  fenêtre. Un seul moteur, donc l'aperçu **est** la production, par construction.
- **Le front n'exécute jamais de code utilisateur.** Bénéfice de sûreté qui vient
  gratuitement : un effet ne peut toucher ni le DOM ni l'API Tauri.
- Les imports depuis `@candeo/effects-api` (`hsv`, `mix`…) sont résolus par le
  chargeur de modules de `rquickjs` vers un module interne. **Pas de bundler.**

Le simulateur coûte un événement IPC par image : 396 octets, 60 fois par seconde.
C'est le chiffre déjà jugé trivial plus haut — on ne va pas le craindre ici.

> `boa_engine` (pur Rust, sans C) serait plus simple à embarquer, mais QuickJS
> est nettement plus rapide et plus complet. Pour du code utilisateur arbitraire,
> la couverture du langage compte davantage que l'absence de FFI.

**AssemblyScript / WASM reste écarté** : ce n'est pas TypeScript mais un
sous-ensemble, et l'utilisateur en découvre les limites après avoir écrit son
effet. Le coût d'apprentissage n'est compensé par aucun gain dont on ait besoin.

---

## 4. Le clavier doit ressembler au clavier

La maquette dessine la disposition **ISO pleine taille réelle** : Entrée en L,
pavé numérique détaché, barre d'espace à 6,25 unités, flèches en T inversé.

> **Le périphérique ne déclare pas sa géométrie.** Il n'expose que la grille
> logique 6 × 22. Le dessin réaliste est donc **écrit à la main** à partir de la
> disposition ISO standard, et la correspondance case ↔ touche vient du relevé
> documenté dans [`../protocol/deathstalker-v2-pro.md`](../protocol/deathstalker-v2-pro.md).

Conséquence : un nouveau modèle de clavier demandera un dessin, pas seulement une
matrice. C'est assumé — l'alternative, une grille de carrés, rend le simulateur
inutile pour juger un effet.

---

## 5. Ce qui a été retiré

**L'affichage de l'image courante** (les 132 triplets en clair) a été supprimé de
la maquette. C'est un outil de mise au point, pas une information utile à
l'usage ; sa place est derrière un mode diagnostic, pas dans l'écran principal.

---

## 6. Choix du périphérique

Un seul modèle est pris en charge aujourd'hui, mais l'écran de sélection existe
dès la v1 : la commande `list_devices` renvoie déjà tous les gabarits connus avec
un indicateur de présence, et l'interface doit montrer un périphérique absent
comme absent, pas l'omettre.

---

## 7. Reste à faire pour la v1

- [ ] Écran galerie — vignettes animées des effets
- [ ] Écran de sélection du périphérique
- [ ] Éditeur Monaco + `.d.ts` de `@candeo/effects-api`
- [ ] Simulateur — tracé ISO pleine taille, alimenté par les images du moteur
- [ ] Validation : `ts.transpileModule()` → commande `install_effect`
- [ ] Moteur `rquickjs` côté Rust : fil de rendu indépendant de la fenêtre,
      module interne `@candeo/effects-api`, émission de l'événement « frame »

Aucune dépendance nouvelle côté Rust hors `rquickjs`, aucune côté front hors
`monaco-editor`.
