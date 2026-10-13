---
title: Compilación
layout: default
parent: Inicio
nav_order: 3
---

# Compilación desde el código fuente
{: .no_toc }

## Contenido
{: .no_toc .text-delta }

1. TOC
{:toc}

---

## Requisitos

- **Visual Studio 2022 o posterior** con la carga de trabajo "Desarrollo para el escritorio con C++"
- **CMake** 3.16+
- **Git** (para los submódulos y FetchContent de wxWidgets)

## Compilación rápida

```bash
git clone --recursive https://github.com/SeiyaFunaokaJP/A2DP-Windows-Bridge.git
cd A2DP-Windows-Bridge
cmake -B build -A x64
cmake --build build --config Release
```

El ejecutable se genera en `build/app/Release/A2DPWB.exe`.

{: .note }
La primera compilación tarda varios minutos, porque CMake FetchContent descarga y compila wxWidgets (v3.2.6).

## Si ya clonó sin `--recursive`

```bash
git submodule update --init --recursive
```

## Estructura del proyecto

```
A2DP-Windows-Bridge/
├── app/                    Código fuente de la aplicación
│   ├── src/                Archivos fuente C++
│   ├── lang/               Localización (en.json es el original; tools/check_lang.py comprueba los demás)
│   ├── resources/          Icono, manifiesto, script de recursos
│   └── CMakeLists.txt      Configuración de compilación de la aplicación
├── cmake/                  Empaquetado de versiones (make_dist.cmake)
├── installer/              Script de Inno Setup para el instalador (A2DPWB.iss)
├── compat/                 Encabezados de compatibilidad MSVC para el código de AOSP
├── docs/                   Documentación (este sitio)
├── extern/                 Bibliotecas de terceros (submódulos de git)
│   ├── btstack/            Pila Bluetooth BTstack
│   ├── libldac/            Codificador LDAC de AOSP
│   ├── libopenaptx/        Codificador aptX / aptX HD / aptX LL
│   ├── fdk-aac/            Codificador AAC de Fraunhofer
│   └── json/               nlohmann/json (solo encabezados)
├── tools/
│   ├── a2dp_decode/        a2dpwb_decode: comprueba / decodifica el flujo multimedia de una captura HCI (.pklg)
│   ├── emu/                Prueba de extremo a extremo frente a un receptor Bluetooth virtual (Python)
│   └── linux_sink/         a2dpwb_sink: receptor A2DP de medición en Linux (Python, Bumble)
├── CMakeLists.txt          Configuración de compilación raíz
└── build.bat.example       Script auxiliar de compilación (cópielo como build.bat y ajuste la ruta de CMake)
```

## Opciones de compilación

| Opción de CMake | Predeterminado | Descripción |
|:-------------|:--------|:------------|
| `LDAC_SOFT_FLOAT` | `OFF` | Usar coma flotante por software para libldac |
| `A2DPWB_BUILD_TOOLS` | `ON` | Compilar las herramientas para desarrolladores (`a2dpwb_decode`, consulte [Uso](usage#verify-stream)). No forman parte del paquete de versión |

```bash
cmake -B build -A x64 -DA2DPWB_BUILD_TOOLS=OFF
```

## Paquetes de versión

```bash
cmake --build build --target A2DPWB_dist --config Release
```

genera `build/dist/A2DPWB-<version>-win64-portable.zip` y, si está instalado [Inno Setup 6](https://jrsoftware.org/isinfo.php) (`winget install JRSoftware.InnoSetup`; después vuelva a ejecutar la configuración de CMake), `build/dist/A2DPWB-<version>-win64-setup.exe` a partir de `installer/A2DPWB.iss`. Ambos contienen los mismos archivos.

## Pruebas sin hardware (tools/emu) {#emu}

`tools/emu` transmite todos los códecs desde A2DPWB a un receptor Bluetooth virtual en el mismo PC, sin adaptador ni auriculares. Solo funciona en Windows y, además de los requisitos de compilación, necesita **Python 3.11 o posterior**.

```bash
python tools/emu/setup_env.py     # una vez: crea tools/emu/.venv con paquetes de versión fija
python tools/emu/run_test.py      # tras una compilación Release; unos 35 s para todos los códecs
```

Cómo funciona:

- `emu_sink.py` ejecuta dos controladores virtuales de [Bumble](https://github.com/google/bumble) sobre un enlace dentro del proceso. Uno se ofrece como H4 por TCP (`127.0.0.1`) para A2DPWB. El otro lo usa un receptor A2DP de Bumble que ofrece SBC, AAC, aptX, aptX HD, aptX LL y LDAC, y que registra su tráfico HCI.
- `run_test.py` ejecuta `A2DPWB.exe --cli` para cada códec con opciones de desarrollo: `--hci-tcp` (controlador virtual en lugar de WinUSB), `--test-tone` (1 kHz a la izquierda / 1.5 kHz a la derecha en lugar del audio del sistema), `--duration`, `--hci-capture`. La variable de entorno `A2DPWB_CONFIG_DIR` hace que A2DPWB use una carpeta de configuración aparte, de modo que el emparejamiento con el receptor virtual nunca toca sus claves de enlace reales.
- Después, ambas capturas HCI se comprueban con `a2dpwb_decode` (la del receptor con `--received`).

Un códec supera la prueba cuando se cumple todo esto:

- se negoció el códec solicitado
- `a2dpwb_decode` no encuentra problemas ni en el lado emisor ni en el receptor
- el receptor recibió todos los paquetes multimedia y tramas que envió A2DPWB, y se envió al menos el 90 % de la duración de audio solicitada
- salvo en LDAC (sin decodificador de código abierto): el audio decodificado es idéntico en ambos lados y es el tono de prueba

| Opción | Descripción |
|:-------|:------------|
| `--codecs sbc aac ...` | Probar solo estos códecs (`sbc aac aptx aptxhd aptxll ldac`, predeterminado: todos) |
| `--duration <s>` | Segundos de transmisión por códec (predeterminado 5) |
| `--max-packet <bytes>` | Se pasa a A2DPWB (tamaño máximo del paquete multimedia, consulte [Uso](usage)) |
| `--build-dir`, `--config` | Dónde buscar `A2DPWB.exe` / `a2dpwb_decode.exe` (predeterminado `build`, `Release`) |
| `--out <dir>` | Carpeta de salida (predeterminado `tools/emu/out`) |

Para cada códec, `tools/emu/out/<codec>/` contiene las capturas (`a2dpwb.pklg`, `sink.pklg`), los registros, los informes de `a2dpwb_decode` y los archivos WAV decodificados. El código de salida es 0 cuando todos los códecs superan la prueba.

{: .note }
El enlace virtual no tiene radio: comprueba la codificación, la paquetización, la señalización AVDTP / L2CAP y la interoperabilidad con una pila Bluetooth independiente (Bumble), pero no las condiciones de radio, la temporización en hardware real ni cómo reproducen el audio unos auriculares reales. Los paquetes de Bumble son solo herramientas de prueba y no forman parte de A2DPWB (consulte THIRD_PARTY_LICENSES.md §13).

`emu_vhci.py` hace lo mismo con la pila Bluetooth de Linux: en Linux conecta un adaptador virtual a BlueZ a través de `/dev/vhci`, que el receptor de medición `tools/linux_sink` toma después como si fuera un adaptador real; así se puede probar sin hardware, opcionalmente con paquetes multimedia perdidos o retenidos (`--drop`, `--stall`). Consulte `tools/linux_sink/README.md`.

## Dependencias

Todas las dependencias se incluyen como submódulos de git o se descargan durante la compilación. No hace falta instalar nada a mano.

| Biblioteca | Método | Versión fijada | Licencia |
|:--------|:-------|:---------------|:--------|
| BTstack | submódulo de git | v1.8.1-6-g5bc5cbdbe | Licencia de BTstack (tipo BSD-3-Clause con una cláusula no comercial) |
| Códec SBC de Bluedroid | incluido en BTstack (`3rd-party/bluedroid`) | la de BTstack | Apache-2.0 |
| rijndael | incluido en BTstack (`3rd-party/rijndael`) | la de BTstack | Dominio público |
| libldac (AOSP) | submódulo de git | android-15.0.0_r36-4-geeee1a3 | Apache-2.0 |
| libopenaptx | submódulo de git | 0.2.0 (no actualizar) | LGPL-2.1+ |
| fdk-aac | submódulo de git | v2.0.3-158-gd8e6b1a | FDK AAC License |
| nlohmann/json | submódulo de git (solo encabezados) | 3.12.0 | MIT |
| wxWidgets | CMake FetchContent | v3.2.6 | wxWindows Library Licence 3.1 |
| zlib / libpng / nanosvg | copias integradas en wxWidgets | 1.2.13.1 / 1.6.37 / la de wxWidgets | zlib / PNG Reference Library License v2 / zlib |

{: .warning }
Como BTstack se enlaza en el programa, cualquier `A2DPWB.exe` que compile solo puede usarse y redistribuirse con fines personales y no comerciales, aunque el código fuente de A2DPWB en sí tenga licencia dual Apache-2.0 o MIT.

Consulte [THIRD_PARTY_LICENSES.md](https://github.com/SeiyaFunaokaJP/A2DP-Windows-Bridge/blob/main/THIRD_PARTY_LICENSES.md) para ver todos los detalles de las licencias.
