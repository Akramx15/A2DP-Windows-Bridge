---
title: Setup
layout: default
nav_order: 2
---

# Setup
{: .no_toc }

## Table of contents
{: .no_toc .text-delta }

1. TOC
{:toc}

---

## Prerequisites

- **Windows 10 (22H2) / 11** (x64)
- **Dedicated USB Bluetooth adapter** (separate from built-in Bluetooth)

A2DPWB assigns the WinUSB driver to the adapter itself. [Zadig](https://zadig.akeo.ie/) is not needed any more (it still works, see [Using Zadig instead](#zadig)).

## Installing A2DPWB

Each [release](https://github.com/SeiyaFunaokaJP/A2DP-Windows-Bridge/releases/latest) comes in two forms with the same files:

- **Installer** (`A2DPWB-<version>-win64-setup.exe`): installs for the current user without administrator rights, adds a Start menu entry (**A2DP Windows Bridge**) and an uninstaller, and installs the Microsoft Edge WebView2 Runtime if it is missing (Windows 11 includes it). Updates install themselves from the app (**Settings > Updates > Update now**)
- **Portable** (`A2DPWB-<version>-win64-portable.zip`): extract anywhere and run `a2dpwb.exe`. Keep the three programs (`a2dpwb.exe`, `a2dpwb-core.exe`, `a2dpwb-driver.exe`) together. The portable copy only tells you about updates; it needs the WebView2 Runtime installed

Both keep settings, profiles, link keys and firmware in `%APPDATA%\A2DPWB`, so you can switch between them. Uninstalling leaves that folder in place.

### Upgrading from v1 {#from-v1}

Run the v2 installer over v1. It asks you to close v1 if it is running, then uninstalls v1 and keeps the settings folder: profiles, the pairing with your headphones and the firmware files carry over, and the headphones do not need to be paired again. An adapter that already has WinUSB (from Zadig) keeps it. If **Start with Windows** was on, it now starts v2.

At its first start v2 copies the v1 settings to `%APPDATA%\A2DPWB\backup-v1\` before it reads them.

## Step 1: First-Run Setup {#first-run}

Plug in the USB Bluetooth adapter and start A2DPWB. When there is no profile yet, or no adapter has WinUSB, A2DPWB opens **Set up A2DPWB**, four steps:

1. **Choose the adapter**: the USB Bluetooth adapters found. If yours is missing, plug it in and press **Refresh**
2. **Driver and firmware**: **Assign WinUSB** (see [Step 2](#winusb)), and for Realtek adapters the firmware files (see [Firmware](#firmware))
3. **Find your headphones**: put them into pairing mode, **Scan Devices**, choose them, then **Create profile** (see [Step 3](#headphones))
4. **Test**: connect, then **Play test tone**: a higher tone in the right ear and a lower one in the left

**Set up later** skips the rest; **Settings > Run setup** opens it again.

## Step 2: Assign WinUSB to the Adapter {#winusb}

A2DPWB communicates directly with a USB Bluetooth adapter via WinUSB, the generic USB driver included in Windows. You need a **dedicated adapter** -- your built-in Bluetooth continues to work normally for regular Windows Bluetooth.

1. Open the **Adapter** screen (or the setup's **Driver and firmware** step)
2. The adapter's **Driver** reads **Windows Bluetooth** (or **Another driver** / **No driver**). Click **Assign WinUSB**
3. Read the confirmation and continue: Windows asks for administrator permission (UAC)
4. The driver now reads **WinUSB (used by A2DPWB)**

{: .warning }
While WinUSB is assigned, Windows' own Bluetooth no longer sees the adapter: devices paired with Windows through it stop working until the driver is restored. Use your built-in Bluetooth for normal peripherals (keyboard, mouse, etc.), and **never assign WinUSB to the PC's built-in Bluetooth adapter**.

**Restore driver** on the same screen gives the adapter back to Windows: Windows picks its own driver again (normally its Bluetooth driver) and the adapter works with Windows' Bluetooth. A2DPWB cannot stream through it until WinUSB is assigned again. Restore the driver before you uninstall A2DPWB if you want to use the adapter with Windows again.

### Using Zadig instead {#zadig}

A WinUSB driver installed with [Zadig](https://zadig.akeo.ie/) works just as well:

1. Open Zadig, go to **Options > List All Devices**
2. Select your USB Bluetooth adapter from the dropdown
3. Select **WinUSB** as the target driver and click **Replace Driver**

## Step 3: Find Your Headphones {#headphones}

**Scan Devices** (in the setup and in the profile editor) finds nearby devices with the USB adapter for about 10 seconds. Put the headphones into pairing mode first. Devices A2DPWB has paired with before are listed as **Paired**.

If the scan does not find them, enter the Bluetooth address under **Manual:** and click **Use**. Typed or pasted addresses are formatted as you go: `AA:BB:CC:DD:EE:FF`, `aa-bb-cc-dd-ee-ff`, `AABBCCDDEEFF` and `AA BB CC DD EE FF` all become `AA:BB:CC:DD:EE:FF`.

**From Windows Settings** (if the headphones were paired with the PC's built-in Bluetooth):
1. Open **Settings > Bluetooth & devices**
2. Click on your audio device
3. Click **Properties**
4. The Bluetooth address is shown (format: `AA:BB:CC:DD:EE:FF`)

**From the command line** (quit A2DPWB first, tray > **Exit**):
```
a2dpwb-core.exe --cli --scan
```

The new profile starts with **Auto** as the codec and the system audio as the capture source; the defaults give the best sound. See [Usage](usage) for everything else.

---

## Firmware (Realtek Adapters) {#firmware}

Realtek-based USB Bluetooth adapters (e.g., TP-Link UB500, RTL8761BU dongles) require proprietary firmware to operate. CSR adapters need no firmware. Intel and Broadcom adapters (experimental) may need firmware files of their own: see [Recommended Adapters](#adapters).

### In the App (Guided Download)

The **Adapter** screen (and the setup's **Driver and firmware** step) shows the firmware of the adapter: **Firmware OK**, **No firmware files needed**, or **Firmware not found** with the required file names. Use the **Open Download Page** button to open the [linux-firmware/rtl_bt](https://git.kernel.org/pub/scm/linux/kernel/git/firmware/linux-firmware.git/tree/rtl_bt) page in your browser, then use **Open Config Folder** to open the destination folder. Download the required `.bin` files and place them in the config folder.

A2DPWB detects the adapter's chipset and shows which firmware files are needed.

### Bluetooth Chip setting

The **Bluetooth Chip** list on the **Adapter** screen decides whether A2DPWB runs Realtek initialization (firmware upload) on the adapter:

| Choice | Meaning |
|:-------|:--------|
| **Auto** (default) | Decided from the detected adapter each time A2DPWB starts. A known Realtek adapter gets its chip; any other adapter gets no Realtek initialization |
| **Not Realtek** | Never run Realtek initialization. Use it for Intel, Broadcom, CSR and other adapters if Auto does not recognize yours |
| **RTL8761BU**, **RTL8822CU**, ... | That Realtek chip. Needed for a Realtek-based adapter that Auto does not recognize (an OEM product under its own VID:PID). The status line shows whether its `.bin` files are in the config folder |
| **Custom** | Uses `custom_fw.bin` and `custom_config.bin` from the config folder |

Do not choose a Realtek chip or Custom for a non-Realtek adapter: A2DPWB would try to upload Realtek firmware to it, and the adapter may fail to start.

### Manual Download

1. Download the firmware and config `.bin` files for your chipset from [linux-firmware/rtl_bt](https://git.kernel.org/pub/scm/linux/kernel/git/firmware/linux-firmware.git/tree/rtl_bt) (e.g., `rtl8761bu_fw.bin` and `rtl8761bu_config.bin`)
2. Place both files in the A2DPWB config folder (`%APPDATA%\A2DPWB`, also under **Settings > Settings folder**)

{: .note }
These firmware files are proprietary Realtek binaries distributed via the linux-firmware project. They are not included in this repository. See [WHENCE](https://git.kernel.org/pub/scm/linux/kernel/git/firmware/linux-firmware.git/tree/WHENCE) for redistribution terms.

---

## Recommended Adapters {#adapters}

| Chipset | Example Products | Notes |
|:--------|:-----------------|:------|
| Realtek | TP-Link UB500, RTL8761BU | Requires firmware download (see above). Best tested |
| CSR | Generic CSR8510 dongles | Works out of the box, no firmware needed |
| Broadcom | ASUS BT400 (BCM20702) | *Experimental.* Adapters that report the vendor class `FF/01/01` instead of Bluetooth `E0/01/01` (BCM20702 OEM dongles) are detected too. Runs on ROM firmware; an optional PatchRAM `.hcd` (e.g. `BCM20702A1-0b05-17cb.hcd`) in the config folder is uploaded at startup |
| Intel | Intel 8265 / 9260 / AX200 / AX201 | *Experimental.* In bootloader mode (e.g. after a cold boot) needs the matching `ibt-*.sfi` + `ibt-*.ddc` from linux-firmware `intel/` in the config folder. AX210 and newer (TLV bootloader) are not supported yet |
| MediaTek, Qualcomm | MT7921/MT7922, QCA61x4 | Not supported: need firmware loaders A2DPWB does not have |

USB Bluetooth 5.0+ adapters generally work best for high-bitrate codecs like LDAC.

When several adapters have WinUSB, A2DPWB uses the first one it finds. Keep WinUSB on only the adapter you use for A2DPWB.
