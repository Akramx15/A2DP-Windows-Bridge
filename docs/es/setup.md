---
title: Instalación
layout: default
parent: Inicio
nav_order: 1
---

# Instalación
{: .no_toc }

## Contenido
{: .no_toc .text-delta }

1. TOC
{:toc}

---

## Requisitos previos

- **Windows 10/11** (x64)
- **Adaptador Bluetooth USB dedicado** (aparte del Bluetooth integrado)
- **Zadig** ([https://zadig.akeo.ie/](https://zadig.akeo.ie/))

## Instalar A2DPWB

Cada [versión](https://github.com/SeiyaFunaokaJP/A2DP-Windows-Bridge/releases/latest) se publica en dos formatos con los mismos archivos:

- **Instalador** (`A2DPWB-<version>-win64-setup.exe`): instala para el usuario actual sin derechos de administrador (o para todos los usuarios, si se elige), añade una entrada en el menú Inicio y un desinstalador y, en la primera instalación, inicia A2DPWB en el idioma elegido durante la instalación
- **Portable** (`A2DPWB-<version>-win64-portable.zip`): extráigalo en cualquier carpeta y ejecute `A2DPWB.exe`

Ambos guardan la configuración, los perfiles, las claves de enlace y el firmware en `%APPDATA%\A2DPWB`, así que puede cambiar de uno a otro. La desinstalación no elimina esa carpeta.

## Paso 1: Instalar el controlador WinUSB

A2DPWB se comunica directamente con un adaptador Bluetooth USB mediante WinUSB. Necesita un **adaptador dedicado**: el Bluetooth integrado sigue funcionando con normalidad para el Bluetooth habitual de Windows.

1. Conecte un adaptador Bluetooth USB
2. Descargue y abra [Zadig](https://zadig.akeo.ie/)
3. Vaya a **Options > List All Devices**
4. Seleccione su adaptador Bluetooth USB en la lista desplegable
5. Seleccione **WinUSB** como controlador de destino
6. Haga clic en **Replace Driver**

{: .warning }
Mientras el adaptador está en modo WinUSB, Windows no puede usarlo para el Bluetooth normal. Use el Bluetooth integrado para los periféricos habituales (teclado, ratón, etc.).

## Paso 2: Averiguar la dirección Bluetooth de los auriculares

Necesita la dirección MAC Bluetooth de sus auriculares/altavoces.

**Desde la Configuración de Windows:**
1. Abra **Configuración > Bluetooth y dispositivos**
2. Haga clic en su dispositivo de audio
3. Haga clic en **Propiedades**
4. Se muestra la dirección Bluetooth (formato: `AA:BB:CC:DD:EE:FF`)

**Desde la GUI de A2DPWB:**
- Inicie `A2DPWB.exe`: los dispositivos de audio Bluetooth emparejados aparecen en la lista desplegable de dispositivos
- O use **Buscar dispositivos** en el diálogo del perfil para encontrar dispositivos cercanos con el adaptador USB (ponga los auriculares en modo de emparejamiento)

**Desde la CLI:**
```
A2DPWB.exe --cli -l
```

## Paso 3: Ejecutar A2DPWB

**GUI:**
```
A2DPWB.exe
```

**CLI:**
```
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF
```

Consulte [Uso](usage) para ver todos los detalles.

---

## Firmware (adaptadores Realtek)

Los adaptadores Bluetooth USB basados en Realtek (p. ej., TP-Link UB500, llaves RTL8761BU) necesitan firmware propietario para funcionar. Los adaptadores CSR no necesitan firmware. Los adaptadores Intel y Broadcom (experimentales) pueden necesitar sus propios archivos de firmware: consulte [Adaptadores recomendados](#adapters).

### GUI (descarga guiada)

Inicie `A2DPWB.exe` y elija **Acciones > Selección de firmware Bluetooth**. Si falta el firmware, se muestra una advertencia. Use el botón **Abrir página de descarga** para abrir la página [linux-firmware/rtl_bt](https://git.kernel.org/pub/scm/linux/kernel/git/firmware/linux-firmware.git/tree/rtl_bt) en el navegador y, después, **Abrir carpeta de configuración** para abrir la carpeta de destino. Descargue los archivos `.bin` necesarios y colóquelos en la carpeta de configuración.

El diálogo detecta automáticamente el chipset del adaptador y muestra qué archivos de firmware se necesitan.

### Opción Chip Bluetooth

La lista **Chip Bluetooth** del mismo diálogo decide si A2DPWB ejecuta la inicialización de Realtek (carga del firmware) en el adaptador:

| Opción | Significado |
|:-------|:--------|
| **Automático** (predeterminado) | Se decide según el adaptador detectado cada vez que se inicia A2DPWB. Un adaptador Realtek conocido recibe su chip; cualquier otro adaptador no recibe inicialización de Realtek |
| **No Realtek** | No ejecutar nunca la inicialización de Realtek. Úsela para adaptadores Intel, Broadcom, CSR y otros si Automático no reconoce el suyo |
| **RTL8761BU**, **RTL8822CU**, ... | Ese chip Realtek. Necesario para un adaptador basado en Realtek que Automático no reconoce (un producto OEM con su propio VID:PID). La línea de estado indica si sus archivos `.bin` están en la carpeta de configuración |
| **Custom** | Usa `custom_fw.bin` y `custom_config.bin` de la carpeta de configuración |

No elija un chip Realtek ni Custom para un adaptador que no sea Realtek: A2DPWB intentaría cargarle firmware de Realtek y es posible que el adaptador no se inicie.

### Descarga manual

1. Descargue los archivos `.bin` de firmware y de configuración de su chipset desde [linux-firmware/rtl_bt](https://git.kernel.org/pub/scm/linux/kernel/git/firmware/linux-firmware.git/tree/rtl_bt) (p. ej., `rtl8761bu_fw.bin` y `rtl8761bu_config.bin`)
2. Coloque ambos archivos en la carpeta de configuración de A2DPWB (`%APPDATA%\A2DPWB`, o la ruta que muestra el diálogo de firmware)

{: .note }
Estos archivos de firmware son binarios propietarios de Realtek distribuidos a través del proyecto linux-firmware. No se incluyen en este repositorio. Consulte [WHENCE](https://git.kernel.org/pub/scm/linux/kernel/git/firmware/linux-firmware.git/tree/WHENCE) para conocer las condiciones de redistribución.

---

## Adaptadores recomendados {#adapters}

| Chipset | Productos de ejemplo | Notas |
|:--------|:-----------------|:------|
| Realtek | TP-Link UB500, RTL8761BU | Requiere descargar firmware (ver arriba). El más probado |
| CSR | Llaves CSR8510 genéricas | Funcionan sin más, no necesitan firmware |
| Broadcom | ASUS BT400 (BCM20702) | *Experimental.* También se detectan los adaptadores que indican la clase de fabricante `FF/01/01` en lugar de la clase Bluetooth `E0/01/01` (llaves OEM BCM20702). Funciona con el firmware en ROM; un archivo PatchRAM `.hcd` opcional (p. ej. `BCM20702A1-0b05-17cb.hcd`) en la carpeta de configuración se carga al iniciar |
| Intel | Intel 8265 / 9260 / AX200 / AX201 | *Experimental.* En modo bootloader (p. ej. tras un arranque en frío) necesita los archivos `ibt-*.sfi` + `ibt-*.ddc` correspondientes de `intel/` de linux-firmware en la carpeta de configuración. AX210 y posteriores (bootloader TLV) aún no son compatibles |
| MediaTek, Qualcomm | MT7921/MT7922, QCA61x4 | No compatibles: necesitan cargadores de firmware que A2DPWB no tiene |

Por lo general, los adaptadores USB Bluetooth 5.0+ funcionan mejor con códecs de alta tasa de bits como LDAC.
