---
title: Uso
layout: default
parent: Inicio
nav_order: 2
---

# Uso
{: .no_toc }

## Contenido
{: .no_toc .text-delta }

1. TOC
{:toc}

---

## Modo GUI (predeterminado)

```
A2DPWB.exe
```

La interfaz gráfica ofrece:

- **Selección del adaptador Bluetooth** -- elija qué adaptador WinUSB usar
- **Selección del dispositivo de audio** -- elija el origen de captura de loopback WASAPI
- **Dirección del dispositivo** -- escríbala a mano, elíjala entre los dispositivos emparejados o use **Buscar dispositivos** en el diálogo del perfil (dispositivos cercanos, encontrados con el adaptador USB). Las direcciones escritas o pegadas se formatean al vuelo: `AA:BB:CC:DD:EE:FF`, `aa-bb-cc-dd-ee-ff`, `AABBCCDDEEFF` y `AA BB CC DD EE FF` se convierten en `AA:BB:CC:DD:EE:FF`
- **Selección de códec** -- Automático / LDAC / aptX HD / aptX LL / aptX / SBC / AAC
- **Calidad de LDAC** -- HQ (990 kbps) / SQ (660 kbps) / MQ (330 kbps)
- **Calidad de LHDC V5** -- Alta (1000 kbps) / Media-alta (900 kbps) / Estándar (500 kbps) / Móvil (400 kbps), los cuatro niveles que ofrece Android
- **ABR** -- activación de la tasa de bits adaptativa para conexiones inestables (se muestra con LDAC y LHDC V5)
- **Gestión de perfiles** -- guarde y cargue configuraciones de dispositivo + códec
- **Estado en tiempo real** -- códec, tasa de bits, estado de la conexión
- **Calidad del enlace** -- **Acciones > Calidad del enlace...** (consulte [Ventana de calidad del enlace y AFH](#link-quality))
- **Menú Configuración** -- Idioma, Tema, Iniciar con Windows, Minimizar en la bandeja del sistema, Buscar actualizaciones al iniciar, Incluir versiones preliminares al buscar actualizaciones (también ofrece versiones beta), Botones de los auriculares controlan la reproducción (consulte [Botones de los auriculares](#headphone-buttons)), Recordar volumen de los auriculares (consulte [Volumen](#volume)), Modo de depuración (añade el menú **Depurar**: consola de depuración, captura HCI, prueba con receptor par)

## Modo CLI

Añada `--cli` para ejecutar sin ventana de GUI.

### Básico

```bash
# Seleccionar automáticamente el mejor códec
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF
```

### Selección de códec

```bash
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF -c ldac      # LDAC
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF -c aptxhd    # aptX HD
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF -c aptxll    # aptX Low Latency
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF -c aptx      # aptX
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF -c aac       # AAC
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF -c sbc       # SBC
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF -c lhdcv5    # LHDC V5 (experimental)
```

Prioridad de la selección automática: LDAC > aptX HD > aptX LL > aptX > LHDC V5 (experimental) > AAC > SBC

Si los auriculares no ofrecen el códec solicitado, la CLI recurre al orden de prioridad de Automático. aptX Adaptive nunca se selecciona: consulte [Familia aptX y compatibilidad con aptX Adaptive](#aptx-compatibility).

### Calidad de LDAC

```bash
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF -c ldac -q hq   # 990 kbps (predeterminado)
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF -c ldac -q sq   # 660 kbps
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF -c ldac -q mq   # 330 kbps

# LHDC V5: hq 1000 / mhq 900 / sq 500 / mq 400 kbps (los cuatro niveles de Android)
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF -c lhdcv5 -q mhq  # 900 kbps
```

### ABR (tasa de bits adaptativa)

```bash
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF -c ldac -a
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF -c lhdcv5 -a
```

ABR reduce automáticamente la tasa de bits cuando la conexión Bluetooth es inestable y la vuelve a subir cuando las condiciones mejoran. LDAC cambia entre sus modos de calidad; LHDC V5 sigue la política de Android y se mantiene entre 160 y 400 kbps.

### Detección de dispositivos

```bash
# Mostrar los dispositivos de audio Bluetooth emparejados
A2DPWB.exe --cli -l
```

{: .note }
La lista de dispositivos usa la API Bluetooth de Windows a través del adaptador Bluetooth **integrado**, no del adaptador WinUSB.

### Todas las opciones

| Opción | Descripción |
|:-------|:------------|
| `-d <addr>` | Dirección Bluetooth de los auriculares (`AA:BB:CC:DD:EE:FF`) |
| `-c <codec>` | `ldac`, `aptxhd`, `aptxll`, `aptx`, `aac`, `sbc`, `auto` (predeterminado: `auto`) |
| `-q <mode>` | Calidad: `hq` (predeterminado), `sq`, `mq`; LHDC V5 también `mhq` (900 kbps) |
| `-a` | ABR (LDAC y LHDC V5) |
| `-m <mode>` | Modo de captura: `loopback` (predeterminado), `virtual` |
| `--audio-device <id>` | ID del dispositivo de audio para `-m virtual` |
| `-u <path>` | Ruta del dispositivo USB del adaptador (si hay varios conectados) |
| `-l` | Mostrar los dispositivos de audio Bluetooth emparejados y salir |
| `--max-packet <bytes>` | Tamaño máximo del paquete multimedia, 679-1679 (consulte [Tamaño máximo del paquete multimedia](#max-packet)) |
| `--afh <auto\|off\|6,11>` | Clasificación de canales AFH del host (consulte [Ventana de calidad del enlace y AFH](#link-quality)) |
| `--remote-sink <host[:port]\|auto>` | Mostrar lo que recibe el receptor par (consulte [Prueba con receptor par](#receiver)) |
| `-h` | Mostrar la ayuda |

Para desarrollo y pruebas también existen `--hci-tcp <host:port>` (un controlador virtual por TCP en lugar del adaptador WinUSB), `--test-tone` (1 kHz a la izquierda / 1.5 kHz a la derecha en lugar del audio capturado), `--duration <s>` (detenerse tras ese número de segundos) y `--hci-capture <file.pklg>` (captura HCI de toda la ejecución); consulte [Compilación](building#emu).

En modo GUI, `--minimized` inicia A2DPWB en la bandeja del sistema (lo usa **Iniciar con Windows**).

## Modos de captura {#capture-modes}

A2DPWB admite tres modos de captura de audio. Cada uno graba un punto distinto del recorrido del audio:

| Modo | Qué graba |
|:-----|:----------------|
| Loopback del sistema | Lo que reproduce el dispositivo de salida predeterminado de Windows: todas las aplicaciones mezcladas |
| Dispositivo virtual | Lo que las aplicaciones reproducen en el dispositivo seleccionado (p. ej., VB-CABLE) |
| Aplicación | Solo lo que reproduce una aplicación (y sus procesos secundarios), después del procesamiento propio de la aplicación. Requiere Windows 11 o Windows 10 versión 2004 (compilación 19041) o posterior |

El diálogo del perfil muestra la misma explicación en **Origen de captura**.

El Loopback del sistema copia el audio; no se lo quita al dispositivo de salida predeterminado. El audio sigue sonando en ese dispositivo (altavoces, un televisor HDMI, ...) además de en los auriculares Bluetooth. Para oírlo solo en los auriculares, use el modo Dispositivo virtual con un dispositivo que no reproduzca en ningún sitio, como VB-CABLE.

En el modo Dispositivo virtual, **Cambiar dispositivo predeterminado** (activado de forma predeterminada) convierte el dispositivo seleccionado en la salida predeterminada de Windows durante la transmisión, de modo que las aplicaciones reproducen en él sin cambiar la configuración de Windows. Al detener la transmisión, A2DPWB vuelve al dispositivo predeterminado anterior.

- La lista **Dispositivo de audio** muestra solo los dispositivos que A2DPWB detecta como dispositivos de audio virtuales.
- El dispositivo de comunicación predeterminado (el que usan las aplicaciones de llamadas) no se cambia.
- Algunas aplicaciones siguen reproduciendo en el dispositivo anterior hasta que se reinician.
- Si A2DPWB se cierra inesperadamente, el dispositivo predeterminado no se restaura. Cámbielo en la configuración de Sonido de Windows.

### Modo Aplicación {#application-mode}

El modo Aplicación envía el audio de una aplicación, por ejemplo un reproductor de música, un navegador o una aplicación de efectos. Elija la aplicación en la lista **Aplicación**. La lista muestra las aplicaciones que están reproduciendo audio en ese momento; si la suya no aparece, inicie la reproducción en ella y haga clic en **Actualizar**. Se incluyen los procesos secundarios de la aplicación, así que también funciona un navegador que reproduce el audio desde un proceso aparte.

- No es necesario que la aplicación esté en ejecución. A2DPWB se conecta igualmente a los auriculares y empieza a enviar en cuanto la aplicación se inicia. Si la aplicación se cierra durante la transmisión, A2DPWB sigue conectado y la vuelve a captar cuando se reinicia.
- La aplicación sigue sonando también en su propio dispositivo de salida. Para oírla solo en los auriculares, silencie ese dispositivo de salida (su volumen maestro): esto no afecta a lo que captura A2DPWB. Pero no silencie la aplicación en el Mezclador de volumen de Windows, porque los auriculares también recibirían silencio.
- El control deslizante de la aplicación en el Mezclador de volumen de Windows cambia el nivel enviado a los auriculares (y el de su dispositivo de salida). El volumen maestro y las teclas de volumen no cambian el audio enviado; con **Sincronizar volumen de Windows** del perfil en Automático, cambian en su lugar el volumen de los auriculares (consulte [Volumen](#volume)).
- Windows convierte el audio a la frecuencia de muestreo que pide A2DPWB, así que el formato del dispositivo en la configuración de Sonido no lo limita.
- En Windows 10 anterior a la versión 2004 (compilación 19041), Windows no permite capturar por aplicación. El diálogo del perfil indica que Aplicación no está disponible en ese PC, y al conectar con un perfil así se muestra un error.

### Volumen {#volume}

Durante la transmisión, la ventana principal muestra un control deslizante **Volumen** cuando los auriculares admiten el volumen absoluto de AVRCP. Ajusta el volumen en los propios auriculares, de modo que el audio enviado conserva toda su resolución. El control deslizante también sigue los botones de volumen y los controles táctiles de los auriculares.

**Sincronizar volumen de Windows** en el perfil hace que un volumen de Windows y el volumen de los auriculares sean uno solo durante la transmisión:

| Opción | Volumen de Windows sincronizado |
|:-------|:----------------------|
| Automático (predeterminado) | El dispositivo que captura A2DPWB: el dispositivo de salida predeterminado en los modos Loopback del sistema y Aplicación, el dispositivo seleccionado en el modo Dispositivo virtual |
| Desactivado | Ninguno: el volumen de Windows y el de los auriculares son independientes |
| Un dispositivo de salida | El volumen de ese dispositivo |

- Los botones y gestos de volumen de los auriculares, y el control deslizante **Volumen**, mueven el volumen de Windows sincronizado, y su control deslizante en la configuración de Sonido mueve el volumen de los auriculares.
- Las teclas de volumen y el control flotante de volumen de Windows siempre actúan sobre el dispositivo de salida predeterminado, así que solo cambian el volumen de los auriculares cuando el dispositivo de salida predeterminado es el sincronizado (como con Automático, salvo en el modo Dispositivo virtual sin **Cambiar dispositivo predeterminado**).
- Al iniciar la transmisión, el volumen de Windows pasa a ser el de los auriculares; al detenerla, el dispositivo recupera el volumen que tenía antes.
- Con Automático, el audio se captura antes de aplicar el volumen de Windows, por lo que se reduce una sola vez, en los auriculares. Si el volumen sincronizado también cambia el audio capturado, el volumen se aplica dos veces: en ese caso elija Desactivado (consulte [Aplicaciones de efectos para todo el sistema](#effects-apps)).
- El silencio no se sincroniza: silenciar el dispositivo de salida deja el PC en silencio, pero no silencia los auriculares.

**Configuración > Recordar volumen de los auriculares** (desactivado de forma predeterminada) guarda el volumen de los auriculares para cada dispositivo y lo vuelve a aplicar en cuanto se conectan, antes de enviar audio. De lo contrario, los auriculares empiezan con su propio último volumen, que puede ser mucho más alto que el usado la última vez con A2DPWB (p. ej., después de usarlos con un teléfono).

### Botones de los auriculares {#headphone-buttons}

Los controles de reproducir/pausar, pista siguiente y pista anterior de los auriculares (botones o gestos táctiles, p. ej. un doble toque en los WH-1000XM4) controlan la reproducción en el PC durante la transmisión: A2DPWB los pasa a Windows como teclas multimedia, que llegan a la aplicación que aparece en los controles multimedia de Windows (Spotify, un navegador, un reproductor...). Detener también se transmite.

- Reproducir y pausar actúan ambos como la tecla Reproducir/Pausar de Windows, de modo que un toque siempre alterna entre reproducción y pausa, aunque la idea que tienen los auriculares del estado no esté actualizada.
- A2DPWB indica a los auriculares si se está reproduciendo audio (según haya o no audio que capturar), para que los auriculares que se pausan al quitárselos envíen el comando correcto.
- Avance rápido / retroceso (mantener pulsado un botón) no tienen tecla multimedia en Windows y se ignoran.
- Los gestos de volumen cambian el volumen propio de los auriculares; con **Sincronizar volumen de Windows**, el volumen de Windows sincronizado los sigue (consulte [Volumen](#volume)).
- Desactívelo con **Configuración > Botones de los auriculares controlan la reproducción**, p. ej. si otro programa ya reacciona a las teclas multimedia y estas actúan dos veces.

Esto requiere AVRCP en los auriculares, que casi todos tienen. Sin él, la transmisión funciona como siempre, solo que sin los botones.

### Aplicaciones de efectos para todo el sistema {#effects-apps}

Una aplicación de efectos para todo el sistema (un ecualizador, un potenciador de sonido, una aplicación de sonido envolvente virtual, ...) suele instalar su propio dispositivo de salida virtual. Las demás aplicaciones reproducen en ese dispositivo; la aplicación de efectos procesa el audio y reproduce el resultado en el dispositivo de salida que usted elige en ella. El modo de A2DPWB que elija decide si los auriculares reciben los efectos:

| Configuración | Qué llega a los auriculares |
|:--------|:----------------------------|
| Dispositivo virtual: el dispositivo virtual propio de la aplicación de efectos | El audio **antes** de los efectos: lo que las aplicaciones envían a la aplicación de efectos |
| Aplicación: la aplicación de efectos | El audio **después** de los efectos: lo que reproduce la aplicación de efectos (recomendado) |
| Dispositivo virtual: el dispositivo de salida de la aplicación de efectos (p. ej., VB-CABLE configurado como su salida) | El audio después de los efectos (para versiones de Windows sin modo Aplicación) |

En el modo Aplicación, el dispositivo de salida de la aplicación de efectos puede ser cualquiera: A2DPWB toma lo que reproduce la aplicación, no lo que reproduce el dispositivo, así que el dispositivo solo es un destino para el audio. Para que el PC no reproduzca el audio al mismo tiempo, elija una salida que nadie escuche (p. ej., el audio HDMI de un monitor) o silencie el volumen maestro de ese dispositivo. Un dispositivo virtual como VB-CABLE también sirve como destino silencioso, pero no es necesario. El dispositivo debe estar habilitado y conectado.

Si el dispositivo virtual propio de la aplicación de efectos es el dispositivo de salida predeterminado de Windows, su volumen también reduce lo que recibe la aplicación de efectos. Con **Sincronizar volumen de Windows** en Automático, un cambio de volumen se aplica entonces dos veces (en la entrada de la aplicación de efectos y en los auriculares). Póngalo en Desactivado en ese perfil y use el volumen de los auriculares o el control deslizante **Volumen**.

#### Por qué A2DPWB no tiene su propio dispositivo de salida virtual {#no-virtual-device}

Una entrada de auriculares Bluetooth en la lista de dispositivos de salida de Windows, que una aplicación de efectos pudiera seleccionar directamente, requeriría un controlador de audio en modo kernel. A2DPWB no incluye ninguno:

- Escribir y mantener un controlador de audio de kernel es un trabajo enorme, y un error en él puede bloquear Windows (pantalla azul).
- Windows 10 y 11 solo cargan controladores firmados por Microsoft. Obtener esa firma requiere un certificado de firma de código EV (varios cientos de dólares estadounidenses al año) y una cuenta de Microsoft Partner Center, normalmente registrada como empresa.
- La única forma de evitar la firma es el modo de firma de prueba de Windows, que reduce la seguridad del sistema y hace fallar algunos programas (p. ej., juegos con antitrampas). A2DPWB no va a depender de él.

El modo Aplicación, o un dispositivo virtual gratuito como VB-CABLE, cubre el mismo uso.

- Windows no tiene una API pública para cambiar el dispositivo predeterminado. A2DPWB usa la interfaz no documentada IPolicyConfig, como otras herramientas de cambio de audio (Windows 10 y 11).

{: .warning }
Ambos modos usan el modo compartido de WASAPI, por lo que la frecuencia de muestreo de captura la determina el **Formato predeterminado** del dispositivo en la configuración de Sonido de Windows (normalmente 48 kHz). Para usar LDAC a 96 kHz, cambie el formato del dispositivo de salida a 96 kHz en **Configuración de sonido > Propiedades del dispositivo > Opciones avanzadas > Formato predeterminado**.

## Comparación de códecs

| Códec | Ideal para | Inconveniente |
|:------|:---------|:----------|
| LDAC (HQ) | Máxima calidad de audio | Mayor latencia, necesita una conexión estable |
| LDAC (ABR) | Calidad + estabilidad | La tasa de bits varía con la conexión |
| aptX HD | Alta calidad, mayor compatibilidad de dispositivos | 576 kbps fijos |
| aptX LL | Juegos, vídeo (baja latencia) | Menor calidad de audio (16 bits) |
| aptX | La mayor compatibilidad entre dispositivos aptX | 16 bits, 352/384 kbps |
| AAC | Dispositivos Apple, buena eficiencia | Calidad moderada |
| SBC | Máxima compatibilidad | La menor calidad de audio |

## Familia aptX y compatibilidad con aptX Adaptive {#aptx-compatibility}

A2DPWB admite tres códecs aptX, todos codificados con [libopenaptx](https://github.com/pali/libopenaptx). aptX Adaptive **no** es compatible.

{: .warning }
**aptX HD y aptX Low Latency son experimentales.** Siguen las implementaciones de Android / PipeWire y superan las pruebas de ida y vuelta de codificación/decodificación, pero aún no se han verificado con auriculares reales.

{: .note }
**¿Solo necesita aptX clásico?** Windows 10 ya admite aptX clásico en su pila Bluetooth integrada (no aptX HD, aptX LL ni aptX Adaptive), así que A2DPWB no es necesario para ese códec. A2DPWB es útil sobre todo para LDAC, aptX HD y aptX Low Latency.

| Códec | Vendor ID / Codec ID | Encabezado RTP | Estado |
|:------|:---------------------|:-----------|:-------|
| aptX | 0x0000004F / 0x0001 | No | Compatible (352/384 kbps a 44.1/48 kHz) |
| aptX HD | 0x000000D7 / 0x0024 | Sí | Compatible (576 kbps, entrada de 24 bits) |
| aptX Low Latency | 0x0000000A o 0x000000D7 / 0x0002 | No | Compatible (352 kbps) |
| aptX Adaptive | 0x000000D7 / 0x00AD | -- | **No compatible** -- solo se detecta y se registra |

### Por qué aptX Adaptive no es compatible

No existe ningún codificador aptX Adaptive de código abierto. libopenaptx implementa solo aptX y aptX HD (A2DPWB también lo usa para aptX Low Latency). [arkq/openaptx](https://github.com/arkq/openaptx) solo reimplementa aptX / aptX HD, no contiene un codificador aptX Adaptive y no permite redistribuir binarios. Por eso A2DPWB solo detecta aptX Adaptive y lo escribe en el registro.

### No hay «recurso» de aptX Adaptive a aptX

Cuando A2DPWB se conecta, los auriculares anuncian una **lista de códecs** (extremos de transmisión AVDTP). A2DPWB elige un códec de esa lista. aptX Adaptive simplemente nunca es candidato, así que nada «recurre» desde él:

- **Los auriculares también ofrecen aptX clásico** -- Muchos auriculares aptX Adaptive lo hacen, porque Qualcomm promociona aptX Adaptive como retrocompatible con aptX / aptX HD. En ese caso A2DPWB selecciona directamente aptX clásico; es un códec independiente en la lista de los auriculares, no un modo reducido de aptX Adaptive.
- **Los auriculares solo ofrecen aptX Adaptive** (sin aptX clásico, aptX HD ni aptX LL) -- aptX no es posible con A2DPWB. Automático elige el siguiente códec en el orden de prioridad, normalmente AAC o SBC.

Prioridad de la selección automática: LDAC > aptX HD > aptX LL > aptX > LHDC V5 (experimental) > AAC > SBC

### El códec seleccionado no está disponible {#codec-fallback}

Cuando solicita un códec concreto que los auriculares no ofrecen:

| Modo | Comportamiento |
|:-----|:---------|
| GUI, **Usar otro códec si este no está disponible** marcado (predeterminado) | Se conecta con el mejor códec que ofrecen los auriculares, en el orden de prioridad de Automático, y el estado indica qué códec se usa en su lugar y por qué. |
| GUI, opción sin marcar | La conexión se detiene con un error («El dispositivo no admite ...»). Si solicitó aptX, aptX HD o aptX LL y los auriculares solo ofrecen aptX Adaptive, el mensaje lo indica y sugiere Automático, AAC o SBC. |
| CLI (`-c <codec>`) | Muestra `Requested codec ... not available, falling back...` y continúa con el orden de prioridad de Automático. Si los auriculares ofrecen aptX Adaptive, una línea adicional indica que nunca se selecciona. |

La opción está en **Editar perfil**, junto al códec (no se usa con Automático).

### Falta un códec mientras los auriculares están conectados a otro dispositivo {#sep-in-use}

Los auriculares ofrecen cada códec en un extremo de transmisión (SEP). En una conexión doble (multipunto), un extremo que transmite al teléfono o al PC se indica como **en uso**, y su códec no se puede usar hasta que esa transmisión se detenga. Algunos auriculares además solo ofrecen códecs de alta resolución (LDAC, L2HC, ...) mientras hay un único dispositivo conectado. A2DPWB informa entonces:

```
BTstack: Remote SEP SEID=6 sink audio, IN USE (codec not queried)
BTstack: Remote SEPs in use by another connection: 1 of 6 audio sink SEP(s); ...
BTstack: Capability discovery complete (LDAC=0, ..., 1 of 6 audio sink SEP(s) in use)
```

y el error indica que hay extremos en uso por otro dispositivo. Desconecte los auriculares del teléfono / PC (o desactive la conexión doble en la aplicación de los auriculares) y vuelva a conectar. Un PC cuyo Bluetooth integrado siga emparejado con los auriculares también cuenta como otro dispositivo.

### Frecuencia de muestreo

aptX, aptX HD y aptX LL solo admiten 44.1 kHz y 48 kHz. A2DPWB elige una de las frecuencias que anuncian los auriculares (con preferencia por la frecuencia actual del dispositivo de Windows; si no, 48 kHz) y deja que WASAPI remuestree la captura. No es necesario cambiar la configuración de Sonido de Windows para aptX.

### Comprobar qué ofrecen sus auriculares

1. En el menú **Configuración**, marque **Modo de depuración (debug.log / registro HCI)** y reinicie A2DPWB.
2. Conéctese a los auriculares.
3. Abra `debug.log` en la carpeta de configuración (`%APPDATA%\A2DPWB`). En modo CLI, las mismas líneas se escriben en la salida de error estándar.

En la GUI, **Depurar > Consola de depuración...** (Ctrl+Shift+D) muestra el mismo registro en directo. Su lista **Flujo de conexión** marca cada etapa del último intento de conexión (inicialización del adaptador, conexión ACL / AVDTP, detección de capacidades, selección de códec, WASAPI, configuración de la transmisión, codificador, inicio de la transmisión, envío de audio, desconexiones / reconexiones) como OK, Advertencia o Error; haga doble clic en un paso para saltar a la línea de registro correspondiente. El registro de abajo se puede filtrar por nivel y por texto. **Copiar diagnóstico** copia al portapapeles la versión, el sistema operativo, el adaptador, el perfil, el flujo, las advertencias / errores y el registro reciente, listo para pegar en un informe de errores (las direcciones Bluetooth se ocultan de forma predeterminada, salvo la parte del fabricante).

Las líneas de capacidades muestran exactamente qué códecs ofrecen los auriculares:

```
BTstack: Remote supports aptX (SEID=..., caps=0x..)
BTstack: Remote supports aptX Adaptive (SEID=...) — never selected (no open encoder); classic aptX is used only if the remote lists it
BTstack: Remote vendor codec vid=0x........ cid=0x.... (SEID=...) — unsupported
BTstack: Capability discovery complete (LDAC=0, aptXHD=0, aptXLL=0, aptX=1, aptXAdaptive=1 [not encodable], SBC=1, AAC=1)
```

- `aptX=1` -- aptX clásico está disponible (aunque también aparezca `aptXAdaptive=1`).
- `aptX=0`, `aptXHD=0`, `aptXLL=0` con `aptXAdaptive=1` -- los auriculares solo ofrecen aptX Adaptive; use Automático, AAC o SBC.
- `Remote vendor codec ... — unsupported` -- otro códec de fabricante que A2DPWB no implementa.

## Tamaño máximo del paquete multimedia (por perfil) {#max-packet}

**Tamaño máx. de paquete multimedia** en cada perfil (**Editar perfil**; CLI: `--max-packet <bytes>`) fija el límite superior de los datos de audio de cada paquete Bluetooth, de 679 a 1679 bytes, para que los auriculares que necesitan otro valor puedan tener el suyo. El límite propio de los auriculares (MTU) también se aplica siempre; A2DPWB usa el menor de los dos. El cambio se aplica a partir de la siguiente conexión. Los perfiles guardados por versiones anteriores toman el valor que antes se configuraba de forma global.

**Se recomienda 1023 (predeterminado).** Cabe en un paquete de banda base Bluetooth (3-DH5). Los valores mayores solo ahorran un 1-2 % de sobrecarga, y cada paquete perdido supone entonces perder más audio. 679 cabe en un paquete 2-DH5 y es el mínimo que acepta LDAC.

## Verificar el flujo multimedia (a2dpwb_decode) {#verify-stream}

La mayoría de los auriculares no pueden mostrar qué códec se está usando. En modo de depuración, la GUI tiene un menú **Depurar** con **Iniciar captura HCI** / **Detener captura HCI**. Una captura es un registro de paquetes HCI (`hci_<date>_<time>.pklg` en la carpeta de configuración) con la negociación del códec y todos los paquetes multimedia enviados. Con LDAC a 990 kbps crece aproximadamente 0.5 GB por hora, así que capture solo el tiempo que necesite.

Puede iniciar una captura antes de conectar o durante la transmisión. Si se inicia durante una conexión, A2DPWB escribe primero los paquetes de establecimiento de la conexión que ha recordado (establecimiento del canal L2CAP y señalización AVDTP, incluida la negociación del códec), para que el archivo pueda analizarse igualmente. Al detenerla, A2DPWB muestra la ruta del archivo.

Abra la captura en Wireshark o compruébela con la herramienta para desarrolladores `a2dpwb_decode` (se compila desde el código fuente; consulte [Compilación](building)):

```
a2dpwb_decode "%APPDATA%\A2DPWB\hci_20260925_120000.pklg" -o C:\temp\check
```

La herramienta sigue la señalización AVDTP (SET_CONFIGURATION / RECONFIGURE y si los auriculares la aceptaron), extrae los paquetes multimedia que envió A2DPWB y los comprueba frente a la configuración negociada:

| Códec | Qué se comprueba | Salida |
|:------|:----------------|:-------|
| SBC, AAC, aptX, aptX HD, aptX LL | Decodificación completa con los decodificadores de referencia; parámetros de trama frente a la configuración; número de tramas en el encabezado de la carga útil multimedia | `<prefix>.<n>.<codec>.wav` para escuchar |
| LDAC | Solo los encabezados de trama (palabra de sincronización, frecuencia de muestreo, modo de canal, longitud de trama), número de tramas, tasa de bits. No existe ningún decodificador LDAC de código abierto | Solo informe |

Para cada flujo también informa de los saltos en la secuencia RTP, la velocidad a la que avanza la marca de tiempo RTP, la tasa de bits y cuántos paquetes ACL indicó el controlador Bluetooth como completados. La última línea es `RESULT: OK` o `RESULT: PROBLEMS FOUND` (código de salida 0 o 1).

{: .note }
Esto muestra lo que envió A2DPWB y que es válido para el códec negociado. Como un receptor solo puede decodificar el códec negociado, un audio correcto en los auriculares junto con un informe sin errores es una prueba sólida de que el códec se está usando de verdad.

## Ventana de calidad del enlace y AFH {#link-quality}

**Acciones > Calidad del enlace...** muestra lo que envía A2DPWB durante la transmisión: códec y tasa de bits, paquetes enviados, paquetes descartados antes del envío (cola de envío llena: el enlace de radio no daba abasto), audio que el codificador no pudo procesar a tiempo, la cola de envío y **Radio**: el RSSI de la conexión, relativo al rango óptimo de recepción del controlador (0 = bien, negativo = demasiado débil; no es un nivel absoluto), y en cuántos de los 79 canales Bluetooth salta el enlace (AFH).

Bluetooth evita por sí mismo los canales ocupados (AFH), pero solo se entera de la presencia de Wi-Fi a través de los errores. A2DPWB además indica a su controlador qué canales debe evitar (clasificación de canales AFH del host), en cada conexión:

| `afh` | |
|:--|:--|
| `auto` (predeterminado) | Las redes Wi-Fi de 2.4 GHz que este PC recibe con fuerza (búsqueda de Wi-Fi de Windows, repetida mientras hay conexión) |
| `off` | Nada: el controlador decide por su cuenta |
| `6,11` | Estos canales Wi-Fi |

Siempre quedan en uso al menos 20 canales Bluetooth. Configúrelo con `"afh"` en `settings.json` (carpeta de configuración), en la ventana Prueba con receptor par (allí se guarda) o con `--afh` en modo CLI. La fila Radio muestra qué canales Wi-Fi se evitan. Que sirva o no depende del entorno: mídalo con la prueba con receptor par.

## Prueba con receptor par (tools/linux_sink) {#receiver}

La ventana de calidad del enlace muestra lo que envía A2DPWB. Para ver lo que llega realmente, ejecute `tools/linux_sink/a2dpwb_sink.py` en un segundo PC con Linux. Controla directamente el adaptador Bluetooth normal de ese PC (sirve el integrado) mediante Bumble sobre el canal de usuario HCI, como receptor A2DP para todos los códecs incluido LDAC, mide cada paquete multimedia que llega y ofrece las estadísticas por la red:

Copie la carpeta `tools/linux_sink` al PC con Linux (basta con la carpeta) y después:

```bash
sh setup.sh      # una vez: bibliotecas de decodificación, entorno de Python (Ubuntu / Debian, Python 3.11+)
sudo sh run.sh   # iniciar el receptor (root: toma el control del adaptador Bluetooth)
```

En modo de depuración, **Depurar > Prueba con receptor par...** encuentra por sí sola el receptor en la red local. **Iniciar** transmite un tono de prueba constante (o el audio del sistema) a su adaptador Bluetooth con el códec, la calidad, la frecuencia de muestreo y la profundidad de bits elegidos (sin perfil ni emparejamiento), y la ventana muestra lo enviado junto a lo recibido: tasa de bits, paquetes perdidos y tardíos (números de secuencia RTP), jitter y huecos, cortes y saltos de un búfer de reproducción modelado, errores de flujo / decodificación y el RSSI en el receptor (relativo a su rango óptimo de recepción: 0 = bien, negativo = demasiado débil). Cada paso (buscar receptor, estadísticas, ajustes del receptor, conexión Bluetooth, códec, llegada de audio) se muestra como OK / NG, con un registro que se puede copiar. En modo CLI, `--remote-sink auto` hace lo mismo: busca el receptor, transmite hacia él salvo que `-d` indique otro dispositivo, e imprime sus estadísticas cada 5 segundos y al final.

Ambos lados cuentan los paquetes de un flujo desde su primer paquete y, tras **Detener**, los recuentos finales siguen visibles («al detener»; los del receptor llegan aproximadamente un segundo después), de modo que se pueden comparar los paquetes enviados y recibidos: deberían coincidir. Bluetooth retransmite dentro de los controladores los paquetes de radio perdidos, y A2DPWB no abandona un paquete mientras el enlace está activo, así que un enlace de radio deficiente se manifiesta como huecos, jitter y cortes en el receptor, no como paquetes perdidos; cuando el enlace no da abasto, los paquetes se descartan antes del envío en el lado emisor. Ninguno de los dos lados puede conocer el número de retransmisiones.

Mientras la herramienta se ejecuta, la pila Bluetooth del propio receptor no puede usar su adaptador. La búsqueda usa una difusión UDP en el puerto 51201, así que ambos PC deben estar en el mismo segmento de red (si no, escriba la dirección). Consulte `tools/linux_sink/README.md` para ver todas las opciones y el protocolo de estadísticas.

## Solución de problemas

### El adaptador Bluetooth no responde {#adapter-no-answer}

Si el adaptador no devuelve nada después de abrirlo (no responde a HCI Reset), A2DPWB desiste a los 5 segundos, lo abre una vez más y, si sigue sin responder, muestra «El adaptador Bluetooth no responde». Desconecte el adaptador, vuelva a conectarlo e inténtelo de nuevo. No hace falta reiniciar A2DPWB: un inicio fallido libera el adaptador y cada nuevo inicio lo vuelve a inicializar.

Para evitar que un adaptador se quede bloqueado, A2DPWB restablece sus canalizaciones USB al abrirlo y envía HCI Reset al cerrarlo.

### Problemas de emparejamiento tras actualizar

A2DPWB empareja mediante SSP (Just Works) con **General Bonding**, y las claves de enlace se guardan para que los auriculares puedan reconectarse sin volver a emparejar. Si emparejó los auriculares con una compilación antigua de A2DPWB, elimine el emparejamiento de A2DPWB en los auriculares (consulte el manual de los auriculares para borrar la lista de emparejamientos) y vuelva a emparejarlos.

### Tiempos de espera de conexión

A2DPWB registra registros SDP (A2DP Source, AVRCP Controller / Target) para que los auriculares lo identifiquen como fuente de audio. Si un intento de conexión agota el tiempo de espera, la conexión semiabierta se cierra, de modo que puede volver a intentarlo en la misma sesión sin reiniciar A2DPWB. Asegúrese de que los auriculares estén encendidos, dentro del alcance y no conectados a otro dispositivo.

Una conexión fallida se reintenta automáticamente (hasta 3 intentos) cuando el enlace se estableció y luego no transmitió nada, y cuando los auriculares no respondieron. Si los auriculares ya no reconocen el emparejamiento (se restablecieron o se emparejaron con otro PC), se descarta la clave guardada y se vuelve a emparejar una vez. El error indica entonces qué ha ocurrido:

| Mensaje | Significado |
|:--|:--|
| El dispositivo no respondió | Apagado, fuera de alcance o no disponible para conexión (si es nuevo: modo de emparejamiento) |
| La conexión se estableció, pero no transmitió nada | Condiciones de radio: vuelva a intentarlo o cambie de sitio el adaptador / el dispositivo |
| El dispositivo respondió, pero luego cerró la conexión por sí mismo | Ahora no acepta conexiones: se está apagando, se está cargando o está conectado a otro dispositivo |
| Error de autenticación, también tras volver a emparejar | Ponga el dispositivo en modo de emparejamiento, o elimine su entrada de la lista de emparejamientos, y vuelva a intentarlo |
