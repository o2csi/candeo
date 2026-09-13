# Protocole d'éclairage — Razer DeathStalker V2 Pro (filaire)

**Relevé des 11–12/09/2026 · validé en écriture directe**

> **Origine des informations.** Établi par observation du matériel : énumération PnP
> Windows, interrogation d'un serveur SDK par son protocole réseau, capture du bus USB
> (USBPcap 1.5.4.0 + Wireshark 4.6.8), puis **écriture et lecture directes** via
> `HidD_SetFeature` / `HidD_GetFeature`.
>
> **Tout ce qui suit vaut pour le micrologiciel v1.5**, relevé les 11 et 12/09/2026.
> Le journal du §11 date chaque fait. Ce qui n'a pas pu être vérifié sur l'appareil
> est marqué **non vérifié**, et le reste au §10.
>
> Les faits relatifs à un protocole ne relèvent pas du droit d'auteur, et leur relevé
> aux fins d'interopérabilité est prévu par l'**article L.122-6-1 IV du Code de la
> propriété intellectuelle** (directive 2009/24/CE, article 6).

---

## 1. Identification

| Élément | Valeur |
|---|---|
| Fabricant | Razer — `VID 0x1532` |
| Produit | DeathStalker V2 Pro filaire — `PID 0x0292` |
| Numéro de série | *(masqué)* |
| Firmware | `v1.5` |
| Variante déclarée | `Razer Device, French (ISO), Quartz` |

> ⚠️ **Ne jamais identifier le périphérique sur sa variante ni son firmware.** Le même
> clavier se déclarait `v1.4 / Unkown Variant` en 2024. Une liaison appariant sur ces
> champs se rompt à la mise à jour — c'est ce qui a cassé un profil d'effets pendant
> le relevé. L'identité, c'est **VID / PID / numéro de série**.

### Interfaces USB

Composite à cinq interfaces. L'éclairage passe par **`MI_03`**, ce que confirme le
champ `wIndex = 3` de chaque requête.

| Interface | Rôle |
|---|---|
| `MI_00` | entrée clavier + contrôles consommateur |
| `MI_01` | collections HID multiples |
| `MI_02` | souris HID |
| **`MI_03`** | **éclairage** |
| `MI_04` | entrée HID supplémentaire |

> Sur un composite, ouvrir la mauvaise interface donne un handle **valide** sur lequel
> toute écriture échoue — sans erreur explicite. Filtrer sur `interface_number`.

### L'entrée `interface -1` — ce n'est pas le clavier

L'énumération HID porte, sur les mêmes VID et PID, une entrée **sans numéro
d'interface** (`-1`), sans nom de produit, de révision `0x0101` là où tout le
composite déclare `0x0200`, en page d'usage `0x000c`/`0x0001` (contrôle
consommateur).

Établi le 13/09/2026 par l'arbre des périphériques Windows : son chemin est
`HID#VID_1532&PID_0292&MI_00&Col03&Col01#9&…`, et son **parent** est
`RZVIRTUAL\VID_1532&PID_0292&MI_00&Col03`, un nœud créé par le service
`RzDev_0292` du pilote du fabricant — lui-même enfant de l'interface USB `MI_01`.
Ce n'est donc pas une interface USB, d'où l'absence de numéro que `hidapi` puisse
lire : c'est une collection HID **virtuelle**, qui n'existe que là où ce pilote est
installé.

candeo l'écarte par la règle qui vaut déjà pour toutes les autres : l'interface
doit être celle du gabarit (`Layout::is_lighting_interface`).

---

## 2. Transport

Transfert **de contrôle** USB, `SET_REPORT` sur rapport de **fonctionnalité**.

| Champ du setup | Valeur |
|---|---|
| `bmRequestType` | `0x21` — hôte→périphérique, classe, destinataire interface |
| `bRequest` | `0x09` — `SET_REPORT` |
| `wValue` | `0x0300` — ReportID 0, ReportType Feature (3) |
| `wIndex` | `0x0003` — interface 3 |
| `wLength` | `90` |

### Depuis l'API HID de Windows

`HidD_SetFeature` attend un tampon de **91 octets** : l'identifiant de rapport (`0x00`)
puis les 90 octets du rapport. Vérifié — 90 seuls sont refusés.

```
buf[0]      = 0x00        identifiant de rapport HID
buf[1..91]  = rapport     les 90 octets décrits ci-dessous
```

**Sens retour** : `GET_REPORT` — `bRequest 0x01`, `bmRequestType 0xa1`
(périphérique→hôte), reste du setup identique. Relevé par l'API et non par
capture : `hid_get_feature_report` (hidapi), `HidD_GetFeature` sous Windows,
même tampon de 91 octets. Ce que la réponse contient est décrit au §8.

---

## 3. Structure du rapport (90 octets)

```
 offset  taille  contenu
 ------  ------  -----------------------------------------------------
   0       1     ÉTAT                        0x00 en écriture ; porte le sens
                                             dans les RÉPONSES — voir §8
   1       1     identifiant de transaction  observé : 0x9f
   2       2     paquets restants            observé : 0x0000
   4       1     type de protocole           observé : 0x00
   5       1     TAILLE DES ARGUMENTS        varie selon la commande
   6       1     CLASSE                      0x0f = éclairage
   7       1     COMMANDE                    voir §4
   8      N      arguments
  ...       -    remplissage à 0x00
  88       1     SOMME DE CONTRÔLE           XOR des octets 2 à 87
  89       1     réservé                     0x00
```

### Somme de contrôle

**XOR des octets 2 à 87 inclus**, placé en octet 88. Vérifié sur l'intégralité des
trames capturées, toutes commandes confondues, sans exception.

```rust
let crc = report[2..88].iter().fold(0u8, |acc, b| acc ^ b);
```

---

## 4. Jeu de commandes — classe `0x0f`

| Commande | Taille args | Rôle |
|---|---|---|
| `0x02` | `0x06`–`0x09` | définir l'effet |
| `0x03` | `0x47` | écrire une rangée de couleurs |
| `0x04` | `0x03` | définir la luminosité |
| `0x80` | `0x03` | **lire** un descripteur — contient `06 16`, soit nos 6×22 |
| `0x81` | `0x03` | **lire** une énumération `00`…`09`, sens non établi |
| `0x82` | `0x03` | **lire l'effet courant** — voir §8 |
| `0x84` | `0x03` | **lire la luminosité** |
| `0x86` | `0x03` | **lire** `00 01`, sens non établi |

Les deux premiers octets d'arguments valent `00 00` dans toutes nos captures. Un
pilote tiers les nomme *variable storage* et *identifiant de LED* ; nous ne
l'avons **pas vérifié** — et les réponses en lecture commencent souvent par `05`,
ce qui irait dans le sens d'un identifiant de LED « rétroéclairage ». À établir.

### `0x0f` / `0x04` — luminosité

```
args = 00 00 <niveau>
```

Observé systématiquement à `00 00 ff`. Émis avant et après chaque changement d'effet.

### `0x0f` / `0x02` — effet

```
args = 00 00 <effet> <param1> <param2> 00
```

| Effet | Valeur | Taille | Paramètres | Pris en charge |
|---|---|---|---|---|
| Off | `0x00` | `0x06` | — | ✅ |
| **Statique** | `0x01` | `0x09` | `args[5]=01`, puis R G B | ✅ |
| **Respiration** | `0x02` | `0x09` | `args[5]=01`, puis R G B | ✅ |
| Spectrum Cycle | `0x03` | `0x06` | — | ✅ |
| Wave | `0x04` | `0x06` | `param1` direction (`00`–`02`), `param2` vitesse (obs. `0x28`) | ✅ |
| Réactif | `0x05` | `0x09` | — | ❌ **refusé** |
| Étoilé | `0x07` | `0x06`+ | — | ❌ **refusé** |
| **Direct / custom** | `0x08` | `0x06` | — | ✅ |

**La colonne « pris en charge » est mesurée, pas déduite** : on pose l'effet, puis
on le relit par `0x0f`/`0x82` (§8). Les identifiants `0x05` et `0x07`, présents sur
d'autres appareils de la marque, laissent l'effet **inchangé** sur celui-ci —
la Vague posée juste avant restait relue à l'identique, paramètres compris.
`0x06` n'a pas été essayé.

> ⚠️ **Et l'écriture de ces deux effets refusés est pourtant « acceptée » : état
> `0x02`.** C'est la démonstration en direct du danger décrit au §8 — l'appareil
> valide le couple classe/commande, **pas la valeur d'un argument**. Un identifiant
> d'effet est un argument. Aucun octet d'état ne remplacera donc une relecture.

> **Correction d'une lecture initiale.** La septième trame de chaque cycle de mise à
> jour n'est pas une commande de validation : c'est `0x02` avec effet `0x08`, donc le
> **passage en mode custom**, émis après l'envoi des rangées.

> **Résolu.** `Static` et `Breathing` n'avaient produit aucune trame pendant la
> capture, et un premier balayage les avait manqués — il les posait **sans
> couleur**, donc en noir, ce qui ne se distingue pas d'un effet inexistant à
> l'œil. Avec `args[5]=01` suivi d'un triplet RGB, les deux répondent et se
> relisent.

### `0x0f` / `0x03` — écriture d'une rangée

```
args = 00 00 <rangée> <col_début> <col_fin>   puis (col_fin - col_début + 1) × (R, G, B)
```

`0x47` = 71 = **5 octets d'arguments + 66 de couleur** pour une rangée complète (22 × 3).

**L'écriture partielle fonctionne** — vérifié sur le matériel : écrire les colonnes 5
à 10 de la rangée 2 n'affecte que ces six touches. Utile pour les effets localisés,
qui évitent ainsi de réémettre toute la matrice.

#### Ordre des composantes : **RGB**

| Couleur envoyée | Octets observés |
|---|---|
| Rouge pur | `ff 00 00` |
| Vert pur | `00 ff 00` |
| Bleu pur | `00 00 ff` |

> À ne pas confondre avec le **SDK Chroma**, dont l'API REST utilise `0x00BBGGRR`.
> Supposer l'un depuis l'autre est une erreur.

---

## 5. Séquence d'une mise à jour complète

| # | Commande | Contenu |
|---|---|---|
| 1 → 6 | `0f` / `03` | rangées 0 à 5, colonnes 0→21 |
| 7 | `0f` / `02` | effet `0x08` — passage en mode custom |

Un `0f`/`04` (luminosité) encadre généralement la séquence.

### Ce que cette séquence coûte — mesuré le 12/09/2026

**13,1 ms en moyenne, 14,4 ms au pire**, sur 120 mises à jour enchaînées au plus
vite, **sans une seule écriture refusée** et l'appareil toujours répondant après
la rafale. Soit un plafond d'environ **76 images par seconde**.

C'est le **goulot d'étranglement de toute la chaîne**, et il est sur le bus, pas
dans le calcul :

| Cadence | Période | Part prise par l'écriture | Reste pour l'effet |
|---|---|---|---|
| 60 img/s | 16,7 ms | **78 %** | ~3,6 ms |
| 30 img/s | 33,3 ms | **39 %** | ~20 ms |

> ⚠️ **60 img/s ne tenait qu'en apparence.** L'écriture seule mangeait plus des
> trois quarts de la période, laissant à l'effet moins que le budget de calcul
> qu'on lui accorde — donc un effet **parfaitement dans les clous** faisait déjà
> rater l'échéance, et la boucle retombait en silence à une cadence qu'elle
> n'annonçait nulle part. La cadence du moteur a été ramenée à **30**.

Deux réserves connues, si la cadence devait remonter :

- **on réécrit les 6 rangées à chaque image**, sans regarder ce qui a changé,
  alors que l'écriture partielle est vérifiée sur le matériel (§4) ;
- **la 7e trame est réémise à chaque image** alors qu'on est déjà en mode
  custom — à elle seule ~1,9 ms sur les 13.

### Trame réelle — rangée 0 entièrement rouge

```
00 9f 00 00 00 47 0f 03 00 00 00 00 15
ff 00 00  ff 00 00  ff 00 00  ff 00 00  ff 00 00  ff 00 00
ff 00 00  ff 00 00  ff 00 00  ff 00 00  ff 00 00  ff 00 00
ff 00 00  ff 00 00  ff 00 00  ff 00 00  ff 00 00  ff 00 00
ff 00 00  ff 00 00  ff 00 00  ff 00 00
00 00 00 00 00 00 00 00 00
5e 00
```

`0x5e` = XOR des octets 2 à 87. C'est la valeur attendue par le test
`checksum_matches_captured_frame` de `candeo-protocol`.

---

## 6. Matrice — 132 et 106 ne sont pas la même chose

**6 rangées × 22 colonnes = 132 cases**, dont **106 portent une LED de touche**.

| Chiffre | Signification |
|---|---|
| **132** | cases de la matrice, et ce que la zone déclare. **Taille d'une image.** |
| **106** | cases portant une touche physique |

> **Le piège.** Une image doit couvrir les **132** positions. En envoyer moins laisse
> les dernières rangées figées sur leur valeur précédente — symptôme observé pendant
> le relevé : la rangée du bas restée blanche pendant que le reste changeait de couleur.

> Un relevé antérieur annonçait 107 positions occupées : artefact de comptage, la
> valeur sentinelle `0xFFFFFFFF` ayant été comptée comme un index distinct.

```
rangée 0 :   0  --   2   3   4   5   6   7   8   9  10  11  12  13  14  15  16  --  --  --  --  --   (16)
rangée 1 :  22  23  24  25  26  27  28  29  30  31  32  33  34  35  36  37  38  39  40  41  42  --   (21)
rangée 2 :  44  45  46  47  48  49  50  51  52  53  54  55  56  57  58  59  60  61  62  63  64  --   (21)
rangée 3 :  66  67  68  69  70  71  72  73  74  75  76  77  78  79  --  --  --  83  84  85  --  --   (17)
rangée 4 :  88  89  90  91  92  93  94  95  96  97  98  99  -- 101  -- 103  -- 105 106 107 108  --   (18)
rangée 5 : 110 111 112  --  --  -- 116  --  --  -- 120 121 122 123 124 125 126  -- 128 129  --  --   (13)
```

### Correspondance index → touche

Reconstituée en croisant la matrice avec la liste ordonnée des noms que le périphérique
déclare. Les comptes par rangée tombent juste : 16 + 21 + 21 + 17 + 18 + 13 = **106**.

| Rangée | Touches |
|---|---|
| 0 | Échap, F1→F12, ImprÉcran, ArrêtDéfil, Pause |
| 1 | rangée chiffres, Retour arrière, Inser/Origine/PgPréc, VerrNum, `/ * −` |
| 2 | Tab, rangée haute, Suppr/Fin/PgSuiv, pavé 7 8 9 + |
| 3 | VerrMaj, rangée repos, Entrée, pavé 4 5 6 |
| 4 | Maj gauche, touche ISO, rangée basse, Maj droite, ↑, pavé 1 2 3, Entrée pavé |
| 5 | Ctrl/Win/Alt, Espace, AltGr/Fn/Menu/Ctrl, ← ↓ →, pavé 0 . |

> **L'Entrée ISO porte deux LED** : index **57** (rangée 2) et **79** (rangée 3). Un
> dégradé vertical y est visible — c'est le matériel, pas un défaut de rendu.
>
> **La barre d'espace n'en porte qu'une** : index **116**, en `(5, 6)`, malgré ses
> 6,25 unités de large.

### Géométrie physique

**Le périphérique ne la déclare pas.** Seule la grille logique 6 × 22 est disponible.
Le dessin réaliste utilisé par l'interface est écrit à la main depuis la disposition
ISO pleine taille standard — voir `docs/design/studio.md`.

L'expansion index par index de la table ci-dessus, et le rectangle de chaque touche,
vivent dans `crates/candeo-device/src/layout.rs`. **Les deux n'ont pas le même statut** :
les noms sont un relevé, la géométrie une convention. Seuls les premiers se vérifient
contre l'appareil.

---

## 7. Modes exposés

| Index SDK | Nom | Effet protocole | Animé par |
|---|---|---|---|
| 0 | `Direct` | `0x08` | **l'hôte** |
| 1 | `Off` | `0x00` | — |
| 2 | `Static` | `0x01` + RGB | firmware |
| 3 | `Breathing` | `0x02` + RGB | firmware |
| 4 | `Spectrum Cycle` | `0x03` | firmware |
| 5 | `Wave` | `0x04` + direction + vitesse | firmware |

Les six modes du SDK correspondent donc exactement aux six identifiants que
l'appareil accepte — ni plus (`0x05` et `0x07` sont refusés), ni moins. La
concordance vaut confirmation croisée des deux relevés.

Les effets firmware **survivent à l'extinction du logiciel hôte** et ne coûtent aucun
temps processeur. Le mode `Direct` impose une poussée continue d'images — c'est le
coût d'un moteur d'effets logiciel, et la raison pour laquelle un effet utilisateur
doit pouvoir tourner sans interface.

---

## 8. Lire l'appareil — `GET_REPORT` et la classe `0x00`

**Le périphérique répond.** C'est ce qui manquait pour distinguer une écriture
*acceptée* d'une écriture *comprise* — la seule chose qui sépare aujourd'hui un
clavier qui obéit d'un clavier qui jette nos trames en silence.

### Transport de lecture

`HidD_GetFeature` sous Windows, `hid_get_feature_report` via hidapi. **Même
interface MI_03, même tampon de 91 octets** que l'écriture. On écrit la commande,
puis on relit : la réponse réutilise la structure du §3, avec deux différences
utiles — l'octet 0 porte un **état**, et les octets 6 et 7 **renvoient en écho**
la classe et la commande, ce qui permet de vérifier qu'on lit bien la réponse
qu'on attend et non la précédente.

### L'octet d'état (offset 0)

| Valeur | Sens |
|---|---|
| `0x00` | aucune |
| `0x01` | occupé |
| `0x02` | **compris** |
| `0x03` | échec |
| `0x04` | expiré |
| `0x05` | **non pris en charge** |

**Vérifié qu'il discrimine réellement**, plutôt que supposé : une classe
inexistante (`0xee`) et une commande inexistante sur une classe valide
(`0x0f`/`0xee`) rendent toutes deux `0x05`, là où une commande valide rend
`0x02`.

⚠️ **Limite mesurée** : une taille d'arguments aberrante sur une commande valide
rend quand même `0x02`. L'appareil valide le **couple classe/commande**, pas la
cohérence de ses arguments. Un contrôle de compatibilité ne peut donc affirmer
que « cette commande existe », jamais « mes arguments sont bons ».

### Classe `0x00` — informations

Relevé le 12/09/2026 sur notre exemplaire, micrologiciel v1.5.

| Commande | Réponse | Sens | Établi par |
|---|---|---|---|
| `0x81` | `01 05` | **version du micrologiciel — 1.05** | concordance avec la version déclarée par ailleurs |
| `0x82` | *(masqué)* | **numéro de série** (15 car. ASCII) | format, et stabilité entre lectures |
| `0x83` | `01 25` | **inconnu** | inconnu d'OpenRazer également |
| `0x84` | `00 00` | **mode de l'appareil** — `0x00` normal, `0x03` pilote | voir ci-dessous |
| `0x85` | `01 00` | **fréquence d'interrogation** — `01`=1000 Hz, `02`=500, `08`=125 | |
| `0x86` | `04 80` | **disposition nationale** — `04` = `fr_FR` | vérifié : notre `layout.rs` est bien AZERTY |
| `0x87` | `01 05` | **inconnu** | inconnu d'OpenRazer, « valeurs de retour variables » |
| `0x80`, `0x88`–`0x8f` | — | `0x05` non pris en charge | |

La valeur rendue par `0x82` est exactement celle du §1 — c'est **la** source du
numéro de série, et la seule.

⚠️ **Le descripteur USB ne porte aucun numéro de série** (`serial_number()` est
vide sur les quatre interfaces) — seule cette commande en donne un. Et
`release_number` vaut `0x0200` sur tout le composite alors que le micrologiciel
est en v1.5 : **le `bcdDevice` est une révision matérielle, pas une version de
micrologiciel.** Ne pas les confondre.

### Classe `0x0f` — relire l'éclairage

C'est la partie la plus utile du sens retour : **elle permet de vérifier un effet
sans dépendre de l'œil**, ce qui manquait cruellement au premier balayage des
identifiants.

| Commande | Réponse observée | Sens |
|---|---|---|
| `0x82` | `00 00 <effet> <p1> <p2>` | **effet courant**, paramètres compris |
| `0x84` | `00 00 ff` | **luminosité courante** |
| `0x80` | `05 19 03 06 16` | descripteur — `06 16` = **6 rangées × 22 colonnes**, notre matrice |
| `0x81` | `05 00 01 02 03 04 05 06 07 08 09` | énumération de 10 valeurs, **sens non établi** |
| `0x86` | `00 01` | non établi |

⚠️ **`0x81` n'est PAS la liste des effets pris en charge**, même si elle en a
l'air : elle contient `05` et `07`, que l'appareil refuse en pratique. C'est
exactement le genre de coïncidence qu'il faut tester au lieu de conclure.

**Méthode de vérification d'un effet** : poser l'effet, attendre ~150 ms, relire
par `0x82`. Si l'identifiant relu diffère de celui posé, l'appareil a **ignoré**
la commande — quand bien même l'écriture aurait rendu `0x02`.

### Le mode de l'appareil — et pourquoi candeo n'y touche pas

`0x00`/`0x84` rend `0x00`, soit **mode normal**, et notre éclairage custom
fonctionne parfaitement ainsi. OpenRazer, lui, bascule les appareils en **mode
pilote** (`0x03`, via `0x00`/`0x04`) à l'initialisation de son démon.

La raison est documentée chez eux, et elle explique pourquoi **nous ne devons pas
l'imiter** : en mode pilote, le micrologiciel **cesse de traiter certaines
touches lui-même** et se contente d'émettre des évènements HID que l'hôte est
censé reprendre. Si personne n'écoute, ces touches ne font plus rien — cas
constaté sur un Basilisk V3, dont le cycle DPI et le verrou de molette sont
devenus inertes, corrigé en repassant en mode normal.

OpenRazer est un **pilote complet** : il gère les touches macro, le DPI, les
profils, donc il a besoin que le micrologiciel lui cède la main. **candeo ne
pilote que l'éclairage.** Basculer en mode pilote ne nous apporterait rien et
casserait des touches que l'appareil gère très bien seul.

> **Décision : ne jamais écrire `0x00`/`0x04`.** À porter comme mise en garde
> explicite dans le SDK d'appareils (#34) — c'est typiquement l'étape qu'un
> contributeur recopierait d'un pilote existant sans voir ce qu'elle coûte.

---

## 9. Reproduire le relevé

```powershell
# 1. Repérer le hub portant le clavier
tshark -D
tshark -i \\.\USBPcap1 -a duration:6 -w test.pcap
tshark -r test.pcap -Y 'usb.idVendor == 0x1532' -T fields -e usb.device_address

# 2. Capturer en poussant des couleurs pures espacées dans le temps
tshark -i \\.\USBPcap1 -a duration:30 -w capture.pcap

# 3. Isoler les transferts de contrôle sortants
tshark -r capture.pcap -Y 'usb.device_address == 9 && usb.transfer_type == 0x02 && usb.endpoint_address == 0x00' -T fields -e frame.number

# 4. Extraire les 90 octets
tshark -r capture.pcap -Y 'frame.number == 113' -V | Select-String 'Data Fragment:'
```

**Le point de méthode qui débloque tout** : envoyer des **couleurs pures et uniformes**,
bien séparées dans le temps. `ff 00 00` répété 22 fois saute aux yeux dans un vidage
hexadécimal, et la position des octets donne l'ordre des composantes sans le déduire.

### Pièges d'outillage

- USBPcap n'attache son filtre aux hubs qu'**après redémarrage**.
- `USBPcapCMD --extcap-interfaces` peut ne rien renvoyer, même en élévation. Passer
  directement par `tshark -i \\.\USBPcapN`, qui fonctionne.
- Les numéros de bus USB changent d'un démarrage à l'autre.

---

## 10. Reste à établir

- [x] **Contenu des réponses du périphérique (`GET_REPORT`)** — §8
- [x] **Lire la version du micrologiciel** — `0x00`/`0x81`, §8
- [x] **Obtenir un numéro de série** — `0x00`/`0x82`, le descripteur USB n'en porte aucun
- [x] **Commandes exactes pour `Static` et `Breathing`** — `0x01` et `0x02`, taille `0x09`, `args[5]=01` puis RGB, §4
- [x] **Relire l'effet courant** — `0x0f`/`0x82`, ce qui permet de vérifier sans l'œil
- [x] **Quels identifiants d'effet l'appareil accepte** — les six du SDK ; `0x05` et `0x07` sont refusés
- [ ] Signification des arguments 0 et 1 (offsets 8 et 9), constants à `0x00` — un pilote tiers les nomme *variable storage* et *identifiant de LED*, non vérifié
- [ ] L'identifiant de transaction (`0x9f`) est-il vérifié par l'appareil ?
- [ ] Plage réelle de la vitesse de `Wave` ; la direction est bornée à `00`–`02`
- [ ] Identifiant d'effet `0x06` : jamais essayé
- [x] **Débit maximal accepté avant décrochage** — voir ci-dessous
- [ ] L'appareil accepte-t-il un rapport plus court que 90 octets ?
- [ ] Sens de `0x0f`/`0x81` (énumération `00`…`09`) et `0x0f`/`0x86` (`00 01`)
- [ ] Les trois premiers octets de `0x0f`/`0x80` (`05 19 03`), dont les deux suivants donnent bien 6×22
- [ ] Sens de `0x00`/`0x83` (`01 25`) et `0x00`/`0x87` (`01 05`) — inconnus d'OpenRazer aussi
- [ ] Second octet de la disposition, `0x86` → `04 80` : que vaut `0x80` ?
- [x] **Une entrée HID fantôme `interface -1`** — collection virtuelle du pilote du fabricant (`RZVIRTUAL`), pas le clavier ; écartée par le filtre d'interface, §1
- [ ] La relecture de `Statique` et `Respiration` par `0x0f`/`0x82` rend-elle la couleur, et à quelle position ? Tant que non établi, l'inspection à l'ouverture ne les réécrit pas
- [ ] **Réécrire à l'identique l'effet et la luminosité courants est-il invisible ?** C'est l'hypothèse qui autorise l'inspection à émettre à chaque ouverture — à confirmer par `sonde_inspection_a_l_ouverture`, application fermée

---

## 11. Journal

| Date | Événement |
|---|---|
| 2026-09-11 | Identification matérielle, relevé des 6 modes via un SDK tiers |
| 2026-09-11 | Installation de Wireshark 4.6.8 et USBPcap 1.5.4.0 |
| 2026-09-12 | **Première capture.** Transport, structure, ordre RGB, indexation des rangées, somme de contrôle |
| 2026-09-12 | Matrice 6×22 = 132 confirmée ; correction d'un envoi à 106 laissant la rangée 5 figée |
| 2026-09-12 | **Jeu de commandes complet** : `0x02` effet, `0x03` rangée, `0x04` luminosité |
| 2026-09-12 | **Validation en écriture directe** via `HidD_SetFeature`, sans logiciel tiers. Tampon de 91 octets et écriture partielle de rangée confirmés |
| 2026-09-12 | Clarification 132 / 106 et correspondance index → touche |
| 2026-09-12 | **Le périphérique répond.** `GET_REPORT` relevé : octet d'état, écho classe/commande, et vérification qu'un `0x05` distingue bien une commande inconnue d'une commande valide |
| 2026-09-12 | **Classe `0x00` relevée** : micrologiciel (`0x81`), numéro de série (`0x82`), mode (`0x84`), fréquence d'interrogation (`0x85`), disposition nationale (`0x86`). `0x83` et `0x87` restent inconnus |
| 2026-09-12 | Établi que `release_number` (`bcdDevice`, `0x0200`) **n'est pas** la version du micrologiciel (v1.5), et que le descripteur USB ne porte aucun numéro de série |
| 2026-09-12 | **Décision : ne jamais basculer en mode pilote.** Il ferait cesser au micrologiciel le traitement de certaines touches, sans contrepartie pour un contrôleur d'éclairage |
| 2026-09-12 | **`0x0f`/`0x82` relit l'effet courant** — vérification d'un effet sans dépendre de l'œil. `Static` (`0x01`) et `Breathing` (`0x02`) enfin établis : ils exigent une couleur, et le premier balayage les posait en noir |
| 2026-09-12 | **Les identifiants `0x05` et `0x07` sont refusés** par cet appareil, alors que l'écriture rend `0x02`. Démonstration en direct qu'un octet d'état ne valide pas les arguments |
| 2026-09-12 | **Débit mesuré** : 13,1 ms par mise à jour complète, plafond ~76 img/s, aucune écriture refusée. Le goulot est le bus, pas le calcul — la cadence du moteur passe de 60 à **30 img/s** |
| 2026-09-13 | **L'entrée `interface -1` élucidée** par l'arbre des périphériques : collection HID virtuelle sous `RZVIRTUAL`, service `RzDev_0292` du pilote du fabricant — pas le clavier |

## 12. Captures

| Fichier | Contenu |
|---|---|
| `deathstalker-*.pcap` | référence : rouge / vert / bleu / noir / blanc |
| `fix132-*.pcap` | validation de l'envoi complet à 132 positions |
| `modes2-*.pcap` | bascules d'effet : Spectrum, Wave, Off, Direct |
