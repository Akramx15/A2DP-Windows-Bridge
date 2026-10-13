---
title: 构建
layout: default
parent: 首页
nav_order: 3
---

# 从源代码构建
{: .no_toc }

## 目录
{: .no_toc .text-delta }

1. TOC
{:toc}

---

## 环境要求

- **Visual Studio 2022 或更高版本**，并安装“使用 C++ 的桌面开发”工作负载
- **CMake** 3.16+
- **Git**（用于子模块和 wxWidgets 的 FetchContent）

## 快速构建

```bash
git clone --recursive https://github.com/SeiyaFunaokaJP/A2DP-Windows-Bridge.git
cd A2DP-Windows-Bridge
cmake -B build -A x64
cmake --build build --config Release
```

可执行文件输出到 `build/app/Release/A2DPWB.exe`。

{: .note }
首次构建需要几分钟，因为 CMake FetchContent 会下载并编译 wxWidgets（v3.2.6）。

## 如果克隆时没有使用 `--recursive`

```bash
git submodule update --init --recursive
```

## 项目结构

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

## 构建选项

| CMake 选项 | 默认值 | 说明 |
|:-------------|:--------|:------------|
| `LDAC_SOFT_FLOAT` | `OFF` | 为 libldac 使用软件浮点 |
| `A2DPWB_BUILD_TOOLS` | `ON` | 构建开发者工具（`a2dpwb_decode`，请参阅[使用方法](usage#verify-stream)）。不包含在发布包中 |

```bash
cmake -B build -A x64 -DA2DPWB_BUILD_TOOLS=OFF
```

## 发布包

```bash
cmake --build build --target A2DPWB_dist --config Release
```

会生成 `build/dist/A2DPWB-<version>-win64-portable.zip`；如果安装了 [Inno Setup 6](https://jrsoftware.org/isinfo.php)（`winget install JRSoftware.InnoSetup`；安装后请重新运行 CMake 配置），还会根据 `installer/A2DPWB.iss` 生成 `build/dist/A2DPWB-<version>-win64-setup.exe`。两者包含相同的文件。

## 无硬件测试（tools/emu） {#emu}

`tools/emu` 在同一台 PC 上将 A2DPWB 的每种编解码器串流到虚拟 Bluetooth 接收端，无需适配器或耳机。它只能在 Windows 上运行，除构建要求外还需要 **Python 3.11 或更高版本**。

```bash
python tools/emu/setup_env.py     # once: creates tools/emu/.venv with pinned packages
python tools/emu/run_test.py      # after a Release build; about 35 s for all codecs
```

工作原理：

- `emu_sink.py` 在进程内链路上运行两个 [Bumble](https://github.com/google/bumble) 虚拟控制器。其中一个以 H4 over TCP（`127.0.0.1`）的形式提供给 A2DPWB。另一个由 Bumble A2DP 接收端使用，它提供 SBC、AAC、aptX、aptX HD、aptX LL 和 LDAC，并记录自身的 HCI 通信。
- `run_test.py` 针对每种编解码器，使用开发选项运行 `A2DPWB.exe --cli`：`--hci-tcp`（使用虚拟控制器代替 WinUSB）、`--test-tone`（以左 1 kHz / 右 1.5 kHz 的测试音代替系统音频）、`--duration`、`--hci-capture`。环境变量 `A2DPWB_CONFIG_DIR` 让 A2DPWB 使用单独的配置文件夹，因此与虚拟接收端的配对不会影响你真实的链接密钥。
- 随后用 `a2dpwb_decode` 检查两份 HCI 捕获（接收端一侧使用 `--received`）。

满足以下所有条件时，该编解码器即通过测试：

- 协商的是所请求的编解码器
- `a2dpwb_decode` 在发送端和接收端都没有发现问题
- 接收端收到了 A2DPWB 发送的每个媒体数据包和帧，并且至少发送了所请求音频时长的 90%
- LDAC 除外（没有开源解码器）：两端解码后的音频完全相同，且为测试音

| 选项 | 说明 |
|:-------|:------------|
| `--codecs sbc aac ...` | 只测试这些编解码器（`sbc aac aptx aptxhd aptxll ldac`，默认：全部） |
| `--duration <s>` | 每种编解码器的串流秒数（默认 5） |
| `--max-packet <bytes>` | 传递给 A2DPWB（最大媒体数据包大小，请参阅[使用方法](usage)） |
| `--build-dir`, `--config` | 查找 `A2DPWB.exe` / `a2dpwb_decode.exe` 的位置（默认 `build`、`Release`） |
| `--out <dir>` | 输出文件夹（默认 `tools/emu/out`） |

对于每种编解码器，`tools/emu/out/<codec>/` 中保存捕获文件（`a2dpwb.pklg`、`sink.pklg`）、日志、`a2dpwb_decode` 报告和解码后的 WAV 文件。所有编解码器都通过时，退出代码为 0。

{: .note }
虚拟链路没有无线电：它检查的是编码、分包、AVDTP / L2CAP 信令以及与独立 Bluetooth 堆栈（Bumble）的互操作性，但不检查无线环境、真实硬件上的时序，也不检查真实耳机如何播放音频。Bumble 软件包仅为测试工具，不属于 A2DPWB（请参阅 THIRD_PARTY_LICENSES.md §13）。

`emu_vhci.py` 面向 Linux Bluetooth 堆栈完成同样的工作：在 Linux 上，它通过 `/dev/vhci` 向 BlueZ 挂载一个虚拟适配器，测量用接收器 `tools/linux_sink` 会像对待真实适配器一样接管它——因此无需硬件即可测试，还可以选择模拟丢失或延迟的媒体数据包（`--drop`、`--stall`）。请参阅 `tools/linux_sink/README.md`。

## 依赖项

所有依赖项都以 git 子模块的形式包含，或在构建时获取。无需手动安装。

| 库 | 方式 | 固定版本 | 许可证 |
|:--------|:-------|:---------------|:--------|
| BTstack | git 子模块 | v1.8.1-6-g5bc5cbdbe | BTstack License（BSD-3-Clause 风格，附带非商业条款） |
| Bluedroid SBC codec | 随 BTstack 附带（`3rd-party/bluedroid`） | 与 BTstack 相同 | Apache-2.0 |
| rijndael | 随 BTstack 附带（`3rd-party/rijndael`） | 与 BTstack 相同 | 公有领域 |
| libldac (AOSP) | git 子模块 | android-15.0.0_r36-4-geeee1a3 | Apache-2.0 |
| libopenaptx | git 子模块 | 0.2.0（请勿更新） | LGPL-2.1+ |
| fdk-aac | git 子模块 | v2.0.3-158-gd8e6b1a | FDK AAC License |
| nlohmann/json | git 子模块（仅头文件） | 3.12.0 | MIT |
| wxWidgets | CMake FetchContent | v3.2.6 | wxWindows Library Licence 3.1 |
| zlib / libpng / nanosvg | wxWidgets 内置副本 | 1.2.13.1 / 1.6.37 / 与 wxWidgets 相同 | zlib / PNG Reference Library License v2 / zlib |

{: .warning }
由于链接了 BTstack，你构建的任何 `A2DPWB.exe` 都只能用于个人非商业用途的使用和再分发，尽管 A2DPWB 源代码本身采用 Apache-2.0 或 MIT 双重许可。

完整的许可证详情请参阅 [THIRD_PARTY_LICENSES.md](https://github.com/SeiyaFunaokaJP/A2DP-Windows-Bridge/blob/main/THIRD_PARTY_LICENSES.md)。
