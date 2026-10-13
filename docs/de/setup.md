---
title: Einrichtung
layout: default
parent: Startseite
nav_order: 1
---

# Einrichtung
{: .no_toc }

## Inhalt
{: .no_toc .text-delta }

1. TOC
{:toc}

---

## Voraussetzungen

- **Windows 10/11** (x64)
- **Eigener USB-Bluetooth-Adapter** (getrennt vom integrierten Bluetooth)
- **Zadig** ([https://zadig.akeo.ie/](https://zadig.akeo.ie/))

## A2DPWB installieren

Jedes [Release](https://github.com/SeiyaFunaokaJP/A2DP-Windows-Bridge/releases/latest) gibt es in zwei Formen mit denselben Dateien:

- **Installationsprogramm** (`A2DPWB-<version>-win64-setup.exe`): installiert für den aktuellen Benutzer ohne Administratorrechte (oder auf Wunsch für alle Benutzer), legt einen Eintrag im Startmenü und ein Deinstallationsprogramm an und startet A2DPWB bei der Erstinstallation in der im Setup gewählten Sprache
- **Portabel** (`A2DPWB-<version>-win64-portable.zip`): an beliebiger Stelle entpacken und `A2DPWB.exe` ausführen

Beide speichern Einstellungen, Profile, Link Keys und Firmware in `%APPDATA%\A2DPWB`, Sie können also zwischen ihnen wechseln. Bei der Deinstallation bleibt dieser Ordner erhalten.

## Schritt 1: WinUSB-Treiber installieren

A2DPWB kommuniziert über WinUSB direkt mit einem USB-Bluetooth-Adapter. Sie benötigen einen **eigenen Adapter** -- Ihr integriertes Bluetooth funktioniert für das normale Windows-Bluetooth weiterhin wie gewohnt.

1. Stecken Sie einen USB-Bluetooth-Adapter ein
2. Laden Sie [Zadig](https://zadig.akeo.ie/) herunter und öffnen Sie es
3. Wählen Sie **Options > List All Devices**
4. Wählen Sie Ihren USB-Bluetooth-Adapter in der Dropdown-Liste aus
5. Wählen Sie **WinUSB** als Zieltreiber
6. Klicken Sie auf **Replace Driver**

{: .warning }
Solange sich der Adapter im WinUSB-Modus befindet, kann Windows ihn nicht für normales Bluetooth verwenden. Nutzen Sie für normale Peripheriegeräte (Tastatur, Maus usw.) Ihr integriertes Bluetooth.

## Schritt 2: Bluetooth-Adresse der Kopfhörer ermitteln

Sie benötigen die Bluetooth-MAC-Adresse Ihrer Kopfhörer bzw. Lautsprecher.

**Über die Windows-Einstellungen:**
1. Öffnen Sie **Einstellungen > Bluetooth und Geräte**
2. Klicken Sie auf Ihr Audiogerät
3. Klicken Sie auf **Eigenschaften**
4. Die Bluetooth-Adresse wird angezeigt (Format: `AA:BB:CC:DD:EE:FF`)

**Über die A2DPWB-GUI:**
- Starten Sie `A2DPWB.exe`; gekoppelte Bluetooth-Audiogeräte werden in der Geräteliste angezeigt
- Oder verwenden Sie **Geräte suchen** im Profildialog, um Geräte in der Nähe mit dem USB-Adapter zu finden (versetzen Sie die Kopfhörer dazu in den Kopplungsmodus)

**Über die CLI:**
```
A2DPWB.exe --cli -l
```

## Schritt 3: A2DPWB starten

**GUI:**
```
A2DPWB.exe
```

**CLI:**
```
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF
```

Ausführliche Informationen finden Sie unter [Verwendung](usage).

---

## Firmware (Realtek-Adapter)

USB-Bluetooth-Adapter mit Realtek-Chip (z. B. TP-Link UB500, RTL8761BU-Dongles) benötigen für den Betrieb eine proprietäre Firmware. CSR-Adapter benötigen keine Firmware. Intel- und Broadcom-Adapter (experimentell) benötigen eventuell eigene Firmwaredateien: siehe [Empfohlene Adapter](#adapters).

### GUI (geführter Download)

Starten Sie `A2DPWB.exe` und wählen Sie **Aktionen > Bluetooth-Firmware auswählen**. Fehlt die Firmware, wird eine Warnung angezeigt. Öffnen Sie mit der Schaltfläche **Downloadseite öffnen** die Seite [linux-firmware/rtl_bt](https://git.kernel.org/pub/scm/linux/kernel/git/firmware/linux-firmware.git/tree/rtl_bt) im Browser und anschließend mit **Konfigurationsordner öffnen** den Zielordner. Laden Sie die benötigten `.bin`-Dateien herunter und legen Sie sie im Konfigurationsordner ab.

Der Dialog erkennt den Chipsatz Ihres Adapters automatisch und zeigt an, welche Firmwaredateien benötigt werden.

### Einstellung Bluetooth-Chip

Die Liste **Bluetooth-Chip** im selben Dialog legt fest, ob A2DPWB die Realtek-Initialisierung (Firmware-Upload) auf dem Adapter ausführt:

| Auswahl | Bedeutung |
|:--------|:----------|
| **Automatisch** (Standard) | Wird bei jedem Start von A2DPWB anhand des erkannten Adapters entschieden. Ein bekannter Realtek-Adapter erhält seinen Chip; jeder andere Adapter erhält keine Realtek-Initialisierung |
| **Nicht Realtek** | Nie eine Realtek-Initialisierung ausführen. Verwenden Sie dies für Intel-, Broadcom-, CSR- und andere Adapter, falls Automatisch Ihren Adapter nicht erkennt |
| **RTL8761BU**, **RTL8822CU**, ... | Der jeweilige Realtek-Chip. Nötig für einen Adapter mit Realtek-Chip, den Automatisch nicht erkennt (ein OEM-Produkt mit eigener VID:PID). Die Statuszeile zeigt, ob die zugehörigen `.bin`-Dateien im Konfigurationsordner liegen |
| **Custom** | Verwendet `custom_fw.bin` und `custom_config.bin` aus dem Konfigurationsordner |

Wählen Sie für einen Adapter ohne Realtek-Chip weder einen Realtek-Chip noch Custom: A2DPWB würde versuchen, Realtek-Firmware auf ihn hochzuladen, und der Adapter startet dann möglicherweise nicht.

### Manueller Download

1. Laden Sie die Firmware- und Config-`.bin`-Dateien für Ihren Chipsatz von [linux-firmware/rtl_bt](https://git.kernel.org/pub/scm/linux/kernel/git/firmware/linux-firmware.git/tree/rtl_bt) herunter (z. B. `rtl8761bu_fw.bin` und `rtl8761bu_config.bin`)
2. Legen Sie beide Dateien im A2DPWB-Konfigurationsordner ab (`%APPDATA%\A2DPWB` oder der im Firmware-Dialog angezeigte Pfad)

{: .note }
Diese Firmwaredateien sind proprietäre Realtek-Binärdateien, die über das Projekt linux-firmware verteilt werden. Sie sind nicht in diesem Repository enthalten. Die Bedingungen für die Weitergabe finden Sie in [WHENCE](https://git.kernel.org/pub/scm/linux/kernel/git/firmware/linux-firmware.git/tree/WHENCE).

---

## Empfohlene Adapter {#adapters}

| Chipsatz | Beispielprodukte | Hinweise |
|:---------|:-----------------|:---------|
| Realtek | TP-Link UB500, RTL8761BU | Firmware-Download erforderlich (siehe oben). Am besten getestet |
| CSR | Generische CSR8510-Dongles | Funktioniert sofort, keine Firmware nötig |
| Broadcom | ASUS BT400 (BCM20702) | *Experimentell.* Adapter, die statt der Bluetooth-Klasse `E0/01/01` die Herstellerklasse `FF/01/01` melden (OEM-Dongles mit BCM20702), werden ebenfalls erkannt. Läuft mit der ROM-Firmware; eine optionale PatchRAM-Datei `.hcd` (z. B. `BCM20702A1-0b05-17cb.hcd`) im Konfigurationsordner wird beim Start hochgeladen |
| Intel | Intel 8265 / 9260 / AX200 / AX201 | *Experimentell.* Im Bootloader-Modus (z. B. nach einem Kaltstart) werden die passenden Dateien `ibt-*.sfi` + `ibt-*.ddc` aus linux-firmware `intel/` im Konfigurationsordner benötigt. AX210 und neuer (TLV-Bootloader) werden noch nicht unterstützt |
| MediaTek, Qualcomm | MT7921/MT7922, QCA61x4 | Nicht unterstützt: benötigen Firmware-Loader, die A2DPWB nicht hat |

USB-Bluetooth-Adapter ab Version 5.0 eignen sich im Allgemeinen am besten für Codecs mit hoher Bitrate wie LDAC.
