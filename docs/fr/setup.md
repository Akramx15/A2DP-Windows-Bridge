---
title: Installation
layout: default
parent: Accueil
nav_order: 1
---

# Installation
{: .no_toc }

## Table des matières
{: .no_toc .text-delta }

1. TOC
{:toc}

---

## Prérequis

- **Windows 10/11** (x64)
- **Adaptateur Bluetooth USB dédié** (distinct du Bluetooth intégré)
- **Zadig** ([https://zadig.akeo.ie/](https://zadig.akeo.ie/))

## Installer A2DPWB

Chaque [version](https://github.com/SeiyaFunaokaJP/A2DP-Windows-Bridge/releases/latest) est proposée sous deux formes contenant les mêmes fichiers :

- **Programme d'installation** (`A2DPWB-<version>-win64-setup.exe`) : installe pour l'utilisateur actuel sans droits administrateur (ou pour tous les utilisateurs, si vous le choisissez), ajoute une entrée au menu Démarrer et un programme de désinstallation, et lors de la première installation démarre A2DPWB dans la langue choisie pendant l'installation
- **Portable** (`A2DPWB-<version>-win64-portable.zip`) : extrayez-le où vous voulez et lancez `A2DPWB.exe`

Les deux conservent les paramètres, les profils, les clés de liaison et le firmware dans `%APPDATA%\A2DPWB`, vous pouvez donc passer de l'un à l'autre. La désinstallation laisse ce dossier en place.

## Étape 1 : installer le pilote WinUSB

A2DPWB communique directement avec un adaptateur Bluetooth USB via WinUSB. Il vous faut un **adaptateur dédié** -- votre Bluetooth intégré continue de fonctionner normalement pour le Bluetooth habituel de Windows.

1. Branchez un adaptateur Bluetooth USB
2. Téléchargez et ouvrez [Zadig](https://zadig.akeo.ie/)
3. Allez dans **Options > List All Devices**
4. Sélectionnez votre adaptateur Bluetooth USB dans la liste déroulante
5. Sélectionnez **WinUSB** comme pilote cible
6. Cliquez sur **Replace Driver**

{: .warning }
Tant que l'adaptateur est en mode WinUSB, Windows ne peut pas l'utiliser pour le Bluetooth habituel. Utilisez votre Bluetooth intégré pour les périphériques courants (clavier, souris, etc.).

## Étape 2 : trouver l'adresse Bluetooth de votre casque

Il vous faut l'adresse MAC Bluetooth de votre casque ou de vos enceintes.

**Depuis les Paramètres Windows :**
1. Ouvrez **Paramètres > Bluetooth et appareils**
2. Cliquez sur votre appareil audio
3. Cliquez sur **Propriétés**
4. L'adresse Bluetooth s'affiche (format : `AA:BB:CC:DD:EE:FF`)

**Depuis la GUI d'A2DPWB :**
- Lancez `A2DPWB.exe` : les appareils audio Bluetooth associés sont listés dans la liste déroulante des appareils
- Ou utilisez **Rechercher** dans la boîte de dialogue du profil pour trouver les appareils à proximité avec l'adaptateur USB (mettez le casque en mode d'association)

**Depuis la CLI :**
```
A2DPWB.exe --cli -l
```

## Étape 3 : lancer A2DPWB

**GUI :**
```
A2DPWB.exe
```

**CLI :**
```
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF
```

Voir [Utilisation](usage) pour tous les détails.

---

## Firmware (adaptateurs Realtek)

Les adaptateurs Bluetooth USB à puce Realtek (p. ex. TP-Link UB500, dongles RTL8761BU) ont besoin d'un firmware propriétaire pour fonctionner. Les adaptateurs CSR n'ont besoin d'aucun firmware. Les adaptateurs Intel et Broadcom (expérimental) peuvent nécessiter leurs propres fichiers de firmware : voir [Adaptateurs recommandés](#adapters).

### GUI (téléchargement guidé)

Lancez `A2DPWB.exe` et choisissez **Actions > Sélection du firmware Bluetooth**. Si le firmware est absent, un avertissement s'affiche. Le bouton **Ouvrir la page de téléchargement** ouvre la page [linux-firmware/rtl_bt](https://git.kernel.org/pub/scm/linux/kernel/git/firmware/linux-firmware.git/tree/rtl_bt) dans votre navigateur, puis **Ouvrir le dossier de configuration** ouvre le dossier de destination. Téléchargez les fichiers `.bin` nécessaires et placez-les dans le dossier de configuration.

La boîte de dialogue détecte automatiquement la puce de votre adaptateur et indique quels fichiers de firmware sont nécessaires.

### Réglage Puce Bluetooth

La liste **Puce Bluetooth** de la même boîte de dialogue détermine si A2DPWB effectue l'initialisation Realtek (envoi du firmware) sur l'adaptateur :

| Choix | Signification |
|:-------|:--------|
| **Auto** (par défaut) | Déterminé à partir de l'adaptateur détecté à chaque démarrage d'A2DPWB. Un adaptateur Realtek connu reçoit sa puce ; tout autre adaptateur ne reçoit aucune initialisation Realtek |
| **Pas Realtek** | Ne jamais effectuer l'initialisation Realtek. À utiliser pour les adaptateurs Intel, Broadcom, CSR et autres si Auto ne reconnaît pas le vôtre |
| **RTL8761BU**, **RTL8822CU**, ... | Cette puce Realtek. Nécessaire pour un adaptateur à puce Realtek qu'Auto ne reconnaît pas (un produit OEM avec son propre VID:PID). La ligne d'état indique si ses fichiers `.bin` sont présents dans le dossier de configuration |
| **Custom** | Utilise `custom_fw.bin` et `custom_config.bin` du dossier de configuration |

Ne choisissez pas une puce Realtek ni Custom pour un adaptateur non Realtek : A2DPWB essaierait d'y envoyer un firmware Realtek, et l'adaptateur pourrait ne pas démarrer.

### Téléchargement manuel

1. Téléchargez les fichiers `.bin` de firmware et de configuration de votre puce depuis [linux-firmware/rtl_bt](https://git.kernel.org/pub/scm/linux/kernel/git/firmware/linux-firmware.git/tree/rtl_bt) (p. ex. `rtl8761bu_fw.bin` et `rtl8761bu_config.bin`)
2. Placez les deux fichiers dans le dossier de configuration d'A2DPWB (`%APPDATA%\A2DPWB`, ou le chemin affiché dans la boîte de dialogue du firmware)

{: .note }
Ces fichiers de firmware sont des binaires propriétaires de Realtek distribués via le projet linux-firmware. Ils ne sont pas inclus dans ce dépôt. Voir [WHENCE](https://git.kernel.org/pub/scm/linux/kernel/git/firmware/linux-firmware.git/tree/WHENCE) pour les conditions de redistribution.

---

## Adaptateurs recommandés {#adapters}

| Puce | Exemples de produits | Remarques |
|:--------|:-----------------|:------|
| Realtek | TP-Link UB500, RTL8761BU | Nécessite le téléchargement du firmware (voir ci-dessus). Le mieux testé |
| CSR | Dongles CSR8510 génériques | Fonctionne d'emblée, sans firmware |
| Broadcom | ASUS BT400 (BCM20702) | *Expérimental.* Les adaptateurs qui annoncent la classe constructeur `FF/01/01` au lieu de la classe Bluetooth `E0/01/01` (dongles OEM BCM20702) sont aussi détectés. Fonctionne avec le firmware en ROM ; un fichier PatchRAM `.hcd` facultatif (p. ex. `BCM20702A1-0b05-17cb.hcd`) placé dans le dossier de configuration est envoyé au démarrage |
| Intel | Intel 8265 / 9260 / AX200 / AX201 | *Expérimental.* En mode bootloader (p. ex. après un démarrage à froid), nécessite les fichiers `ibt-*.sfi` + `ibt-*.ddc` correspondants, issus de `intel/` dans linux-firmware, dans le dossier de configuration. AX210 et plus récents (bootloader TLV) ne sont pas encore pris en charge |
| MediaTek, Qualcomm | MT7921/MT7922, QCA61x4 | Non pris en charge : nécessitent des chargeurs de firmware qu'A2DPWB ne possède pas |

Les adaptateurs Bluetooth USB 5.0+ donnent généralement les meilleurs résultats avec les codecs à haut débit comme LDAC.
