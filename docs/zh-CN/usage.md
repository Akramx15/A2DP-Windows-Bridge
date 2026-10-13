---
title: 使用方法
layout: default
parent: 首页
nav_order: 2
---

# 使用方法
{: .no_toc }

## 目录
{: .no_toc .text-delta }

1. TOC
{:toc}

---

## GUI 模式（默认）

```
A2DPWB.exe
```

图形界面提供：

- **Bluetooth 适配器选择**——选择要使用的 WinUSB 适配器
- **音频设备选择**——选择 WASAPI 环回捕获的音源
- **设备地址**——手动输入、从已配对设备中选择，或在配置文件对话框中使用 **扫描设备**（通过 USB 适配器查找附近的设备）。输入或粘贴的地址会自动格式化：`AA:BB:CC:DD:EE:FF`、`aa-bb-cc-dd-ee-ff`、`AABBCCDDEEFF` 和 `AA BB CC DD EE FF` 都会变为 `AA:BB:CC:DD:EE:FF`
- **编解码器选择**——自动 / LDAC / aptX HD / aptX LL / aptX / SBC / AAC
- **LDAC 音质**——HQ (990 kbps) / SQ (660 kbps) / MQ (330 kbps)
- **LHDC V5 音质**——高 (1000 kbps) / 中高 (900 kbps) / 标准 (500 kbps) / 移动 (400 kbps)，即 Android 提供的四个级别
- **ABR**——针对不稳定连接的自适应比特率开关（LDAC 和 LHDC V5 时显示）
- **配置文件管理**——保存和加载设备 + 编解码器配置
- **实时状态**——编解码器、比特率、连接状态
- **链路质量**——**操作 > 链路质量...**（请参阅[链路质量窗口与 AFH](#link-quality)）
- **设置菜单**——语言、主题、随 Windows 启动、最小化到系统托盘、启动时检查更新、检查更新时包括预发布版本（也会提示 beta 版本）、耳机按键控制播放（请参阅[耳机按键](#headphone-buttons)）、记住耳机音量（请参阅[音量](#volume)）、调试模式（添加 **调试** 菜单：调试控制台、HCI 捕获、对端接收器测试）

## CLI 模式

添加 `--cli` 即可在不显示 GUI 窗口的情况下运行。

### 基本用法

```bash
# Auto-select best codec
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF
```

### 选择编解码器

```bash
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF -c ldac      # LDAC
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF -c aptxhd    # aptX HD
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF -c aptxll    # aptX Low Latency
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF -c aptx      # aptX
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF -c aac       # AAC
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF -c sbc       # SBC
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF -c lhdcv5    # LHDC V5 (experimental)
```

自动选择的优先级：LDAC > aptX HD > aptX LL > aptX > LHDC V5（实验性）> AAC > SBC

如果耳机不提供所请求的编解码器，CLI 会退回到“自动”的优先级顺序。aptX Adaptive 永远不会被选中——请参阅 [aptX 系列与 aptX Adaptive 的兼容性](#aptx-compatibility)。

### LDAC 音质

```bash
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF -c ldac -q hq   # 990 kbps (default)
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF -c ldac -q sq   # 660 kbps
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF -c ldac -q mq   # 330 kbps

# LHDC V5: hq 1000 / mhq 900 / sq 500 / mq 400 kbps (Android's four levels)
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF -c lhdcv5 -q mhq  # 900 kbps
```

### ABR（自适应比特率）

```bash
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF -c ldac -a
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF -c lhdcv5 -a
```

ABR 会在 Bluetooth 连接不稳定时自动降低比特率，并在状况改善后提高比特率。LDAC 在其各音质模式之间切换；LHDC V5 遵循 Android 的策略，保持在 160 到 400 kbps 之间。

### 设备发现

```bash
# List paired Bluetooth audio devices
A2DPWB.exe --cli -l
```

{: .note }
设备列表通过**内置** Bluetooth 适配器使用 Windows Bluetooth API 获取，而不是通过 WinUSB 适配器。

### 全部选项

| 选项 | 说明 |
|:-------|:------------|
| `-d <addr>` | 耳机的 Bluetooth 地址（`AA:BB:CC:DD:EE:FF`） |
| `-c <codec>` | `ldac`、`aptxhd`、`aptxll`、`aptx`、`aac`、`sbc`、`auto`（默认：`auto`） |
| `-q <mode>` | 音质：`hq`（默认）、`sq`、`mq`；LHDC V5 另有 `mhq`（900 kbps） |
| `-a` | ABR（LDAC 和 LHDC V5） |
| `-m <mode>` | 捕获模式：`loopback`（默认）、`virtual` |
| `--audio-device <id>` | 用于 `-m virtual` 的音频设备 ID |
| `-u <path>` | 适配器的 USB 设备路径（连接了多个适配器时） |
| `-l` | 列出已配对的 Bluetooth 音频设备并退出 |
| `--max-packet <bytes>` | 最大媒体数据包大小，679-1679（请参阅[最大媒体数据包大小](#max-packet)） |
| `--afh <auto\|off\|6,11>` | AFH 主机信道分类（请参阅[链路质量窗口与 AFH](#link-quality)） |
| `--remote-sink <host[:port]\|auto>` | 显示对端接收器收到的内容（请参阅[对端接收器测试](#receiver)） |
| `-h` | 显示帮助 |

用于开发和测试的选项还有：`--hci-tcp <host:port>`（通过 TCP 使用虚拟控制器，代替 WinUSB 适配器）、`--test-tone`（以左 1 kHz / 右 1.5 kHz 的测试音代替捕获的音频）、`--duration <s>`（运行指定秒数后停止）和 `--hci-capture <file.pklg>`（对整个运行过程进行 HCI 捕获）；请参阅[构建](building#emu)。

在 GUI 模式下，`--minimized` 会让 A2DPWB 以系统托盘方式启动（**随 Windows 启动** 使用此选项）。

## 捕获模式 {#capture-modes}

A2DPWB 支持三种音频捕获模式，分别录制音频路径中的不同位置：

| 模式 | 录制内容 |
|:-----|:----------------|
| 系统环回 | Windows 默认输出设备播放的内容：所有应用混合后的声音 |
| 虚拟设备 | 应用播放到所选设备（如 VB-CABLE）的内容 |
| 应用程序 | 仅某一个应用（及其子进程）播放的内容，且是经过该应用自身处理之后的声音。需要 Windows 11 或 Windows 10 版本 2004（内部版本 19041）及更高版本 |

配置文件对话框的 **捕获源** 下也有相同的说明。

系统环回只是复制音频，不会把声音从默认输出设备上拿走。声音会继续在该设备（扬声器、HDMI 电视等）上播放，同时也在 Bluetooth 耳机上播放。如果只想在耳机上听到，请使用虚拟设备模式，并选择不会在任何地方发声的设备，例如 VB-CABLE。

在虚拟设备模式下，**自动切换默认设备**（默认开启）会在串流期间将所选设备设为 Windows 默认输出设备，这样无需更改 Windows 设置，应用就会播放到该设备。停止串流时，A2DPWB 会切换回之前的默认设备。

- **音频设备** 列表中只显示 A2DPWB 识别为虚拟音频设备的设备。
- 默认通信设备（通话类应用使用的设备）不会被更改。
- 某些应用在重新启动之前，仍会继续在之前的设备上播放。
- 如果 A2DPWB 意外退出，默认设备不会被切换回去。请在 Windows 声音设置中更改。

### 应用程序模式 {#application-mode}

应用程序模式发送某一个应用的音频，例如音乐播放器、浏览器或音效应用。在 **应用** 列表中选择该应用。列表显示当前正在播放音频的应用；如果找不到你的应用，请先在其中开始播放，然后单击 **刷新**。应用的子进程也包含在内，因此在独立进程中播放音频的浏览器也能正常工作。

- 应用不必正在运行。A2DPWB 仍会连接耳机，并在应用启动后开始发送。如果应用在串流期间退出，A2DPWB 会保持连接，并在应用重新启动后重新捕获它。
- 应用仍会在其自身的输出设备上播放。如果只想在耳机上听到，请将该输出设备静音（其主音量）：这不会影响 A2DPWB 捕获的内容。但不要在 Windows 音量合成器中将该应用静音，否则耳机也会没有声音。
- 应用在 Windows 音量合成器中的滑块会改变发送到耳机（以及其输出设备上）的音量。主音量和音量键不会改变发送的音频；当配置文件的 **Windows 音量同步** 为“自动”时，它们改为调节耳机的音量（请参阅[音量](#volume)）。
- Windows 会将音频转换为 A2DPWB 请求的采样率，因此声音设置中的设备格式不会对其造成限制。
- 在 Windows 10 版本 2004（内部版本 19041）之前的版本上，Windows 不支持按应用捕获。配置文件对话框会将“应用程序”标记为在该 PC 上不可用，使用此类配置文件连接时会显示错误。

### 音量 {#volume}

串流期间，如果耳机支持 AVRCP 绝对音量，主窗口会显示 **音量** 滑块。它直接设置耳机本身的音量，因此发送的音频保持完整的分辨率。该滑块也会跟随耳机上的音量按键和触控操作。

配置文件中的 **Windows 音量同步** 会在串流期间将某个 Windows 音量与耳机音量合为一个音量：

| 选项 | 同步的 Windows 音量 |
|:-------|:----------------------|
| 自动（默认） | A2DPWB 捕获的设备：系统环回和应用程序模式下为默认输出设备，虚拟设备模式下为所选设备 |
| 关 | 无：Windows 音量与耳机音量相互独立 |
| 某个输出设备 | 该设备的音量 |

- 耳机的音量按键和手势以及 **音量** 滑块会调节同步的 Windows 音量，而声音设置中该设备的滑块会调节耳机的音量。
- 音量键和 Windows 音量浮出控件始终作用于默认输出设备，因此只有当默认输出设备就是同步的设备时，它们才会改变耳机音量（“自动”即如此，但在未开启 **自动切换默认设备** 的虚拟设备模式下除外）。
- 开始串流时，耳机音量会采用 Windows 音量；停止串流时，设备会恢复到之前的音量。
- 使用“自动”时，音频是在应用 Windows 音量之前被捕获的，因此只会在耳机上衰减一次。如果同步的音量也会改变捕获到的音频，音量就会被应用两次：此时请选择“关”（请参阅[系统级音效应用](#effects-apps)）。
- 静音不会同步：将输出设备静音会让 PC 不发声，但不会让耳机静音。

**设置 > 记住耳机音量**（默认关闭）会为每台设备保存耳机音量，并在连接后、发送任何音频之前立即重新设置。否则耳机会以其自身上次的音量启动，这可能比上次使用 A2DPWB 时的音量大得多（例如刚在手机上用过之后）。

### 耳机按键 {#headphone-buttons}

串流期间，耳机上的播放/暂停、下一曲和上一曲控制（按键或触控手势，例如 WH-1000XM4 上的双击）可控制 PC 上的播放：A2DPWB 将它们作为媒体键传递给 Windows，由 Windows 媒体控件中显示的应用（Spotify、浏览器、媒体播放器等）接收。停止也会被传递。

- 播放和暂停都作为 Windows 的播放/暂停键处理，因此即使耳机所认为的状态已过时，每次轻点也总会在播放和暂停之间切换。
- A2DPWB 会告诉耳机当前是否在播放音频（依据是否有可捕获的音频），因此摘下时会暂停的耳机也能发送正确的命令。
- 快进 / 快退（长按按键）没有对应的 Windows 媒体键，会被忽略。
- 音量手势会改变耳机自身的音量；开启 **Windows 音量同步** 时，同步的 Windows 音量也会随之变化（请参阅[音量](#volume)）。
- 可通过 **设置 > 耳机按键控制播放** 关闭此功能，例如另一个程序已经响应媒体键、导致操作被执行两次时。

此功能需要耳机支持 AVRCP，几乎所有耳机都支持。如果不支持，串流仍可照常进行，只是按键不起作用。

### 系统级音效应用 {#effects-apps}

系统级音效应用（均衡器、音效增强、虚拟环绕声应用等）通常会安装自己的虚拟输出设备。其他应用播放到该设备；音效应用处理音频后，将结果播放到你在音效应用中选择的输出设备。在 A2DPWB 中选择哪种模式，决定了耳机是否能获得音效：

| 设置 | 到达耳机的内容 |
|:--------|:----------------------------|
| 虚拟设备：音效应用自身的虚拟设备 | 音效处理**之前**的音频：应用送入音效应用的内容 |
| 应用程序：音效应用 | 音效处理**之后**的音频：音效应用播放的内容（推荐） |
| 虚拟设备：音效应用的输出设备（例如将 VB-CABLE 设为其输出） | 音效处理之后的音频（适用于不支持应用程序模式的 Windows 版本） |

在应用程序模式下，音效应用的输出设备可以是任意设备：A2DPWB 获取的是应用播放的内容，而不是设备播放的内容，因此该设备只是音频的去处。为了不让 PC 同时发声，请选择没人收听的输出（例如显示器的 HDMI 音频），或将该设备的主音量静音。VB-CABLE 这样的虚拟设备也可以作为静默的输出目标，但并非必需。该设备必须已启用且已连接。

如果音效应用自身的虚拟设备是 Windows 默认输出设备，其音量也会降低音效应用接收到的音频。此时若 **Windows 音量同步** 为“自动”，音量变化会被应用两次（在音效应用的输入端和耳机上）。请在该配置文件中将其设为“关”，并使用耳机的音量或 **音量** 滑块。

#### 为什么 A2DPWB 没有自己的虚拟输出设备 {#no-virtual-device}

要在 Windows 输出设备列表中出现一个可供音效应用直接选择的 Bluetooth 耳机项，就需要内核模式音频驱动。A2DPWB 不包含这样的驱动：

- 编写和维护内核音频驱动工作量很大，其中的错误可能导致 Windows 崩溃（蓝屏）。
- Windows 10 和 11 只加载经过 Microsoft 签名的驱动。获得该签名需要 EV 代码签名证书（每年数百美元）和 Microsoft 合作伙伴中心帐户，通常需以公司名义注册。
- 绕过签名的唯一方法是 Windows 测试签名模式，它会降低系统安全性，并导致部分软件（例如带反作弊的游戏）无法正常运行。A2DPWB 不会依赖它。

应用程序模式，或 VB-CABLE 等免费虚拟设备，可以满足相同的用途。

- Windows 没有用于更改默认设备的公开 API。A2DPWB 与其他音频切换工具一样，使用未公开的 IPolicyConfig 接口（Windows 10 和 11）。

{: .warning }
两种模式都使用 WASAPI 共享模式，因此捕获的采样率由 Windows 声音设置中设备的 **默认格式** 决定（通常为 48 kHz）。如需以 96 kHz 使用 LDAC，请在 **声音设置 > 设备属性 > 高级 > 默认格式** 中将输出设备的格式改为 96 kHz。

## 编解码器比较

| 编解码器 | 适合 | 取舍 |
|:------|:---------|:----------|
| LDAC (HQ) | 最高音质 | 延迟较高，需要稳定的连接 |
| LDAC (ABR) | 音质 + 稳定性 | 比特率随连接状况波动 |
| aptX HD | 高音质，设备支持更广 | 固定 576 kbps |
| aptX LL | 游戏、视频（低延迟） | 音质较低（16-bit） |
| aptX | aptX 设备支持最广 | 16-bit，352/384 kbps |
| AAC | Apple 设备，效率高 | 音质中等 |
| SBC | 兼容性最佳 | 音质最低 |

## aptX 系列与 aptX Adaptive 的兼容性 {#aptx-compatibility}

A2DPWB 支持三种 aptX 编解码器，均使用 [libopenaptx](https://github.com/pali/libopenaptx) 编码。**不**支持 aptX Adaptive。

{: .warning }
**aptX HD 和 aptX Low Latency 为实验性功能。** 它们参照 Android / PipeWire 的实现，并通过了编码/解码往返测试，但尚未在真实耳机上验证。

{: .note }
**只需要经典 aptX？** Windows 10 的内置 Bluetooth 堆栈已经支持经典 aptX（不含 aptX HD、aptX LL 和 aptX Adaptive），因此无需 A2DPWB。A2DPWB 主要用于 LDAC、aptX HD 和 aptX Low Latency。

| 编解码器 | Vendor ID / Codec ID | RTP 标头 | 状态 |
|:------|:---------------------|:-----------|:-------|
| aptX | 0x0000004F / 0x0001 | 无 | 支持（44.1/48 kHz 下 352/384 kbps） |
| aptX HD | 0x000000D7 / 0x0024 | 有 | 支持（576 kbps，24-bit 输入） |
| aptX Low Latency | 0x0000000A 或 0x000000D7 / 0x0002 | 无 | 支持（352 kbps） |
| aptX Adaptive | 0x000000D7 / 0x00AD | -- | **不支持**——仅检测并记录到日志 |

### 为什么不支持 aptX Adaptive

不存在开源的 aptX Adaptive 编码器。libopenaptx 只实现了 aptX 和 aptX HD（A2DPWB 也将其用于 aptX Low Latency）。[arkq/openaptx](https://github.com/arkq/openaptx) 只是重新实现了 aptX / aptX HD，不包含 aptX Adaptive 编码器，且不允许再分发二进制文件。因此 A2DPWB 只检测 aptX Adaptive 并将其写入日志。

### 不存在从 aptX Adaptive 到 aptX 的“回退”

A2DPWB 连接时，耳机会通告一个**编解码器列表**（AVDTP 流端点）。A2DPWB 从该列表中选择一种编解码器。aptX Adaptive 从来不是候选项，因此也就不存在从它“回退”的情况：

- **耳机同时列出了经典 aptX**——许多 aptX Adaptive 耳机都是如此，因为 Qualcomm 宣传 aptX Adaptive 向下兼容 aptX / aptX HD。这种情况下 A2DPWB 会直接选择经典 aptX；它是耳机列表中独立的一种编解码器，而不是 aptX Adaptive 的降级模式。
- **耳机只列出了 aptX Adaptive**（没有经典 aptX、aptX HD 或 aptX LL）——无法通过 A2DPWB 使用 aptX。“自动”会按优先级顺序选择下一个编解码器，通常是 AAC 或 SBC。

自动选择的优先级：LDAC > aptX HD > aptX LL > aptX > LHDC V5（实验性）> AAC > SBC

### 明确指定的编解码器不可用 {#codec-fallback}

当你请求了耳机未列出的特定编解码器时：

| 模式 | 行为 |
|:-----|:---------|
| GUI，勾选了 **此编解码器不可用时使用其他编解码器**（默认） | 按“自动”的优先级顺序，使用耳机提供的最佳编解码器连接，状态栏会显示改用了哪种编解码器以及原因。 |
| GUI，未勾选该选项 | 连接因错误而停止（“设备不支持 ...”）。如果你请求的是 aptX、aptX HD 或 aptX LL，而耳机只提供 aptX Adaptive，消息中会说明这一点，并建议使用“自动”、AAC 或 SBC。 |
| CLI（`-c <codec>`） | 输出 `Requested codec ... not available, falling back...`，并按“自动”的优先级顺序继续。如果耳机提供 aptX Adaptive，会额外输出一行，说明它永远不会被选中。 |

该选项位于 **编辑配置文件** 中编解码器的旁边（选择“自动”时不使用）。

### 耳机同时连接其他设备时缺少编解码器 {#sep-in-use}

耳机通过流端点（SEP）提供各个编解码器。在双设备（多点）连接时，正在向手机或 PC 串流的端点会被报告为**正在使用**，在该串流停止之前无法使用其编解码器。有些耳机还只在仅连接一台设备时才提供高解析度编解码器（LDAC、L2HC 等）。此时 A2DPWB 会报告：

```
BTstack: Remote SEP SEID=6 sink audio, IN USE (codec not queried)
BTstack: Remote SEPs in use by another connection: 1 of 6 audio sink SEP(s); ...
BTstack: Capability discovery complete (LDAC=0, ..., 1 of 6 audio sink SEP(s) in use)
```

并且错误消息会说明端点正被其他设备使用。请断开耳机与手机 / PC 的连接（或在耳机的应用中关闭双设备连接），然后重新连接。如果 PC 的内置 Bluetooth 仍与耳机配对，也算作其他设备。

### 采样率

aptX、aptX HD 和 aptX LL 只支持 44.1 kHz 和 48 kHz。A2DPWB 会从耳机通告的采样率中选择一个（优先使用当前 Windows 设备的采样率，否则使用 48 kHz），并由 WASAPI 对捕获的音频进行重采样。使用 aptX 时无需更改 Windows 声音设置。

### 查看耳机提供的编解码器

1. 在 **设置** 菜单中勾选 **调试模式（debug.log / HCI 日志）**，然后重新启动 A2DPWB。
2. 连接耳机。
3. 打开配置文件夹（`%APPDATA%\A2DPWB`）中的 `debug.log`。在 CLI 模式下，相同的内容会写入标准错误输出。

在 GUI 中，**调试 > 调试控制台...**（Ctrl+Shift+D）会实时显示相同的日志。其中的 **连接流程** 列表会将最近一次连接尝试的各个阶段（适配器初始化、ACL / AVDTP 连接、功能发现、编解码器选择、WASAPI、流配置、编码器、流开始、音频发送、断开 / 重新连接）标记为“正常”、“警告”或“失败”；双击某个步骤可跳转到其对应的日志行。下方的日志可以按级别和文本筛选。**复制诊断信息** 会将版本、操作系统、适配器、配置文件、连接流程、警告 / 错误以及最近的日志复制到剪贴板，可直接粘贴到错误报告中（默认情况下 Bluetooth 地址只保留厂商部分，其余被隐藏）。

功能发现相关的日志行会准确显示耳机列出了哪些编解码器：

```
BTstack: Remote supports aptX (SEID=..., caps=0x..)
BTstack: Remote supports aptX Adaptive (SEID=...) — never selected (no open encoder); classic aptX is used only if the remote lists it
BTstack: Remote vendor codec vid=0x........ cid=0x.... (SEID=...) — unsupported
BTstack: Capability discovery complete (LDAC=0, aptXHD=0, aptXLL=0, aptX=1, aptXAdaptive=1 [not encodable], SBC=1, AAC=1)
```

- `aptX=1`——可以使用经典 aptX（即使同时有 `aptXAdaptive=1`）。
- `aptX=0`、`aptXHD=0`、`aptXLL=0` 且 `aptXAdaptive=1`——耳机只提供 aptX Adaptive；请使用“自动”、AAC 或 SBC。
- `Remote vendor codec ... — unsupported`——A2DPWB 未实现的其他厂商编解码器。

## 最大媒体数据包大小（每个配置文件） {#max-packet}

每个配置文件中的 **最大媒体数据包大小**（**编辑配置文件**；CLI：`--max-packet <bytes>`）设置每个 Bluetooth 数据包中音频数据的上限，范围为 679 到 1679 字节，这样需要不同值的耳机可以各自使用自己的设置。耳机自身的上限（MTU）也始终有效；A2DPWB 使用两者中较小的值。更改从下次连接时生效。由旧版本保存的配置文件会沿用之前全局设置的值。

**推荐使用 1023（默认）。** 它正好容纳一个 Bluetooth 基带数据包（3-DH5）。更大的值只能节省 1-2% 的开销，而每丢失一个数据包会损失更多音频。679 正好容纳一个 2-DH5 数据包，也是 LDAC 接受的最小值。

## 验证媒体流（a2dpwb_decode） {#verify-stream}

大多数耳机无法显示正在使用的编解码器。在调试模式下，GUI 有一个 **调试** 菜单，其中包含 **开始 HCI 捕获** / **停止 HCI 捕获**。捕获文件是一份 HCI 数据包日志（配置文件夹中的 `hci_<date>_<time>.pklg`），包含编解码器协商和发送的每个媒体数据包。在 LDAC 990 kbps 下，它每小时大约增长 0.5 GB，因此请只在需要时进行捕获。

可以在连接前开始捕获，也可以在串流过程中开始。如果在连接期间开始捕获，A2DPWB 会先写入它记住的连接建立数据包（L2CAP 信道建立和 AVDTP 信令，包括编解码器协商），因此该文件仍然可以分析。停止后，A2DPWB 会显示文件路径。

可以用 Wireshark 打开捕获文件，或使用开发者工具 `a2dpwb_decode`（需从源代码构建，请参阅[构建](building)）进行检查：

```
a2dpwb_decode "%APPDATA%\A2DPWB\hci_20260925_120000.pklg" -o C:\temp\check
```

该工具会跟踪 AVDTP 信令（SET_CONFIGURATION / RECONFIGURE 以及耳机是否接受），提取 A2DPWB 发送的媒体数据包，并对照协商的配置进行检查：

| 编解码器 | 检查内容 | 输出 |
|:------|:----------------|:-------|
| SBC、AAC、aptX、aptX HD、aptX LL | 使用参考解码器完整解码；帧参数与配置是否一致；媒体负载标头中的帧数 | 可供试听的 `<prefix>.<n>.<codec>.wav` |
| LDAC | 仅检查帧头（同步字、采样率、声道模式、帧长度）、帧数和比特率。不存在开源的 LDAC 解码器 | 仅报告 |

对于每个流，它还会报告 RTP 序列号的间断、RTP 时间戳的推进速度、比特率，以及 Bluetooth 控制器报告已完成的 ACL 数据包数量。最后一行为 `RESULT: OK` 或 `RESULT: PROBLEMS FOUND`（退出代码为 0 或 1）。

{: .note }
这可以证明 A2DPWB 发送了什么，以及这些内容对于协商的编解码器是否有效。由于接收端只能解码协商好的编解码器，如果耳机播放的声音正常，同时报告也没有问题，就足以有力地证明该编解码器确实在使用中。

## 链路质量窗口与 AFH {#link-quality}

**操作 > 链路质量...** 会显示串流期间 A2DPWB 发送的内容：编解码器和比特率、已发送的数据包、发送前丢弃的数据包（发送队列已满：无线链路跟不上）、编码器来不及处理的音频、发送队列，以及 **无线**：连接的 RSSI（相对于控制器的理想接收范围，0 = 正常，负值 = 太弱；并非绝对电平），以及链路在 79 个 Bluetooth 信道中的多少个信道上跳频（AFH）。

Bluetooth 会自行避开繁忙的信道（AFH），但它只能通过错误来了解 Wi-Fi 的存在。A2DPWB 还会在每次连接时告诉其控制器应避开哪些信道（AFH 主机信道分类）：

| `afh` | |
|:--|:--|
| `auto`（默认） | 本 PC 能强烈接收到的 2.4 GHz Wi-Fi 网络（Windows Wi-Fi 扫描，连接期间重复进行） |
| `off` | 不指定：由控制器自行决定 |
| `6,11` | 指定的 Wi-Fi 信道 |

始终至少保留 20 个 Bluetooth 信道可用。可以通过 `settings.json`（配置文件夹中）的 `"afh"`、对端接收器测试窗口（会保存在那里）或 CLI 模式下的 `--afh` 进行设置。“无线”一行会显示正在避开哪些 Wi-Fi 信道。是否有帮助取决于环境：请用对端接收器测试进行测量。

## 对端接收器测试（tools/linux_sink） {#receiver}

链路质量窗口显示的是 A2DPWB 发送的内容。要查看实际到达的内容，请在另一台运行 Linux 的 PC 上运行 `tools/linux_sink/a2dpwb_sink.py`。它直接驱动该 PC 上的普通 Bluetooth 适配器（内置适配器即可），通过 HCI user channel 使用 Bumble，作为支持包括 LDAC 在内所有编解码器的 A2DP 接收端，测量到达的每个媒体数据包，并通过网络提供统计数据：

将 `tools/linux_sink` 文件夹复制到 Linux PC 上（只需这个文件夹），然后运行：

```bash
sh setup.sh      # once: decoder libraries, Python environment (Ubuntu / Debian, Python 3.11+)
sudo sh run.sh   # start the receiver (root: it takes the Bluetooth adapter)
```

在调试模式下，**调试 > 对端接收器测试...** 会自动在本地网络中查找接收器。**开始** 会以所选的编解码器、音质、采样率和位深度，向其 Bluetooth 适配器串流稳定的测试音（或系统音频）——无需配置文件或配对步骤——窗口会将发送的内容与到达的内容并排显示：比特率、丢失和迟到的数据包（RTP 序列号）、抖动和间隔、模拟播放缓冲区中的断音和跳过、流 / 解码错误，以及接收端的 RSSI（相对于其理想接收范围：0 = 正常，负值 = 太弱）。各个步骤（查找接收器、统计信息、接收器设置、Bluetooth 连接、编解码器、音频到达）会显示为 OK / NG，并附有可复制的日志。在 CLI 模式下，`--remote-sink auto` 可完成同样的操作：查找接收器，向其串流（除非 `-d` 指定了其他设备），并每 5 秒以及结束时输出其统计数据。

双方都从流的第一个数据包开始计数，并且在 **停止** 之后最终计数会继续显示（“停止时”；接收器的计数约晚一秒到达），因此可以比较发送和接收的数据包数：两者应当相等。Bluetooth 会在控制器内部重传丢失的无线数据包，而 A2DPWB 在链路保持连接期间不会放弃任何数据包，因此较差的无线链路在接收端会表现为间隔、抖动和断音，而不是丢包；当链路跟不上时，数据包会在发送端于发送前被丢弃。双方都无法获得重传次数。

工具运行期间，接收端的适配器无法供其自身的 Bluetooth 堆栈使用。发现过程使用 UDP 端口 51201 上的广播，因此两台 PC 必须位于同一网段（否则请输入地址）。所有选项和统计协议请参阅 `tools/linux_sink/README.md`。

## 故障排除

### Bluetooth 适配器无响应 {#adapter-no-answer}

如果适配器打开后没有任何回应（不响应 HCI Reset），A2DPWB 会在 5 秒后放弃，再打开一次；如果仍无响应，则显示“Bluetooth 适配器无响应”。请拔下适配器，重新插入后再试。无需重新启动 A2DPWB：启动失败时会释放适配器，每次新的启动都会重新初始化它。

为防止适配器卡住，A2DPWB 在打开适配器时会重置其 USB 管道，在关闭时会发送 HCI Reset。

### 更新后出现配对问题

A2DPWB 使用 SSP（Just Works）和 **General Bonding** 进行配对，并保存链接密钥，这样耳机无需重新配对即可再次连接。如果你曾用较旧的 A2DPWB 版本配对过耳机，请在耳机上删除 A2DPWB 的配对信息（清除配对列表的方法请参阅耳机的说明书），然后重新配对。

### 连接超时

A2DPWB 会注册 SDP 记录（A2DP Source、AVRCP Controller / Target），以便耳机将其识别为音频源。如果连接尝试超时，半开的连接会被断开，因此可以在同一会话中直接重试，无需重新启动 A2DPWB。请确认耳机已开机、在范围内，并且没有连接到其他设备。

当链路建立后没有传输任何数据，或耳机没有应答时，失败的连接会自动重试（最多 3 次）。如果耳机已不再保留配对信息（被重置，或与其他 PC 配对过），已存储的密钥会被丢弃，并重新进行一次配对。随后错误消息会说明发生了什么：

| 消息 | 含义 |
|:--|:--|
| 设备无应答 | 设备关闭、超出范围或不可连接（新设备：请进入配对模式） |
| 连接已建立但没有传输任何数据 | 无线环境问题：请重试，或调整适配器 / 设备的位置 |
| 设备已应答，但随后主动断开了连接 | 设备当前不接受连接：正在关机、充电或已连接到其他设备 |
| 身份验证失败，重新配对后仍然失败 | 请将设备置于配对模式，或删除其配对列表中的条目，然后重试 |
