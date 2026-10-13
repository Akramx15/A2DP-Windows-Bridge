---
title: 安装与设置
layout: default
parent: 首页
nav_order: 1
---

# 安装与设置
{: .no_toc }

## 目录
{: .no_toc .text-delta }

1. TOC
{:toc}

---

## 前提条件

- **Windows 10/11**（x64）
- **专用 USB Bluetooth 适配器**（独立于内置 Bluetooth）
- **Zadig**（[https://zadig.akeo.ie/](https://zadig.akeo.ie/)）

## 安装 A2DPWB

每个[发布版本](https://github.com/SeiyaFunaokaJP/A2DP-Windows-Bridge/releases/latest)提供两种形式，内含文件相同：

- **安装程序**（`A2DPWB-<version>-win64-setup.exe`）：无需管理员权限即可为当前用户安装（也可选择为所有用户安装），添加“开始”菜单项和卸载程序；首次安装后会以安装时选择的语言启动 A2DPWB
- **便携版**（`A2DPWB-<version>-win64-portable.zip`）：解压到任意位置，运行 `A2DPWB.exe` 即可

两者都将设置、配置文件、链接密钥和固件保存在 `%APPDATA%\A2DPWB` 中，因此可以在两者之间切换。卸载时该文件夹会保留。

## 第 1 步：安装 WinUSB 驱动

A2DPWB 通过 WinUSB 直接与 USB Bluetooth 适配器通信。你需要一个**专用适配器**——内置 Bluetooth 仍可照常用于 Windows 的普通 Bluetooth 功能。

1. 插入 USB Bluetooth 适配器
2. 下载并打开 [Zadig](https://zadig.akeo.ie/)
3. 选择 **Options > List All Devices**
4. 从下拉列表中选择你的 USB Bluetooth 适配器
5. 选择 **WinUSB** 作为目标驱动
6. 单击 **Replace Driver**

{: .warning }
适配器处于 WinUSB 模式时，Windows 无法将其用于普通 Bluetooth。键盘、鼠标等普通外设请使用内置 Bluetooth。

## 第 2 步：查找耳机的 Bluetooth 地址

你需要耳机/音箱的 Bluetooth MAC 地址。

**通过 Windows 设置：**
1. 打开 **设置 > 蓝牙和其他设备**
2. 单击你的音频设备
3. 单击 **属性**
4. 即可看到 Bluetooth 地址（格式：`AA:BB:CC:DD:EE:FF`）

**通过 A2DPWB GUI：**
- 启动 `A2DPWB.exe`，已配对的 Bluetooth 音频设备会列在设备下拉列表中
- 或在配置文件对话框中使用 **扫描设备**，通过 USB 适配器查找附近的设备（请让耳机进入配对模式）

**通过 CLI：**
```
A2DPWB.exe --cli -l
```

## 第 3 步：运行 A2DPWB

**GUI：**
```
A2DPWB.exe
```

**CLI：**
```
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF
```

详细信息请参阅[使用方法](usage)。

---

## 固件（Realtek 适配器）

基于 Realtek 的 USB Bluetooth 适配器（如 TP-Link UB500、RTL8761BU 适配器）需要专有固件才能工作。CSR 适配器无需固件。Intel 和 Broadcom 适配器（实验性）可能需要各自的固件文件：请参阅[推荐适配器](#adapters)。

### GUI（下载引导）

启动 `A2DPWB.exe`，选择 **操作 > Bluetooth 固件选择**。如果缺少固件，会显示警告。使用 **打开下载页面** 按钮在浏览器中打开 [linux-firmware/rtl_bt](https://git.kernel.org/pub/scm/linux/kernel/git/firmware/linux-firmware.git/tree/rtl_bt) 页面，然后使用 **打开配置文件夹** 打开目标文件夹。下载所需的 `.bin` 文件并放入该配置文件夹。

该对话框会自动检测适配器的芯片组，并显示需要哪些固件文件。

### Bluetooth 芯片设置

同一对话框中的 **Bluetooth 芯片** 列表决定 A2DPWB 是否对适配器执行 Realtek 初始化（上传固件）：

| 选项 | 含义 |
|:-------|:--------|
| **自动**（默认） | 每次启动 A2DPWB 时根据检测到的适配器决定。已知的 Realtek 适配器会使用对应的芯片；其他适配器不执行 Realtek 初始化 |
| **非 Realtek** | 从不执行 Realtek 初始化。如果“自动”无法识别你的适配器，Intel、Broadcom、CSR 等适配器请选择此项 |
| **RTL8761BU**、**RTL8822CU**…… | 指定的 Realtek 芯片。用于“自动”无法识别的 Realtek 适配器（使用自有 VID:PID 的 OEM 产品）。状态行会显示其 `.bin` 文件是否已在配置文件夹中 |
| **Custom** | 使用配置文件夹中的 `custom_fw.bin` 和 `custom_config.bin` |

不要为非 Realtek 适配器选择 Realtek 芯片或 Custom：A2DPWB 会尝试向其上传 Realtek 固件，适配器可能因此无法启动。

### 手动下载

1. 从 [linux-firmware/rtl_bt](https://git.kernel.org/pub/scm/linux/kernel/git/firmware/linux-firmware.git/tree/rtl_bt) 下载与芯片组对应的固件和配置 `.bin` 文件（如 `rtl8761bu_fw.bin` 和 `rtl8761bu_config.bin`）
2. 将这两个文件放入 A2DPWB 配置文件夹（`%APPDATA%\A2DPWB`，或固件对话框中显示的路径）

{: .note }
这些固件文件是通过 linux-firmware 项目分发的 Realtek 专有二进制文件，未包含在本仓库中。再分发条款请参阅 [WHENCE](https://git.kernel.org/pub/scm/linux/kernel/git/firmware/linux-firmware.git/tree/WHENCE)。

---

## 推荐适配器 {#adapters}

| 芯片组 | 产品示例 | 说明 |
|:--------|:-----------------|:------|
| Realtek | TP-Link UB500、RTL8761BU | 需要下载固件（见上文）。测试最充分 |
| CSR | 通用 CSR8510 适配器 | 开箱即用，无需固件 |
| Broadcom | ASUS BT400 (BCM20702) | *实验性。* 报告厂商类 `FF/01/01` 而非 Bluetooth 类 `E0/01/01` 的适配器（BCM20702 OEM 适配器）也能被检测到。使用 ROM 固件运行；如果配置文件夹中有可选的 PatchRAM `.hcd` 文件（如 `BCM20702A1-0b05-17cb.hcd`），会在启动时上传 |
| Intel | Intel 8265 / 9260 / AX200 / AX201 | *实验性。* 处于 bootloader 模式时（如冷启动后），需要将 linux-firmware `intel/` 中对应的 `ibt-*.sfi` + `ibt-*.ddc` 放入配置文件夹。尚不支持 AX210 及更新型号（TLV bootloader） |
| MediaTek、Qualcomm | MT7921/MT7922、QCA61x4 | 不支持：需要 A2DPWB 所没有的固件加载程序 |

对于 LDAC 等高比特率编解码器，USB Bluetooth 5.0+ 适配器通常效果最好。
