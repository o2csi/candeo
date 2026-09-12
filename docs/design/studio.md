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

### Le simulateur est permanent

Une bascule « envoyer au clavier » sépare l'aperçu de l'écriture réelle. Itérer
sur un effet ne doit pas exiger de regarder le vrai clavier, ni d'en posséder un.

Le rendu du simulateur utilise le **dessin physique** des touches (voir §4), pas
la grille logique : un effet spatial ne se juge pas sur une grille régulière.

---

## 3. « Commit » : pourquoi compiler, et pourquoi pas

La question posée était : TypeScript à l'exécution tiendra-t-il la charge ?

**Oui, largement.** 132 LED × 60 images/s = **7 920 couleurs par seconde**. C'est
trivial pour n'importe quel moteur JavaScript, et le franchissement IPC par image
l'est tout autant. La performance n'est donc **pas** le motif.

Le vrai motif est ailleurs : **un effet doit tourner fenêtre fermée.** Tant que
le rendu vit dans le WebView, fermer la fenêtre éteint le clavier. Il faut donc
que l'effet, une fois validé, quitte le front et vive côté Rust.

### Chaîne retenue

```
éditeur Monaco  ──esbuild-wasm──▶  JavaScript  ──rquickjs──▶  boucle Rust
   TypeScript                        (module)                 (sans interface)
```

- **`esbuild-wasm`** transpile dans le front, à la validation. Pas de service de
  compilation, pas de dépendance réseau.
- **`rquickjs`** exécute le JavaScript côté Rust. Léger, embarquable, isolable.

**AssemblyScript / WASM écarté** : ce n'est pas TypeScript mais un sous-ensemble,
et l'utilisateur découvre les limites trop tard — après avoir écrit son effet.
Le coût d'apprentissage n'est pas compensé par un gain de performance dont on n'a
pas besoin.

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
- [ ] Simulateur — tracé ISO pleine taille
- [ ] Validation : `esbuild-wasm` → stockage de l'effet
- [ ] Exécution `rquickjs` côté Rust, indépendante de la fenêtre
