---
title: Bauen
layout: default
parent: Startseite
nav_order: 3
---

# Aus dem Quellcode bauen
{: .no_toc }

## Inhalt
{: .no_toc .text-delta }

1. TOC
{:toc}

---

## Voraussetzungen

- **Visual Studio 2022 oder neuer** mit der Workload „Desktopentwicklung mit C++“
- **CMake** 3.16+
- **Git** (für Submodule und wxWidgets per FetchContent)

## Schnellstart

```bash
git clone --recursive https://github.com/SeiyaFunaokaJP/A2DP-Windows-Bridge.git
cd A2DP-Windows-Bridge
cmake -B build -A x64
cmake --build build --config Release
```

Die ausführbare Datei wird unter `build/app/Release/A2DPWB.exe` ausgegeben.

{: .note }
Der erste Build dauert mehrere Minuten, weil CMake FetchContent wxWidgets (v3.2.6) herunterlädt und kompiliert.

## Wenn Sie bereits ohne `--recursive` geklont haben

```bash
git submodule update --init --recursive
```

## Projektstruktur

```
A2DP-Windows-Bridge/
├── app/                    Quellcode der Anwendung
│   ├── src/                C++-Quelldateien
│   ├── lang/               Lokalisierung (en.json ist die Vorlage; tools/check_lang.py prüft die übrigen)
│   ├── resources/          Symbol, Manifest, Ressourcenskript
│   └── CMakeLists.txt      Build-Konfiguration der App
├── cmake/                  Release-Pakete (make_dist.cmake)
├── installer/              Inno-Setup-Skript für das Installationsprogramm (A2DPWB.iss)
├── compat/                 MSVC-Kompatibilitätsheader für AOSP-Code
├── docs/                   Dokumentation (diese Website)
├── extern/                 Bibliotheken von Drittanbietern (git submodules)
│   ├── btstack/            Bluetooth-Stack BTstack
│   ├── libldac/            LDAC-Encoder aus AOSP
│   ├── libopenaptx/        Encoder für aptX / aptX HD / aptX LL
│   ├── fdk-aac/            AAC-Encoder von Fraunhofer
│   └── json/               nlohmann/json (nur Header)
├── tools/
│   ├── a2dp_decode/        a2dpwb_decode: prüft / dekodiert den Medienstream in einer HCI-Aufzeichnung (.pklg)
│   ├── emu/                End-to-End-Test gegen eine virtuelle Bluetooth-Senke (Python)
│   └── linux_sink/         a2dpwb_sink: messender A2DP-Empfänger unter Linux (Python, Bumble)
├── CMakeLists.txt          Build-Konfiguration (Wurzel)
└── build.bat.example       Build-Hilfsskript (nach build.bat kopieren, CMake-Pfad eintragen)
```

## Build-Optionen

| CMake-Option | Standard | Beschreibung |
|:-------------|:---------|:-------------|
| `LDAC_SOFT_FLOAT` | `OFF` | Software-Gleitkomma für libldac verwenden |
| `A2DPWB_BUILD_TOOLS` | `ON` | Entwicklerwerkzeuge bauen (`a2dpwb_decode`, siehe [Verwendung](usage#verify-stream)). Nicht Teil des Release-Pakets |

```bash
cmake -B build -A x64 -DA2DPWB_BUILD_TOOLS=OFF
```

## Release-Pakete

```bash
cmake --build build --target A2DPWB_dist --config Release
```

erzeugt `build/dist/A2DPWB-<version>-win64-portable.zip` und, wenn [Inno Setup 6](https://jrsoftware.org/isinfo.php) installiert ist (`winget install JRSoftware.InnoSetup`; danach die CMake-Konfiguration erneut ausführen), aus `installer/A2DPWB.iss` die Datei `build/dist/A2DPWB-<version>-win64-setup.exe`. Beide enthalten dieselben Dateien.

## Testen ohne Hardware (tools/emu) {#emu}

`tools/emu` streamt jeden Codec von A2DPWB an eine virtuelle Bluetooth-Senke auf demselben PC, ganz ohne Adapter oder Kopfhörer. Es läuft nur unter Windows und benötigt zusätzlich zu den Build-Voraussetzungen **Python 3.11 oder neuer**.

```bash
python tools/emu/setup_env.py     # einmalig: legt tools/emu/.venv mit festen Paketversionen an
python tools/emu/run_test.py      # nach einem Release-Build; etwa 35 s für alle Codecs
```

Funktionsweise:

- `emu_sink.py` betreibt zwei virtuelle [Bumble](https://github.com/google/bumble)-Controller auf einer prozessinternen Verbindung. Einer wird für A2DPWB als H4 über TCP (`127.0.0.1`) bereitgestellt. Den anderen nutzt eine Bumble-A2DP-Senke, die SBC, AAC, aptX, aptX HD, aptX LL und LDAC anbietet und ihren HCI-Verkehr aufzeichnet.
- `run_test.py` führt `A2DPWB.exe --cli` für jeden Codec mit Entwicklungsoptionen aus: `--hci-tcp` (virtueller Controller statt WinUSB), `--test-tone` (1 kHz links / 1,5 kHz rechts statt Systemaudio), `--duration`, `--hci-capture`. Die Umgebungsvariable `A2DPWB_CONFIG_DIR` lässt A2DPWB einen separaten Konfigurationsordner verwenden, sodass die Kopplung mit der virtuellen Senke Ihre echten Link Keys nie berührt.
- Beide HCI-Aufzeichnungen werden anschließend mit `a2dpwb_decode` geprüft (die Senkenseite mit `--received`).

Ein Codec besteht den Test, wenn all dies zutrifft:

- der angeforderte Codec wurde ausgehandelt
- `a2dpwb_decode` findet weder auf der Sende- noch auf der Empfangsseite ein Problem
- die Senke hat jedes von A2DPWB gesendete Medienpaket und jeden Frame empfangen, und mindestens 90 % der angeforderten Audiodauer wurden gesendet
- außer bei LDAC (kein Open-Source-Decoder): das dekodierte Audio ist auf beiden Seiten identisch und entspricht dem Testton

| Option | Beschreibung |
|:-------|:-------------|
| `--codecs sbc aac ...` | Nur diese Codecs testen (`sbc aac aptx aptxhd aptxll ldac`, Standard: alle) |
| `--duration <s>` | Sekunden Streaming pro Codec (Standard 5) |
| `--max-packet <bytes>` | Wird an A2DPWB übergeben (max. Mediapaketgröße, siehe [Verwendung](usage)) |
| `--build-dir`, `--config` | Wo `A2DPWB.exe` / `a2dpwb_decode.exe` zu finden sind (Standard `build`, `Release`) |
| `--out <dir>` | Ausgabeordner (Standard `tools/emu/out`) |

Für jeden Codec enthält `tools/emu/out/<codec>/` die Aufzeichnungen (`a2dpwb.pklg`, `sink.pklg`), Protokolle, die Berichte von `a2dpwb_decode` und die dekodierten WAV-Dateien. Der Exitcode ist 0, wenn jeder Codec bestanden hat.

{: .note }
Die virtuelle Verbindung hat keinen Funk: Sie prüft Kodierung, Paketierung, AVDTP- / L2CAP-Signalisierung und die Interoperabilität mit einem unabhängigen Bluetooth-Stack (Bumble), aber nicht die Funkbedingungen, das Timing auf echter Hardware oder wie echte Kopfhörer das Audio wiedergeben. Die Bumble-Pakete dienen nur als Testwerkzeug und sind nicht Teil von A2DPWB (siehe THIRD_PARTY_LICENSES.md §13).

`emu_vhci.py` tut dasselbe gegenüber dem Linux-Bluetooth-Stack: Unter Linux bindet es über `/dev/vhci` einen virtuellen Adapter an BlueZ an, den der Messempfänger `tools/linux_sink` dann wie einen echten Adapter übernimmt - so lässt er sich ohne Hardware testen, optional mit verlorenen oder zurückgehaltenen Medienpaketen (`--drop`, `--stall`). Siehe `tools/linux_sink/README.md`.

## Abhängigkeiten

Alle Abhängigkeiten sind als git submodules enthalten oder werden beim Build abgerufen. Eine manuelle Installation ist nicht erforderlich.

| Bibliothek | Methode | Feste Version | Lizenz |
|:-----------|:--------|:--------------|:-------|
| BTstack | git submodule | v1.8.1-6-g5bc5cbdbe | BTstack License (BSD-3-Clause-artig mit einer Klausel gegen kommerzielle Nutzung) |
| Bluedroid SBC codec | in BTstack enthalten (`3rd-party/bluedroid`) | wie in BTstack | Apache-2.0 |
| rijndael | in BTstack enthalten (`3rd-party/rijndael`) | wie in BTstack | Gemeinfrei (Public Domain) |
| libldac (AOSP) | git submodule | android-15.0.0_r36-4-geeee1a3 | Apache-2.0 |
| libopenaptx | git submodule | 0.2.0 (nicht aktualisieren) | LGPL-2.1+ |
| fdk-aac | git submodule | v2.0.3-158-gd8e6b1a | FDK AAC License |
| nlohmann/json | git submodule (nur Header) | 3.12.0 | MIT |
| wxWidgets | CMake FetchContent | v3.2.6 | wxWindows Library Licence 3.1 |
| zlib / libpng / nanosvg | integrierte Kopien in wxWidgets | 1.2.13.1 / 1.6.37 / wie in wxWidgets | zlib / PNG Reference Library License v2 / zlib |

{: .warning }
Da BTstack eingebunden ist, darf jede von Ihnen gebaute `A2DPWB.exe` nur für private, nicht kommerzielle Zwecke verwendet und weitergegeben werden, auch wenn der Quellcode von A2DPWB selbst unter Apache-2.0 oder MIT doppelt lizenziert ist.

Ausführliche Lizenzinformationen finden Sie in [THIRD_PARTY_LICENSES.md](https://github.com/SeiyaFunaokaJP/A2DP-Windows-Bridge/blob/main/THIRD_PARTY_LICENSES.md).
