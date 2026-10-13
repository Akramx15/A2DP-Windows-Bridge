---
title: Compilation
layout: default
parent: Accueil
nav_order: 3
---

# Compilation depuis les sources
{: .no_toc }

## Table des matières
{: .no_toc .text-delta }

1. TOC
{:toc}

---

## Prérequis

- **Visual Studio 2022 ou ultérieur** avec la charge de travail « Développement Desktop en C++ »
- **CMake** 3.16+
- **Git** (pour les sous-modules et le FetchContent de wxWidgets)

## Compilation rapide

```bash
git clone --recursive https://github.com/SeiyaFunaokaJP/A2DP-Windows-Bridge.git
cd A2DP-Windows-Bridge
cmake -B build -A x64
cmake --build build --config Release
```

L'exécutable est généré dans `build/app/Release/A2DPWB.exe`.

{: .note }
La première compilation prend plusieurs minutes, car CMake FetchContent télécharge et compile wxWidgets (v3.2.6).

## Si vous avez déjà cloné sans `--recursive`

```bash
git submodule update --init --recursive
```

## Structure du projet

```
A2DP-Windows-Bridge/
├── app/                    Application source
│   ├── src/                C++ source files
│   ├── lang/               Localization (en.json is the source; tools/check_lang.py checks the others)
│   ├── resources/          Icon, manifest, resource script
│   └── CMakeLists.txt      App build config
├── cmake/                  Release packaging (make_dist.cmake)
├── installer/              Inno Setup script for the installer (A2DPWB.iss)
├── compat/                 MSVC compatibility headers for AOSP code
├── docs/                   Documentation (this site)
├── extern/                 Third-party libraries (git submodules)
│   ├── btstack/            BTstack Bluetooth stack
│   ├── libldac/            AOSP LDAC encoder
│   ├── libopenaptx/        aptX / aptX HD / aptX LL encoder
│   ├── fdk-aac/            Fraunhofer AAC encoder
│   └── json/               nlohmann/json (header-only)
├── tools/
│   ├── a2dp_decode/        a2dpwb_decode: checks / decodes the media stream in an HCI capture (.pklg)
│   ├── emu/                End-to-end test against a virtual Bluetooth sink (Python)
│   └── linux_sink/         a2dpwb_sink: measuring A2DP receiver on Linux (Python, Bumble)
├── CMakeLists.txt          Root build config
└── build.bat.example       Build helper script (copy to build.bat, set the CMake path)
```

## Options de compilation

| Option CMake | Par défaut | Description |
|:-------------|:--------|:------------|
| `LDAC_SOFT_FLOAT` | `OFF` | Utiliser la virgule flottante logicielle pour libldac |
| `A2DPWB_BUILD_TOOLS` | `ON` | Compiler les outils de développement (`a2dpwb_decode`, voir [Utilisation](usage#verify-stream)). Ne font pas partie du paquet de version |

```bash
cmake -B build -A x64 -DA2DPWB_BUILD_TOOLS=OFF
```

## Paquets de version

```bash
cmake --build build --target A2DPWB_dist --config Release
```

produit `build/dist/A2DPWB-<version>-win64-portable.zip` et, si [Inno Setup 6](https://jrsoftware.org/isinfo.php) est installé (`winget install JRSoftware.InnoSetup` ; relancez ensuite la configuration CMake), `build/dist/A2DPWB-<version>-win64-setup.exe` à partir de `installer/A2DPWB.iss`. Les deux contiennent les mêmes fichiers.

## Tester sans matériel (tools/emu) {#emu}

`tools/emu` diffuse chaque codec depuis A2DPWB vers un récepteur Bluetooth virtuel sur le même PC, sans adaptateur ni casque. Il fonctionne uniquement sous Windows et nécessite **Python 3.11 ou ultérieur** en plus des prérequis de compilation.

```bash
python tools/emu/setup_env.py     # once: creates tools/emu/.venv with pinned packages
python tools/emu/run_test.py      # after a Release build; about 35 s for all codecs
```

Fonctionnement :

- `emu_sink.py` exécute deux contrôleurs virtuels [Bumble](https://github.com/google/bumble) reliés par une liaison interne au processus. L'un est exposé en H4 via TCP (`127.0.0.1`) pour A2DPWB. L'autre est utilisé par un récepteur A2DP Bumble qui propose SBC, AAC, aptX, aptX HD, aptX LL et LDAC, et enregistre son trafic HCI.
- `run_test.py` lance `A2DPWB.exe --cli` pour chaque codec avec les options de développement : `--hci-tcp` (contrôleur virtuel au lieu de WinUSB), `--test-tone` (1 kHz à gauche / 1,5 kHz à droite au lieu de l'audio du système), `--duration`, `--hci-capture`. La variable d'environnement `A2DPWB_CONFIG_DIR` fait utiliser à A2DPWB un dossier de configuration séparé, si bien que l'association avec le récepteur virtuel ne touche jamais vos vraies clés de liaison.
- Les deux captures HCI sont ensuite vérifiées avec `a2dpwb_decode` (le côté récepteur avec `--received`).

Un codec réussit le test lorsque toutes ces conditions sont remplies :

- le codec demandé a été négocié
- `a2dpwb_decode` ne trouve aucun problème, ni côté envoi ni côté réception
- le récepteur a reçu chaque paquet média et chaque trame envoyés par A2DPWB, et au moins 90 % de la durée audio demandée a été envoyée
- sauf pour LDAC (pas de décodeur open source) : l'audio décodé est identique des deux côtés et correspond au son de test

| Option | Description |
|:-------|:------------|
| `--codecs sbc aac ...` | Tester uniquement ces codecs (`sbc aac aptx aptxhd aptxll ldac`, par défaut : tous) |
| `--duration <s>` | Secondes de diffusion par codec (5 par défaut) |
| `--max-packet <bytes>` | Transmis à A2DPWB (taille max. des paquets média, voir [Utilisation](usage)) |
| `--build-dir`, `--config` | Où trouver `A2DPWB.exe` / `a2dpwb_decode.exe` (par défaut `build`, `Release`) |
| `--out <dir>` | Dossier de sortie (par défaut `tools/emu/out`) |

Pour chaque codec, `tools/emu/out/<codec>/` contient les captures (`a2dpwb.pklg`, `sink.pklg`), les journaux, les rapports d'`a2dpwb_decode` et les fichiers WAV décodés. Le code de sortie est 0 lorsque tous les codecs ont réussi.

{: .note }
La liaison virtuelle n'a pas de radio : elle vérifie l'encodage, la mise en paquets, la signalisation AVDTP / L2CAP et l'interopérabilité avec une pile Bluetooth indépendante (Bumble), mais pas les conditions radio, le timing sur du vrai matériel ni la façon dont un vrai casque restitue l'audio. Les paquets Bumble ne sont que des outils de test et ne font pas partie d'A2DPWB (voir THIRD_PARTY_LICENSES.md §13).

`emu_vhci.py` fait de même vis-à-vis de la pile Bluetooth de Linux : sous Linux, il attache un adaptateur virtuel à BlueZ via `/dev/vhci`, que le récepteur de mesure `tools/linux_sink` prend ensuite comme un vrai adaptateur - il peut ainsi être testé sans matériel, éventuellement avec des paquets média perdus ou retenus (`--drop`, `--stall`). Voir `tools/linux_sink/README.md`.

## Dépendances

Toutes les dépendances sont incluses sous forme de sous-modules git ou récupérées au moment de la compilation. Aucune installation manuelle n'est nécessaire.

| Bibliothèque | Méthode | Version figée | Licence |
|:--------|:-------|:---------------|:--------|
| BTstack | sous-module git | v1.8.1-6-g5bc5cbdbe | BTstack License (de type BSD-3-Clause avec une clause non commerciale) |
| Codec SBC Bluedroid | inclus dans BTstack (`3rd-party/bluedroid`) | celle de BTstack | Apache-2.0 |
| rijndael | inclus dans BTstack (`3rd-party/rijndael`) | celle de BTstack | Domaine public |
| libldac (AOSP) | sous-module git | android-15.0.0_r36-4-geeee1a3 | Apache-2.0 |
| libopenaptx | sous-module git | 0.2.0 (ne pas mettre à jour) | LGPL-2.1+ |
| fdk-aac | sous-module git | v2.0.3-158-gd8e6b1a | FDK AAC License |
| nlohmann/json | sous-module git (en-têtes uniquement) | 3.12.0 | MIT |
| wxWidgets | CMake FetchContent | v3.2.6 | wxWindows Library Licence 3.1 |
| zlib / libpng / nanosvg | copies intégrées à wxWidgets | 1.2.13.1 / 1.6.37 / celle de wxWidgets | zlib / PNG Reference Library License v2 / zlib |

{: .warning }
Comme BTstack y est lié, tout `A2DPWB.exe` que vous compilez ne peut être utilisé et redistribué qu'à des fins personnelles et non commerciales, même si le code source d'A2DPWB lui-même est sous double licence Apache-2.0 ou MIT.

Voir [THIRD_PARTY_LICENSES.md](https://github.com/SeiyaFunaokaJP/A2DP-Windows-Bridge/blob/main/THIRD_PARTY_LICENSES.md) pour le détail complet des licences.
