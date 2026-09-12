# SDK d'appareils — contribuer un appareil qu'on est seul à posséder

Ce document prépare [#34]. Il ne décrit pas ce qui existe : il tranche le
**vocabulaire** par lequel un appareil contribué dira ce qu'il sait faire, et les
règles qui rendent une contribution **relisable par quelqu'un qui ne possède pas
le matériel**.

Il est écrit pour une personne précise : celle qui a un appareil que nous n'avons
pas, qui veut le faire marcher, et qui devra convaincre un relecteur incapable de
vérifier quoi que ce soit par lui-même.

> **Ce qui n'est pas décidé ici.** Le format de fichier d'un gabarit, les octets
> d'un protocole autre que celui du DeathStalker, et l'implémentation. Les blocs
> de code ci-dessous montrent la **forme** d'une déclaration, pas une syntaxe
> arrêtée.

---

## 1. Trois choses, dont deux sont déjà des données

Un appareil se réduit à une **identité** (VID/PID, interface du composite), un
**gabarit** (positions adressables, noms, géométrie) et une façon de **construire
ses rapports**. C'est l'analyse de [#34], et elle tient.

Ce que ce document ajoute est qu'il en manque une quatrième, aujourd'hui absente
et invisible : **ce que l'appareil sait faire**. `Layout` porte `name`, `vid`,
`pid`, `interface`, `rows`, `cols`, `matrix`, `keys` — de quoi ouvrir le bon
périphérique et lui pousser une image, rien qui permette de répondre à « cet
effet a-t-il un sens ici ». Tant qu'il n'y a qu'un appareil, la question ne se
pose pas : la réponse est oui, toujours. Elle se pose au deuxième.

### Pourquoi une capacité et pas un modèle

La tentation est de laisser un effet nommer les appareils qu'il vise. C'est le
mauvais axe, et [#44] §5 en donne la raison définitive :

> Une liste de modèles est un **monde clos**. Elle ne peut rien savoir du
> matériel sorti après elle, et son auteur ne peut de toute façon pas l'essayer.
> Une exigence de **capacité** est ouverte : elle se confronte à n'importe quel
> gabarit, y compris ceux qui n'existent pas encore.

La conséquence porte sur le SDK autant que sur les effets. **Contribuer un
appareil, c'est déclarer ce qu'il sait faire** — après quoi un effet écrit
aujourd'hui fonctionne sur un appareil ajouté dans deux ans sans que personne ne
rouvre sa liste, et un appareil ayant une particularité peut l'exposer sans qu'un
effet ait à coder un modèle en dur.

⚠️ **Ce vocabulaire est une interface publique.** Un effet exporté le contient en
toutes lettres ; le changer après coup casse des fichiers qui ne sont plus chez
nous. C'est la raison d'être des trois critères d'admission du §3.

---

## 2. La nature de l'appareil

Le gabarit porte une **nature** — `keyboard`, `mouse`, `mousepad`, … — et chaque
effet **déclare obligatoirement** celles qu'il vise. L'obligation est tranchée
dans [#44] §5 et n'est pas rediscutée ici :

> Un champ facultatif produit du silence, pas une réponse : personne ne le
> remplit, et « absent » finit par vouloir dire « je n'y ai pas pensé » autant
> que « ça marche partout ».

### Pourquoi une nature, puisqu'il y a des capacités

Parce que les deux répondent à des questions différentes, et qu'aucune ne
remplace l'autre :

| | Question | Qui décide |
|---|---|---|
| **Capacité** | Est-ce que ça **fonctionne** ici ? | la mécanique |
| **Nature** | Est-ce que ça **a un sens** ici ? | l'auteur de l'effet |

« Balayage » fonctionnerait très bien sur un tapis de souris muni d'une grille :
aucune capacité ne le refuse. Son auteur peut malgré tout savoir que sa traînée
suppose des rangées de touches et n'a pas l'air de grand-chose ailleurs. La
nature est le seul endroit où il peut le dire.

Elle sert aussi à ce que le vocabulaire de capacités ne doit **surtout pas**
avoir à exprimer : de quoi l'objet a l'air. Le simulateur dessine un clavier
parce que c'en est un ([`studio.md`](studio.md) §4) ; il n'y a pas de capacité
« ressemble à un clavier », et il ne faut pas essayer d'en écrire une.

### La liste est close, et ce n'est pas un monde clos

Ajouter une nature demande une ligne dans une liste du dépôt, donc une PR — la
même que celle qui apporte l'appareil. C'est volontaire : `mouse` et `souris` et
`pointing-device` écrits librement par trois contributeurs donneraient trois
natures qu'aucun effet ne peut viser ensemble.

> **Ce n'est pas la fermeture que [#44] dénonce.** Une liste de *modèles* se
> périme parce que le matériel change ; une liste de *natures* ne se périme pas
> parce que les catégories d'objets bougent d'un cran par décennie. Une souris
> sortie demain se déclare `mouse` et tous les effets « souris » l'atteignent
> sans qu'une ligne change. C'est exactement l'inverse d'une liste de VID/PID.

### « Toutes natures » s'écrit

```ts
kinds: 'all',          // un choix, pas un oubli
kinds: ['keyboard'],
kinds: ['mouse', 'mousepad'],
```

---

## 3. Le vocabulaire de capacités

### 3.1 Trois critères d'admission

Un terme n'entre au vocabulaire que s'il passe les trois. C'est ce qui empêche
les deux dérives — un vocabulaire trop fin, impossible à remplir, et un
vocabulaire trop grossier, auquel personne ne peut se fier.

1. **Un effet en dépend.** Il existe, ou on peut nommer précisément, un effet qui
   change de comportement — qui tourne ou refuse — selon la valeur du terme. Sans
   ça le terme est du remplissage : il coûte une décision à chaque contributeur
   et ne sert à personne.
2. **Un contributeur peut répondre depuis son seul relevé.** Sans posséder un
   autre appareil, sans lire le code d'un pilote tiers, sans deviner. Un terme
   auquel on ne peut répondre est un terme auquel on répondra **faux**.
3. **Son absence a un sens sûr.** Un gabarit écrit avant l'ajout du terme doit
   rester correct. Sans cette règle le vocabulaire ne peut plus grandir, et c'est
   la moitié de son intérêt qui disparaît.

### 3.2 Les six termes

| Terme | Ce qu'il dit | L'effet qui en dépend |
|---|---|---|
| `directFrame` | l'hôte peut poser une image | **tous** |
| `color` | `rgb` ou `mono` | tout ce qui parle de teinte |
| `matrix` | les positions forment une grille, le voisinage a un sens | Balayage, Onde radiale |
| `geometry` | chaque position a une place physique | une onde vraiment radiale |
| `namedKeys` | chaque position porte le nom de sa touche | surligner un raccourci |
| `zones` | des parties nommées hors grille | une pulsation de molette |

---

**`directFrame` — l'hôte peut poser une image.**

C'est le seul terme dont tout dépend, et le seul dont on pourrait croire qu'il
est inutile puisque tous les effets l'exigent. Il gagne sa place par le cas
contraire : un appareil dont le micrologiciel n'expose que ses propres effets
existe, et mérite un gabarit — on veut le nommer, le lister, lui poser sa
respiration matérielle. Ce qu'on ne veut pas, c'est y démarrer un effet, animer
le simulateur, et laisser l'utilisateur regarder un appareil éteint. C'est
précisément le silence qui a coûté une session entière de diagnostic
([`effects-runtime.md`](effects-runtime.md) §7) ; il ne doit pas revenir par la
porte d'un appareil contribué.

**Rempli par** : le contributeur a trouvé le mode piloté par l'hôte, ou ne l'a
pas trouvé. Sur le DeathStalker c'est l'effet `0x08` ([`deathstalker-v2-pro.md`](../protocol/deathstalker-v2-pro.md) §4).

---

**`color` — `rgb` ou `mono`.**

Le seul terme du lot qui ne se lit nulle part dans les données : un gabarit
monochrome a la même matrice, les mêmes noms, la même géométrie. Il faut donc le
déclarer, et c'est le seul.

Un rétroéclairage blanc ou à couleur unique n'a rien d'exotique, et un effet dont
le sujet **est** la teinte — Onde radiale, qui fait tourner `hsv` — n'y produit
pas un résultat dégradé : il produit une surface de luminosité à peu près
constante, c'est-à-dire rien. Mieux vaut que la galerie le dise.

⚠️ **Absent vaut `rgb`**, parce que c'est ce que sont tous les gabarits
existants. La règle générale est au §3.4 ; ce terme en est l'illustration.

---

**`matrix` — les positions forment une grille.**

Le terme le plus lourd de conséquences, parce qu'il est **déjà supposé partout et
déclaré nulle part**. Les trois effets livrés lisent `key.row` :

```js
const k = Math.max(0, 1 - Math.abs(key.row - head) / trail)   // balayage.js
const d = Math.hypot(key.col - cx, key.row - cy)              // onde-radiale.js
```

Sur un appareil sans grille, `key.row` vaudrait `undefined`, la soustraction
`NaN`, et la couleur serait bornée à zéro : **les trois effets livrés
s'afficheraient en noir, sans une erreur.** C'est la démonstration que
l'exigence doit être écrite, et qu'elle doit être écrite **maintenant**, tant
que les effets concernés sont tous chez nous.

Une exigence quantifiée est permise ici — « au moins trois rangées », « au moins
dix colonnes » pour un effet qui dessine une jauge — sous une règle stricte :

> **Un seuil ne porte que sur un nombre que le gabarit déclare déjà pour une
> autre raison.** `rows` et `cols` sont dans `Layout` depuis le début. Autoriser
> les seuils sur autre chose ferait entrer par la bande des grandeurs que
> personne ne sait mesurer.

---

**`geometry` — chaque position a une place physique.**

Ce que le périphérique **ne déclare pas** : il n'expose que sa grille logique. Le
rectangle de chaque touche est une transcription à la main de la disposition ISO,
et `layout.rs` le dit déjà sans pouvoir en tirer de conséquence.

L'effet qui en dépend n'existe pas encore, et c'est justement ce qui rend le
terme utile. « Onde radiale » se dit radiale mais calcule sa distance en
`(rangée, colonne)` : sur un clavier pleine taille, le pavé numérique est à
quatre colonnes du centre alors qu'il en est physiquement à l'autre bout, et les
trous de la matrice comptent comme de la distance. L'onde est donc ronde dans une
grille et déformée sur le bureau. Une version qui exigerait `geometry` serait
ronde pour de bon — et refuserait proprement les gabarits qui n'ont pas été
dessinés.

**Rempli par** : le contributeur a dessiné sa disposition, ou non. C'est du
travail réel et facultatif ; ne pas le faire reste une contribution valable, qui
perd seulement les effets spatiaux.

C'est aussi ce dont le **simulateur** a besoin pour ressembler à l'appareil. Un
gabarit sans géométrie n'est pas indessinable, il est dessiné en grille de
carrés — ce que [`studio.md`](studio.md) §4 avait écarté pour l'appareil que nous
avons, et qui redevient le moindre mal pour un appareil que nous n'avons pas.

---

**`namedKeys` — chaque position porte un nom.**

La correspondance index → touche du DeathStalker a demandé de croiser la matrice
avec la liste ordonnée que le périphérique déclare (§6 du relevé). Ce n'est ni
automatique ni donné, et un contributeur peut légitimement s'arrêter avant.

L'effet qui en dépend est celui qui éclaire une **touche par son nom** plutôt que
par sa position : surligner les touches d'un raccourci, allumer `WASD`, faire
respirer la seule barre d'espace. Ces effets sont impossibles à écrire
aujourd'hui de manière portable, et le terme est ce qui les rend possibles sans
coder un modèle en dur.

⚠️ **Un nom n'est pas une identité.** Le DeathStalker relevé est en `fr_FR` ISO :
ses noms sont `A`, `Z`, `ù`. Un effet qui cherche `W` sur un AZERTY le trouvera
là où un QWERTY le met ailleurs. Le terme dit « il y a des noms », il ne promet
aucune disposition ; ce que porte la disposition relève de la provenance (§5).

---

**`zones` — des parties nommées hors grille.**

C'est le terme qui ouvre le SDK aux appareils qui ne sont pas des claviers, et
c'est celui qui demande le plus de précautions, parce qu'un nom libre n'est
utilisable par personne.

> **Les rôles viennent d'une liste close** — `main`, `logo`, `wheel`, `strip`,
> `underglow`, `wrist` — étendue par PR, comme les natures. Un effet qui exige
> `wheel` a besoin que `wheel` veuille dire la même chose partout ; si le champ
> est libre, `molette`, `wheel` et `scroll` coexistent et l'effet ne trouve
> jamais rien. L'étiquette affichée, elle, est du texte libre.

C'est ici que l'argument du monde ouvert se voit le mieux. Une pulsation de
molette écrite aujourd'hui pour une souris fonctionnera sur un **clavier** de
2028 muni d'une molette éclairée, sans qu'une ligne de l'effet change — parce
qu'elle n'a jamais parlé de souris, seulement de molette.

⚠️ **Un booléen par organe est la dérive à éviter.** `hasWheel`, `hasLogo`,
`hasUnderglow`… est un vocabulaire infini que personne ne peut remplir ni tenir à
jour. Une zone est une **position adressable qui porte un rôle** : rien de plus,
et c'est ce qui la rend descriptible.

### 3.3 Ce qui n'entre pas au vocabulaire, et pourquoi

Écarter est aussi une décision, et c'est celle qui tient le vocabulaire petit.

| Écarté | Raison |
|---|---|
| **écriture partielle de rangée** | vraie sur le DeathStalker, et **invisible d'un effet** : un effet écrit une image, il n'adresse jamais le bus. C'est une propriété de transport, pas une capacité. |
| **luminosité matérielle** | l'hôte et l'interface en dépendent, aucun effet n'en dépend : une image porte déjà ses couleurs. |
| **effets du micrologiciel** | énumérer `Static`, `Breathing`, `Wave`… par modèle recréerait un monde clos d'un autre genre. C'est du ressort de l'interface, pas des effets. |
| **cadence soutenable** | c'est une **mesure**, pas une capacité — voir ci-dessous. |
| **nombre de LED** | dérivé, et surtout piégeux : 132 positions pour 106 touches, et confondre les deux fige une rangée. Aucun effet ne doit brancher dessus. |
| **VID / PID** | l'objet même du refus de [#44] §5. |

**Le cas de la cadence mérite son paragraphe.** Une mise à jour complète du
DeathStalker coûte 13,1 ms, soit un plafond de ~76 images par seconde, et c'est
ce qui a fait retomber le moteur de 60 à 30 (§5 du relevé). Un appareil contribué
plus lent existera. Il faut donc bien que le gabarit porte ce chiffre — mais
comme **mesure datée**, à côté de la provenance, pas comme capacité : l'hôte s'en
sert pour régler sa boucle et pour avertir, et aucun effet n'a à s'en soucier. Un
effet qui exigerait « au moins 30 images par seconde » ne serait pas refusé
utilement ; il serait refusé bêtement, alors que ralenti il reste un effet.

### 3.4 Comment le vocabulaire grandit sans rien casser

Deux règles, et elles n'ont pas le même prix.

**Côté gabarit — un terme ajouté doit avoir un sens sûr quand il est absent.**
`namedKeys` absent veut dire « pas de noms » : sûr. `color` absent veut dire
`rgb`, parce que c'est ce que sont tous les gabarits écrits avant le terme —
le défaut sûr n'est pas « rien », c'est **ce que l'existant supposait déjà**. Un
terme sans défaut sûr ne peut pas être ajouté après coup, seulement à une rupture
de version. C'est la contrainte qui doit être vérifiée **avant** d'ajouter un mot,
pas découverte après.

**Côté effet — un terme inconnu se refuse avec une phrase.** Un effet venu d'une
version plus récente peut exiger un terme que l'hôte ne connaît pas. Il est
refusé, et il est refusé par une phrase qui nomme le terme, comme `apiVersion` en
[#44] §4 — jamais par une erreur de moteur que personne ne relie à son code.

### 3.5 Le gabarit ne remplit presque rien : les capacités se **lisent**

C'est le point qui fait tenir tout le reste, et c'est la réponse à « un
vocabulaire trop fin devient impossible à remplir ».

**On ne déclare que ce qui ne se déduit pas.** Un contributeur ne remplit pas un
questionnaire de capacités : il fournit les données qu'il a relevées, et le
vocabulaire en est la lecture.

| Terme | D'où il vient |
|---|---|
| `directFrame` | le pilote implémente « poser une image », ou non |
| `matrix` | le gabarit porte une grille |
| `geometry` | les positions portent un rectangle |
| `namedKeys` | toutes les positions portent un nom non vide |
| `zones` | le gabarit porte des zones |
| `color` | **déclaré** — rien dans les données ne le révèle |

Un contributeur ne peut donc pas se tromper sur cinq termes sur six : il peut
seulement en offrir moins, en fournissant moins. Et l'incitation va dans le bon
sens — dessiner sa géométrie, nommer ses touches, c'est ouvrir sa contribution à
plus d'effets. Personne n'a à arbitrer une case à cocher dont il ne comprend pas
l'enjeu.

Ce qui reste déclaré à la main est donc minuscule : la nature, la profondeur de
couleur, et la provenance du §5. Trois choses auxquelles on peut répondre sans
rien deviner.

---

## 4. Deux déclarations, et leur confrontation

### Le DeathStalker V2 Pro

Ce que deviendrait le gabarit existant. Les données sont celles de
[`layout.rs`](../../crates/candeo-device/src/layout.rs), inchangées ; tout ce qui
est nouveau tient dans les deux derniers blocs.

```rust
pub static DEATHSTALKER_V2_PRO: Layout = Layout {
    name: "Razer DeathStalker V2 Pro (filaire)",
    kind: DeviceKind::Keyboard,
    vid: 0x1532,
    pid: 0x0292,
    interface: 3,                       // MI_03 — l'éclairage, et lui seul

    grid: Some(Grid { rows: 6, cols: 22, matrix: &[/* 132 positions */] }),
    keys: &[/* 106 touches, nommées et dessinées */],
    zones: &[],                         // tout est dans la grille

    color: Color::Rgb,                  // le seul terme déclaré

    provenance: Provenance {
        firmware: "1.05",
        firmware_read_by: "classe 0x00, commande 0x81",
        surveyed_on: "2026-09-12",
        method: Method::CapturedAndVerified,
        variant: "French (ISO) — fr_FR, relevé par 0x00/0x86",
        sustained_rate: Some(Rate { frames_per_second: 76, measured_on: "2026-09-12" }),
        origin: Origin {
            grid: Source::Surveyed,     // relevé sur l'appareil
            keys_named: Source::Surveyed,
            keys_drawn: Source::Convention,   // dessiné à la main, jamais vérifié
        },
    },
};
```

Les capacités qui en découlent, sans qu'une ligne les énonce : `directFrame`,
`matrix { rows: 6, cols: 22 }`, `geometry`, `namedKeys`, `color: rgb`, aucune
zone.

### Une souris à trois zones

> ⚠️ **Ceci n'est pas un relevé.** Aucun exemplaire n'a été ouvert, aucune trame
> capturée. C'est une illustration de la **forme** d'une déclaration sur un
> appareil aussi éloigné que possible du seul que nous ayons. Les octets, les
> identifiants et la cadence d'un appareil réel ne se devinent pas.

```rust
kind: DeviceKind::Mouse,

grid: None,                             // il n'y a pas de voisinage ici
keys: &[],
zones: &[
    Zone { role: Role::Wheel,  label: "Molette" },
    Zone { role: Role::Logo,   label: "Logo" },
    Zone { role: Role::Strip,  label: "Bandeau latéral" },
],

color: Color::Rgb,
```

Une image y fait **trois** couleurs au lieu de 132. Rien d'autre ne change : le
moteur pousse toujours un tableau plat de `Rgb` dans `DeviceOut::present`, et le
trait ne bouge pas d'une ligne. C'est le meilleur argument en faveur de cette
forme — **elle n'ajoute aucun chemin de données, seulement de quoi savoir à qui
on parle.**

### Ce que ça donne à l'écran

| Effet | Exige | Clavier | Souris 3 zones |
|---|---|---|---|
| Respiration | `kinds: 'all'`, rien | ✅ | ✅ |
| Balayage | `matrix { rowsMin: 3 }` | ✅ | ❌ pas de grille |
| Onde radiale | `matrix` | ✅ | ❌ pas de grille |
| Onde radiale *physique* | `geometry` | ✅ | ❌ rien n'est dessiné |
| Surligner un raccourci | `namedKeys` | ✅ | ❌ rien n'est nommé |
| Pulsation de molette | `zones: ['wheel']` | ❌ pas de molette | ✅ |

La dernière ligne est celle qui compte : le jour où un clavier à molette
éclairée arrive, sa case passe à ✅ **sans que l'effet soit rouvert**.

### Côté effet

```ts
export default defineEffect({
  name: 'Balayage',
  kinds: ['keyboard'],
  requires: { matrix: { rowsMin: 3 } },
  render({ layout, time, frame, params }) { … },
})
```

**`requires` est obligatoire, comme `kinds`.** [#44] n'exigeait la déclaration que
pour la nature ; ce document l'étend, et pour la même raison exactement — les
trois effets livrés lisent `key.row` sans le dire, et s'afficheraient en noir sur
un appareil sans grille. Un défaut implicite ne serait pas plus sûr : il serait
seulement invisible. La migration coûte une ligne par effet, et cette ligne écrit
une vérité qui était jusqu'ici supposée.

Ne rien exiger s'écrit, comme « toutes natures » :

```ts
  requires: 'none',
```

### Une exigence tenue par l'hôte, pas par la politesse

Rien n'empêche un effet de déclarer `requires: 'none'` et de lire `key.row` quand
même. C'est la même situation que les écritures hors bornes de [#44] §4, et elle
appelle la même réponse : **c'est l'hôte qui arrête, pas la politesse de
l'auteur.**

La piste : le moteur construit l'objet `layout` remis à QuickJS **d'après ce que
l'effet a déclaré exiger**. Lire un champ non exigé devient alors une erreur
d'image nommée — « cet effet lit `row` sans exiger `matrix` » — qui emprunte le
chemin d'erreur existant, celui que trente images consécutives transforment en
arrêt propre. Le coût est de quelques milliers d'accès interceptés par seconde à
30 images, ce qui n'est rien.

Non tranché ici : c'est une piste d'implémentation, pas une décision d'interface.
Ce qui est tranché, c'est que **l'écart entre ce qui est déclaré et ce qui est lu
ne doit pas être silencieux**.

---

## 5. La provenance : contre quoi le gabarit a été établi

Un gabarit contribué **doit** dire sur quel micrologiciel il a été établi, et
quand. Sans ça, un comportement bizarre chez un tiers est indébrouillable — et le
contributeur n'aura plus son matériel sous la main pour trancher. C'est la
demande de [#35], et c'est la seule chose que ce SDK exige et qui ne serve pas à
faire marcher l'appareil : elle sert à comprendre plus tard.

### Ce qui est piégeux, et pourquoi le champ ne suffit pas seul

Le `release_number` de l'énumération HID vaut `0x0200` sur tout le composite
pendant que le micrologiciel se déclare v1.5. C'est `bcdDevice`, **une révision
matérielle figée** : `hidapi` ne donne pas ce qu'on cherche, et le seul chemin
vers la vraie version est une commande de l'appareil — `0x00`/`0x81` ici.

Un contributeur pressé recopiera `release_number`, parce qu'il est là, qu'il
ressemble à une version, et que rien ne le contredit. D'où le second champ :

> **`firmware_read_by` est aussi obligatoire que `firmware`.** « 1.05, lu par la
> classe 0x00 commande 0x81 » est vérifiable et discutable. « 2.00 », seul, ne
> l'est pas — et c'est précisément la valeur qu'on obtient en se trompant.

### Les champs, et ce que chacun sauve

| Champ | Ce qu'il permet, le jour où ça cloche |
|---|---|
| `firmware` + `firmware_read_by` | distinguer « le code est faux » de « la version a changé » |
| `surveyed_on` | dater le relevé face au journal de mises à jour du fabricant |
| `method` | savoir si les octets ont été **vus sur le bus**, **écrits et relus**, ou **déduits** d'un appareil voisin |
| `variant` | la disposition nationale : la matrice d'un ISO et d'un ANSI ne portent pas les mêmes touches |
| `sustained_rate` | régler la boucle sur une mesure plutôt que sur l'appareil que nous avons |
| `origin` | savoir, ligne par ligne, ce qui a été **relevé** et ce qui a été **dessiné** |

### `origin` est le champ le plus utile en relecture

`layout.rs` dit déjà, en prose, que les noms viennent du relevé et la géométrie
d'une convention : « les deux n'ont pas le même statut ». Aujourd'hui ce n'est
qu'un commentaire. Porté en donnée, il dit à un relecteur **ce qu'il peut
contester et ce qu'il doit croire sur parole**, et à l'interface ce qu'elle doit
afficher à côté d'un appareil dont personne ici n'a vérifié le dessin.

Trois valeurs suffisent :

- `surveyed` — vu sur l'appareil, reproductible par la méthode du §9 du relevé ;
- `convention` — construit à la main, exact au sens d'un usage ; seul l'œil le
  dément ;
- `inferred` — copié d'un appareil voisin, **jamais essayé**.

`inferred` est le plus important des trois, parce que c'est celui qu'on obtient
en contribuant le second modèle d'une gamme sans le posséder. Il doit exister
pour être écrit plutôt que caché.

### Ce que la provenance n'est pas

Une déclaration, comme l'`author` d'un effet ([#44] §1) : rien ne l'authentifie.
La différence est qu'elle est **réfutable** — une version, une date et une
méthode se confrontent à une observation ultérieure, là où un nom ne se confronte
à rien. C'est ce qui justifie de l'exiger sans prétendre la vérifier.

À la connexion, une version différente **avertit sans bloquer** ([#35]) : bloquer
rendrait l'application inutile après une mise à jour de routine, alors que le
protocole n'aura très probablement pas bougé. L'avertissement transforme une
panne muette en soupçon énoncé — et c'est tout ce qu'on peut honnêtement en
faire.

---

## 6. Le « mode pilote » : l'étape à ne pas recopier

C'est la mise en garde qui justifie à elle seule qu'un SDK existe, parce que
c'est l'étape qu'un contributeur reprendrait d'un pilote existant **sans voir ce
qu'elle coûte**, et qu'aucune relecture ne rattraperait si elle n'était pas
nommée.

**Le fait** : sur le DeathStalker, `0x00`/`0x84` rend `0x00` — l'appareil est en
**mode normal**, et notre éclairage custom fonctionne parfaitement ainsi.
OpenRazer, lui, bascule ses appareils en **mode pilote** (`0x00`/`0x04` → `0x03`)
à l'initialisation de son démon.

**Pourquoi c'est cohérent chez eux** : OpenRazer est un **pilote complet** — il
gère les touches macro, le DPI, les profils. Il a besoin que le micrologiciel lui
cède la main.

**Ce que ça coûte** : en mode pilote, le micrologiciel **cesse de traiter
certaines touches lui-même** et se contente d'émettre des évènements HID que
l'hôte est censé reprendre. Si personne n'écoute, ces touches ne font plus rien —
constaté sur un Basilisk V3 dont le cycle DPI et le verrou de molette sont
devenus inertes, et corrigé en repassant en mode normal.

> **candeo ne pilote que l'éclairage.** Basculer ne nous apporte rien et casse des
> touches que l'appareil gère très bien seul. **Ne jamais écrire `0x00`/`0x04`.**

### Ce que le SDK en fait — une règle, pas seulement un avertissement

Un paragraphe dans une documentation se recopie moins bien qu'une ligne de code.
La mise en garde doit donc être **portée par la forme du SDK** :

> **Le SDK n'expose pas « envoyer ce rapport ». Il expose des intentions** :
> poser une image, régler la luminosité, choisir un effet du micrologiciel.

Il n'y a pas d'intention nommée « changer le mode de l'appareil ». Un
contributeur qui recopie une séquence d'initialisation trouvée ailleurs n'a donc
pas d'endroit où la mettre : il devrait **sortir du SDK** pour le faire, ce qui
se voit en relecture au lieu de se fondre dans une suite d'octets.

Et l'asymétrie qui va avec :

| | Permis |
|---|---|
| **Lecture** | large — la version, le numéro de série, le mode courant, le descripteur. Lire ne casse rien, et c'est ce qui alimente la provenance du §5. |
| **Écriture** | les seules intentions d'éclairage, sur la seule interface que le gabarit déclare. |

Ce qui répond aussi à la question ouverte de [#34] — « un gabarit tiers ne doit
pas pouvoir écrire n'importe quoi sur n'importe quelle interface » : **le pilote
contribué n'ouvre rien**. L'hôte énumère, filtre sur `interface_number`, ouvre, et
lui remet un canal déjà lié à cette interface. Un pilote qui ne voit jamais
`HidApi` ne peut pas se tromper de périphérique, ni en découvrir un autre.

---

## 7. Livrer un appareil en données plutôt qu'en code

[#34] posait la question ; le vocabulaire ci-dessus y répond presque seul. Un
gabarit est déjà de la donnée pure, et les capacités s'en lisent (§3.5). Ce qui
reste en code, c'est la construction des rapports.

**La bonne unité n'est pas l'appareil, c'est la famille de protocole.** Le
DeathStalker n'a rien de particulier : il parle le protocole Razer — rapport de
fonctionnalité de 90 octets, classe `0x0f`, somme de contrôle par XOR des octets
2 à 87, une commande par rangée. Un second clavier Razer ne demanderait **aucune
ligne de code** : une famille, et des données.

Ce qui rend une famille paramétrable est exactement ce qui varie d'un modèle à
l'autre au sein d'une marque — l'interface, la matrice, les identifiants d'effet
pris en charge. Ce qui n'en varie pas — la structure du rapport, la plage de la
somme de contrôle — reste dans le code de la famille, en un seul endroit, testé
une fois contre une trame capturée.

Les deux coûts de contribution deviennent alors très différents, et c'est
souhaitable :

| Contribution | Coût |
|---|---|
| un appareil d'une famille connue | **des données** — relisable ligne à ligne |
| une nouvelle famille | du code, et une trame capturée pour l'ancrer |

⚠️ **Le piège de la famille « presque »**. Un modèle dont la somme de contrôle
couvre une autre plage, ou dont le rapport fait 64 octets, n'est pas une variante
de paramètres : c'est une autre famille. Un gabarit de données qui aurait tort
serait accepté par l'appareil, rendrait `Ok`, et n'allumerait rien — voir §8. La
provenance `method: inferred` existe pour ce cas précis.

---

## 8. Ce qu'on ne peut pas vérifier sans le matériel

C'est la question la plus inconfortable de [#34], et la seule dont la réponse
honnête est « beaucoup de choses ».

### Le socle : accepté n'est pas compris

`reachingKeyboard` prouve que l'écriture a été **acceptée**, pas qu'elle a été
**comprise**. Le relevé le démontre en direct : l'appareil valide le couple
classe/commande, **pas la valeur des arguments**. Les identifiants d'effet `0x05`
et `0x07` sont acceptés — état `0x02`, « compris » — et laissent l'effet
**inchangé**. Une taille d'arguments aberrante passe aussi.

Tout ce qui suit en découle : sur un appareil qu'on n'a pas, **aucune couche ne
signale un gabarit faux**. L'écriture aboutit, le voyant est au vert, et rien ne
s'allume.

### La liste, sans adoucissement

| Invérifiable sans le matériel | Ce que ça donne quand c'est faux |
|---|---|
| l'interface porte bien l'éclairage | handle **valide**, toute écriture perdue |
| les octets sont compris | `Ok`, appareil figé |
| l'image couvre toute la matrice | les dernières rangées restent figées — le piège 132 / 106 |
| la géométrie ressemble à l'appareil | un simulateur faux, que seul l'œil dément |
| les noms correspondent aux gravures | un effet qui allume la mauvaise touche |
| la cadence tient | des images perdues en silence |
| aucune commande ne casse autre chose | le mode pilote du §6 |

### Ce que l'intégration continue peut établir, elle

Et c'est moins maigre qu'il n'y paraît, à condition d'y mettre la bonne pièce.

**La trame capturée jointe au gabarit.** C'est déjà ce que fait
`checksum_matches_captured_frame` : une rangée réellement observée sur le bus, sa
somme de contrôle attendue, et un test qui reconstruit la trame et compare. Une
contribution en apporte au moins une. Elle ne prouve pas que l'appareil obéit —
elle prouve que **le code du dépôt reproduit ce que le contributeur a vu**, ce
qui est exactement la moitié qu'on peut tenir sans le matériel.

Le reste se vérifie par cohérence interne, et les tests existants de `layout.rs`
en donnent déjà le modèle — indices uniques, bijection entre matrice et touches,
rectangles disjoints, comptes par rangée. Généralisés à tout gabarit, ils
rattrapent la faute la plus probable d'une transcription à la main : le décalage
d'une touche.

S'y ajoute la cohérence entre données et capacités, qui devient vérifiable parce
que les capacités sont lues et non déclarées : un gabarit qui prétend `geometry`
sans rectangles n'est pas refusé à l'exécution, il est **impossible à écrire**.

### Ce que relire une contribution veut dire

La conséquence pour le relecteur est directe, et il vaut mieux l'écrire que la
laisser se découvrir :

> **On ne relit pas l'exactitude d'un relevé. On relit sa cohérence et sa
> provenance.** Dire oui à une contribution matérielle, c'est dire « ceci est
> cohérent, daté, et reproductible par quelqu'un qui aurait l'appareil » —
> jamais « ceci est juste ».

Les questions auxquelles un relecteur peut répondre :

- la trame jointe est-elle **reconstruite** par le code, somme de contrôle
  comprise ?
- la provenance est-elle complète, et `firmware_read_by` nomme-t-il autre chose
  que `release_number` ?
- `origin` distingue-t-il ce qui est relevé de ce qui est dessiné ?
- les écritures passent-elles toutes par les intentions du SDK (§6) ?
- une commande sort-elle de l'éclairage ? Si oui, **pourquoi** ?
- les capacités sont-elles bien lues des données, et non affirmées ?

Et celles auxquelles il ne peut pas répondre — donc qu'il ne doit pas feindre de
trancher : est-ce que l'appareil obéit, est-ce que le dessin lui ressemble, est-ce
que les noms sont les bons.

### Ce que l'utilisateur doit en savoir

Un appareil dont le relevé n'a été reproduit par personne d'autre que son auteur
ne doit pas se présenter comme les autres. La provenance est déjà à l'écran
([#35]) ; il suffit qu'elle dise aussi **qui a vu cet appareil fonctionner**.

Et l'adoption reste ce qu'elle est : le défaut est `detected`, jamais `adopted`.
« Écrire sur un périphérique USB qu'on comprend mal n'est pas anodin »
([`effects-runtime.md`](effects-runtime.md) §7) — c'est encore plus vrai d'un
appareil dont le gabarit vient d'ailleurs.

---

## 9. Chargement : compilé, pas greffon

Un greffon qui écrit en USB est une surface d'attaque, et [#34] demandait de
trancher plutôt que de supposer. La raison de trancher **compilé** n'est pourtant
pas d'abord la sécurité :

**Un greffon échapperait à la seule vérification que nous ayons.** Tout le §8
repose sur des tests exécutés par l'intégration continue sur le contenu du
dépôt : la trame rejouée, la cohérence de la matrice, la présence de la
provenance. Un gabarit chargé à l'exécution n'est passé par aucun de ces tests,
et personne ne l'a relu. Il ne resterait de la contribution que ce qu'elle a de
plus cher — l'écriture sur un bus — sans rien de ce qui la rend acceptable.

S'y ajoute que la PR **est** le mécanisme de chargement : elle porte la relecture,
la trame de référence, la provenance et l'historique. C'est ce qui a fait grandir
OpenRGB, et ce n'est pas un hasard.

À rouvrir si, et seulement si, le nombre de gabarits rend la compilation du
catalogue déraisonnable. Ce n'est pas un problème que nous ayons avec un
appareil, ni avec vingt.

---

## 10. Ce que ça change dans le code existant

Décrit, non écrit : l'implémentation est le corps de [#34].

- **`Layout` gagne une nature et une provenance**, et sa grille devient
  facultative — un appareil sans voisinage n'a pas de `rows` et de `cols` à
  inventer. `led_count` devient « positions de la grille + zones », et le contrat
  « une image couvre toutes les positions » ne bouge pas.
- **`Key` voit son nom et son rectangle devenir facultatifs**, ce qui est la
  condition pour que `namedKeys` et `geometry` se lisent au lieu de se déclarer.
- **`DeviceOut` ne change pas.** Le moteur reçoit déjà une sortie abstraite et un
  gabarit, jamais un `Keyboard` : c'est ce joint qui rend tout ce document
  additif.
- **L'API des effets gagne `kinds` et `requires`**, obligatoires, et son `Key`
  gagne un rôle de zone. Les trois effets livrés gagnent une ligne chacun.
- **Le manifeste relevé dans l'arbre syntaxique** ([`effects-runtime.md`](effects-runtime.md))
  doit lire ces deux champs comme il lit déjà `name` et `params` : des littéraux,
  refusés à la validation plutôt que découverts à la première image.
- **Le simulateur** doit savoir dessiner autre chose qu'un clavier ISO : une
  grille de carrés sans géométrie, des pastilles nommées pour des zones.

---

## 11. Questions ouvertes

- **La langue des identifiants.** Ce document propose `kinds`, `requires`,
  `geometry`, `wheel` — de l'anglais, pour tenir avec `name`, `params`, `render`,
  `rows`, `cols` et le reste de la surface publique. La prose reste française. À
  confirmer, parce que c'est aussi coûteux à changer plus tard que le reste du
  vocabulaire.
- **Les zones et la grille cohabitent-elles vraiment ?** Un clavier à
  sous-éclairage aurait les deux, et une image plate concaténerait les deux. Rien
  ne s'y oppose ici, rien ne l'a essayé non plus.
- **L'interception des lectures non déclarées** (§4) est une piste, pas une
  décision. Son coût réel dans QuickJS n'a pas été mesuré.
- **Une famille de protocole se décrit-elle en données ?** §7 la laisse en code.
  La frontière exacte entre « paramètre de famille » et « nouvelle famille » ne
  se tranchera qu'au deuxième protocole, et pas avant.
- **Deux exemplaires du même modèle restent indistinguables** quand le descripteur
  USB ne porte pas de numéro de série ([#35]). Le SDK n'y change rien, mais un
  catalogue de gabarits rend le cas plus fréquent.
- **Une onde radiale physique** n'existe pas encore, et c'est le seul effet qui
  justifierait `geometry` à lui seul. Tant qu'elle n'est pas écrite, le terme est
  soutenu par un argument et non par un fichier.

---

## 12. Ce que ce document ne résout pas

Le relevé reste à faire par quelqu'un qui possède l'appareil. Aucun vocabulaire
ne crée de connaissance : il évite seulement qu'elle soit perdue faute d'endroit
où la mettre, et qu'elle soit fausse faute d'avoir été datée.

[#34]: https://github.com/oorabona/candeo/issues/34
[#35]: https://github.com/oorabona/candeo/issues/35
[#44]: https://github.com/oorabona/candeo/issues/44
