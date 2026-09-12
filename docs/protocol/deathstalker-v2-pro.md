# Protocole d'éclairage — Razer DeathStalker V2 Pro (filaire)

**Relevé des 11–12/09/2026**

> **Origine des informations.** Ce document ne contient que des **faits observés
> directement** sur le matériel : énumération PnP Windows, interrogation du serveur
> SDK d'OpenRGB par son protocole réseau, et **capture du bus USB** (USBPcap 1.5.4.0
> + Wireshark 4.6.8).
>
> **Aucun code source tiers n'a été lu pour le rédiger.** Ni OpenRGB, ni openrazer,
> ni les greffons SignalRGB. Les faits relatifs à un protocole ne sont pas couverts
> par le droit d'auteur, et leur relevé aux fins d'interopérabilité est prévu par
> l'**article L.122-6-1 IV du Code de la propriété intellectuelle** (transposition
> de la directive 2009/24/CE, article 6).
>
> Une réimplémentation fondée sur ce document n'est donc **pas** une œuvre dérivée
> d'OpenRGB, et n'est pas soumise à sa licence GPL-2.0-or-later.

---

## 1. Identification

| Élément | Valeur |
|---|---|
| Fabricant | Razer — `VID 0x1532` |
| Produit | DeathStalker V2 Pro filaire — `PID 0x0292` |
| Numéro de série | *(masqué)* |
| Firmware | `v1.5` |
| Variante déclarée | `Razer Device, French (ISO), Quartz` |

> ⚠️ La variante et la version **changent avec le firmware** : le même clavier se
> déclarait `v1.4 / Unkown Variant` en 2024. **Ne jamais identifier le périphérique
> sur ces champs** — utiliser VID / PID / numéro de série.

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

---

## 2. Transport

Transfert **de contrôle** USB, `SET_REPORT` sur rapport de **fonctionnalité**.

| Champ du setup | Valeur | Signification |
|---|---|---|
| `bmRequestType` | `0x21` | hôte → périphérique, classe, destinataire interface |
| `bRequest` | `0x09` | `SET_REPORT` |
| `wValue` | `0x0300` | ReportID 0, ReportType **Feature (3)** |
| `wIndex` | `0x0003` | **interface 3** |
| `wLength` | `90` | taille de la charge utile |

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
   6       1     CLASSE de commande          0x0f = éclairage
   7       1     ID de commande              voir §4
   8     N=[5]   arguments
  ...       -    remplissage à 0x00
  88       1     SOMME DE CONTRÔLE           XOR des octets 2 à 87
  89       1     réservé                     0x00
```

### Somme de contrôle

**XOR de tous les octets de l'offset 2 à 87 inclus**, placé en octet 88. Vérifié
sur l'intégralité des trames capturées, toutes commandes confondues, **sans une
seule exception**.

```python
crc = 0
for b in report[2:88]:
    crc ^= b
report[88] = crc
```

---

## 4. Jeu de commandes — classe `0x0f`

| ID | Taille args | Rôle |
|---|---|---|
| `0x02` | `0x06` | **définir l'effet** |
| `0x03` | `0x47` | **écrire une rangée de couleurs** |
| `0x04` | `0x03` | **définir la luminosité** |

### `0x0f` / `0x04` — luminosité

```
args = 00 00 <niveau>
```

Observé systématiquement à `00 00 ff` (niveau 255). Émis **avant et après** chaque
changement d'effet, et à chaque cycle de mise à jour.

### `0x0f` / `0x02` — effet

```
args = 00 00 <effet> <param1> <param2> 00
```

Identifiants d'effet relevés :

| Valeur | Effet | Paramètres |
|---|---|---|
| `0x00` | Off | — |
| `0x03` | Spectrum Cycle | — |
| `0x04` | Wave | `param1` = direction (obs. `02`), `param2` = vitesse (obs. `0x28` = 40) |
| `0x08` | **Direct / custom** | — |

> **Correction d'une lecture initiale.** La septième trame de chaque cycle de mise à
> jour, que j'avais prise pour une commande de validation, est en réalité
> `0x0f`/`0x02` avec effet `0x08` : c'est le **passage en mode custom**, émis après
> l'envoi des rangées. Ce n'est pas un « commit ».

> **Non élucidé** : les bascules vers `Static` et `Breathing` n'ont produit **aucune**
> trame `0x02` pendant la capture, contrairement aux quatre autres modes. Hypothèse à
> vérifier : ces deux effets exigent une couleur associée, et la commande n'est émise
> que lorsqu'une couleur est effectivement fournie.

### `0x0f` / `0x03` — écriture d'une rangée

```
args = 00 00 <rangée> <col_début> <col_fin>   puis 22 × (R, G, B)
```

`0x47` = 71 = **5 octets d'arguments + 66 octets de couleur** (22 × 3).

| Argument | Offset | Valeur observée |
|---|---|---|
| inconnu 0 | 8 | `0x00` |
| inconnu 1 | 9 | `0x00` |
| **rangée** | 10 | `0x00` à `0x05` |
| colonne de début | 11 | `0x00` |
| colonne de fin | 12 | `0x15` = 21 |

#### Ordre des composantes : **RGB**

Vérifié par envoi de couleurs pures et lecture directe du bus :

| Couleur envoyée | Octets observés |
|---|---|
| Rouge pur | `ff 00 00` |
| Vert pur | `00 ff 00` |
| Bleu pur | `00 00 ff` |
| Noir | `00 00 00` |

> À ne pas confondre avec le **SDK Chroma**, dont l'API REST utilise `0x00BBGGRR`.
> Le protocole du périphérique est bien en RGB. Supposer l'un depuis l'autre est
> une erreur.

---

## 5. Séquence d'une mise à jour complète

Sept transferts consécutifs, espacés d'environ 2,5 ms :

| # | Classe / ID | Contenu |
|---|---|---|
| 1 → 6 | `0f` / `03` | écriture des rangées 0 à 5, colonnes 0→21 |
| 7 | `0f` / `02` | effet `0x08` — passage en mode custom |

Un `0f`/`04` (luminosité) encadre généralement la séquence.

### Exemple de trame réelle — rangée 0 entièrement rouge

```
00 9f 00 00 00 47 0f 03 00 00 00 00 15
ff 00 00  ff 00 00  ff 00 00  ff 00 00  ff 00 00  ff 00 00
ff 00 00  ff 00 00  ff 00 00  ff 00 00  ff 00 00  ff 00 00
ff 00 00  ff 00 00  ff 00 00  ff 00 00  ff 00 00  ff 00 00
ff 00 00  ff 00 00  ff 00 00  ff 00 00
00 00 00 00 00 00 00 00 00
5e 00
```

`0x5e` = XOR des octets 2 à 87. Vérifié.

---

## 6. Matrice et cartographie

**6 rangées × 22 colonnes = 132 positions**, dont **107 portent une LED**. Les
positions vides valent `0xFFFFFFFF`.

```
rangée 0 :   0  --   2   3   4   5   6   7   8   9  10  11  12  13  14  15  16  --  --  --  --  --
rangée 1 :  22  23  24  25  26  27  28  29  30  31  32  33  34  35  36  37  38  39  40  41  42  --
rangée 2 :  44  45  46  47  48  49  50  51  52  53  54  55  56  57  58  59  60  61  62  63  64  --
rangée 3 :  66  67  68  69  70  71  72  73  74  75  76  77  78  79  --  --  --  83  84  85  --  --
rangée 4 :  88  89  90  91  92  93  94  95  96  97  98  99  -- 101  -- 103  -- 105 106 107 108  --
rangée 5 : 110 111 112  --  --  -- 116  --  --  -- 120 121 122 123 124 125 126  -- 128 129  --  --
```

> **Le piège pratique** : le périphérique déclare **132 LED**, pas 107. Un tampon
> de couleurs doit couvrir les 132 positions. En n'en envoyant que 106, la rangée 5
> conserve sa valeur précédente — symptôme observé : la dernière rangée physique
> (Ctrl, Espace, Alt…) reste figée sur l'ancienne couleur pendant que le reste change.

---

## 7. Modes exposés

| Index OpenRGB | Nom | Effet protocole | Animé par |
|---|---|---|---|
| 0 | `Direct` | `0x08` | **l'hôte** — n'affiche que ce qu'on pousse |
| 1 | `Off` | `0x00` | — |
| 2 | `Static` | non capturé | firmware |
| 3 | `Breathing` | non capturé | firmware |
| 4 | `Spectrum Cycle` | `0x03` | firmware |
| 5 | `Wave` | `0x04` + direction + vitesse | firmware |

**Conséquence de conception** : les effets firmware persistent après extinction du
logiciel hôte et ne coûtent aucun CPU. Le mode `Direct` impose une poussée continue
d'images — c'est le coût d'un moteur d'effets logiciel.

---

## 8. Méthode de capture, pour reproduire

```powershell
# 1. Repérer le hub portant le clavier
tshark -D                                    # liste \\.\USBPcap1, \\.\USBPcap2…
tshark -i \\.\USBPcap1 -a duration:6 -w test.pcap
tshark -r test.pcap -Y 'usb.idVendor == 0x1532' -T fields -e usb.device_address

# 2. Capturer en poussant des couleurs pures espacées dans le temps
tshark -i \\.\USBPcap1 -a duration:30 -w capture.pcap

# 3. Isoler les transferts de contrôle sortants
tshark -r capture.pcap -Y 'usb.device_address == 9 && usb.transfer_type == 0x02 && usb.endpoint_address == 0x00' -T fields -e frame.number

# 4. Extraire les 90 octets d'une trame
tshark -r capture.pcap -Y 'frame.number == 113' -V | Select-String 'Data Fragment:'
```

**Le point de méthode qui débloque tout** : envoyer des **couleurs pures et
uniformes**, bien séparées dans le temps. `ff 00 00` répété 22 fois saute aux yeux
dans un vidage hexadécimal, et la position des octets donne immédiatement l'ordre
des composantes sans avoir à le déduire.

> ⚠️ USBPcap n'attache son filtre aux hubs qu'**après redémarrage**. Avant cela,
> aucune interface `\\.\USBPcapN` n'apparaît.
>
> ⚠️ `USBPcapCMD --extcap-interfaces` peut ne rien renvoyer même en élévation ;
> passer directement par `tshark -i \\.\USBPcapN`, qui fonctionne.

---

## 9. Reste à établir

- [ ] Commandes exactes pour `Static` et `Breathing` (aucune trame `0x02` capturée)
- [ ] Signification des arguments 0 et 1 (offsets 8 et 9), constants à `0x00`
- [ ] L'identifiant de transaction (octet 1, `0x9f`) doit-il varier ? est-il vérifié ?
- [ ] Écriture partielle : les colonnes de début/fin permettent-elles de n'envoyer
      qu'un segment de rangée ? (à tester en écrivant directement en HID)
- [ ] Plage et effet réels des paramètres direction et vitesse de `Wave`
- [ ] Débit maximal accepté avant décrochage
- [ ] Contenu des réponses du périphérique (`GET_REPORT`)
- [ ] Le périphérique accepte-t-il un rapport plus court que 90 octets ?

---

## 10. Journal

| Date | Événement |
|---|---|
| 2026-09-11 | Identification matérielle, relevé des 6 modes via le SDK OpenRGB |
| 2026-09-11 | Installation de Wireshark 4.6.8 et USBPcap 1.5.4.0 |
| 2026-09-12 | **Première capture.** Transport, structure du rapport, ordre RGB, indexation des rangées, somme de contrôle établis et vérifiés |
| 2026-09-12 | Matrice 6×22 = 132 LED confirmée ; correction d'un envoi à 106 LED laissant la rangée 5 figée |
| 2026-09-12 | **Jeu de commandes complété** : `0x02` effet, `0x03` rangée, `0x04` luminosité. Correction : la 7ᵉ trame est un `SET EFFECT 0x08`, pas une validation |
| 2026-09-12 | Cartographie complète des 107 positions LED de la matrice |

---

## 11. Fichiers de capture

| Fichier | Contenu |
|---|---|
| `deathstalker-20260912-012139.pcap` | référence : rouge / vert / bleu / noir / blanc |
| `fix132-012638.pcap` | validation de l'envoi complet à 132 LED |
| `modes2-013622.pcap` | bascules d'effet : Static, Breathing, Spectrum, Wave, Off, Direct |
