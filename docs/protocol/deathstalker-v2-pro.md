# Protocole d'éclairage — Razer DeathStalker V2 Pro (filaire)

**Relevé des 11–12/09/2026 · validé en écriture directe**

> **Origine des informations.** Établi **uniquement par observation du matériel** :
> énumération PnP Windows, interrogation d'un serveur SDK par son protocole réseau,
> capture du bus USB (USBPcap 1.5.4.0 + Wireshark 4.6.8), puis **écriture directe**
> via `HidD_SetFeature`.
>
> **Aucun code source tiers n'a été consulté.** Ni OpenRGB, ni openrazer, ni les
> greffons SignalRGB.
>
> Les faits relatifs à un protocole ne relèvent pas du droit d'auteur, et leur relevé
> aux fins d'interopérabilité est prévu par l'**article L.122-6-1 IV du Code de la
> propriété intellectuelle** (directive 2009/24/CE, article 6). Ce document et son
> implémentation ne sont **pas** une œuvre dérivée d'OpenRGB.

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

---

## 3. Structure du rapport (90 octets)

```
 offset  taille  contenu
 ------  ------  -----------------------------------------------------
   0       1     status                      observé : 0x00
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
| `0x02` | `0x06` | définir l'effet |
| `0x03` | `0x47` | écrire une rangée de couleurs |
| `0x04` | `0x03` | définir la luminosité |

### `0x0f` / `0x04` — luminosité

```
args = 00 00 <niveau>
```

Observé systématiquement à `00 00 ff`. Émis avant et après chaque changement d'effet.

### `0x0f` / `0x02` — effet

```
args = 00 00 <effet> <param1> <param2> 00
```

| Effet | Valeur | Paramètres |
|---|---|---|
| Off | `0x00` | — |
| Spectrum Cycle | `0x03` | — |
| Wave | `0x04` | `param1` direction (obs. `02`), `param2` vitesse (obs. `0x28`) |
| **Direct / custom** | `0x08` | — |

> **Correction d'une lecture initiale.** La septième trame de chaque cycle de mise à
> jour n'est pas une commande de validation : c'est `0x02` avec effet `0x08`, donc le
> **passage en mode custom**, émis après l'envoi des rangées.

> **Non élucidé** : les bascules vers `Static` et `Breathing` n'ont produit aucune
> trame `0x02` pendant la capture. Hypothèse — ces effets exigent une couleur associée
> et la commande n'est émise que lorsqu'une couleur est fournie.

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
| 2 | `Static` | non capturé | firmware |
| 3 | `Breathing` | non capturé | firmware |
| 4 | `Spectrum Cycle` | `0x03` | firmware |
| 5 | `Wave` | `0x04` + direction + vitesse | firmware |

Les effets firmware **survivent à l'extinction du logiciel hôte** et ne coûtent aucun
temps processeur. Le mode `Direct` impose une poussée continue d'images — c'est le
coût d'un moteur d'effets logiciel, et la raison pour laquelle un effet utilisateur
doit pouvoir tourner sans interface.

---

## 8. Reproduire le relevé

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

## 9. Reste à établir

- [ ] Commandes exactes pour `Static` et `Breathing`
- [ ] Signification des arguments 0 et 1 (offsets 8 et 9), constants à `0x00`
- [ ] L'identifiant de transaction (`0x9f`) est-il vérifié par l'appareil ?
- [ ] Plage et effet réels des paramètres direction et vitesse de `Wave`
- [ ] Débit maximal accepté avant décrochage
- [ ] Contenu des réponses du périphérique (`GET_REPORT`)
- [ ] L'appareil accepte-t-il un rapport plus court que 90 octets ?

---

## 10. Journal

| Date | Événement |
|---|---|
| 2026-09-11 | Identification matérielle, relevé des 6 modes via un SDK tiers |
| 2026-09-11 | Installation de Wireshark 4.6.8 et USBPcap 1.5.4.0 |
| 2026-09-12 | **Première capture.** Transport, structure, ordre RGB, indexation des rangées, somme de contrôle |
| 2026-09-12 | Matrice 6×22 = 132 confirmée ; correction d'un envoi à 106 laissant la rangée 5 figée |
| 2026-09-12 | **Jeu de commandes complet** : `0x02` effet, `0x03` rangée, `0x04` luminosité |
| 2026-09-12 | **Validation en écriture directe** via `HidD_SetFeature`, sans logiciel tiers. Tampon de 91 octets et écriture partielle de rangée confirmés |
| 2026-09-12 | Clarification 132 / 106 et correspondance index → touche |

## 11. Captures

| Fichier | Contenu |
|---|---|
| `deathstalker-*.pcap` | référence : rouge / vert / bleu / noir / blanc |
| `fix132-*.pcap` | validation de l'envoi complet à 132 positions |
| `modes2-*.pcap` | bascules d'effet : Spectrum, Wave, Off, Direct |
