---
title: 许可证
layout: default
parent: 首页
nav_order: 5
---

# 许可证
{: .no_toc }

## 目录
{: .no_toc .text-delta }

1. TOC
{:toc}

---

## 项目许可证

A2DP Windows Bridge (A2DPWB) 采用 **Apache License 2.0** 或 **MIT License** 双重许可，可任选其一（SPDX `Apache-2.0 OR MIT`）。请参阅 [LICENSE](https://github.com/SeiyaFunaokaJP/A2DP-Windows-Bridge/blob/main/LICENSE)、[LICENSE-APACHE](https://github.com/SeiyaFunaokaJP/A2DP-Windows-Bridge/blob/main/LICENSE-APACHE) 和 [LICENSE-MIT](https://github.com/SeiyaFunaokaJP/A2DP-Windows-Bridge/blob/main/LICENSE-MIT)。

{: .warning }
A2DPWB 源代码采用 Apache-2.0 或 MIT 双重许可。分发的 `A2DPWB.exe` 包含 BTstack，因此只能用于个人非商业用途的使用和再分发。

## 第三方库

| 库 | 固定版本 | 许可证 | 用途 |
|:--------|:---------------|:--------|:------|
| [BTstack](https://github.com/bluekitchen/btstack) | v1.8.1-6-g5bc5cbdbe | BTstack License（BSD-3-Clause 风格，附带非商业条款）/ 商业许可 | 用户模式 Bluetooth 堆栈 |
| Bluedroid SBC codec（随 BTstack 附带） | 与 BTstack 相同 | Apache-2.0 | SBC 编码器 |
| rijndael（随 BTstack 附带） | 与 BTstack 相同 | 公有领域 | AES（BTstack 使用） |
| [libldac (AOSP)](https://android.googlesource.com/platform/external/libldac) | android-15.0.0_r36-4-geeee1a3 | Apache-2.0 | LDAC 编码器 |
| [libopenaptx](https://github.com/pali/libopenaptx) | 0.2.0 | LGPL-2.1+ | aptX / aptX HD / aptX LL 编码器 |
| [fdk-aac](https://github.com/mstorsjo/fdk-aac) | v2.0.3-158-gd8e6b1a | FDK AAC License | AAC-LC 编码器 |
| [LHDC-V5-Encoder](https://github.com/WillyBilly06/LHDC-V5-Encoder)（AOSP 编码器的 C 语言移植版） | commit 3f9d198 | Apache-2.0（FFT：BSD-3-Clause，KISS FFT） | LHDC V5 编码器（实验性） |
| [wxWidgets](https://www.wxwidgets.org/) | 3.2.6 | wxWindows Library Licence 3.1 | GUI 框架 |
| zlib（wxWidgets 内置） | 1.2.13.1 | zlib license | 压缩（PNG 支持） |
| libpng（wxWidgets 内置） | 1.6.37 | PNG Reference Library License v2 | PNG 图像 |
| nanosvg（随 wxWidgets 附带） | 与 wxWidgets 3.2.6 相同 | zlib license | SVG 图像 |
| [nlohmann/json](https://github.com/nlohmann/json) | 3.12.0 | MIT | JSON 解析器 |

完整的许可证文本和合规详情请参阅 [THIRD_PARTY_LICENSES.md](https://github.com/SeiyaFunaokaJP/A2DP-Windows-Bridge/blob/main/THIRD_PARTY_LICENSES.md)。发布的 zip 包在 `licenses/` 文件夹中包含各许可证文本，并附有列出确切源代码提交的 `SOURCE.txt`。Realtek 固件和 Zadig 不随 A2DPWB 分发（仅提供链接）。

## 重要说明

### BTstack 许可证（非商业）

BTstack **不是** BSD-3-Clause，也不是 OSI 认可的开源许可证。其许可证（SPDX `LicenseRef-BTstack`）在 BSD-3-Clause 的三项条件之外，还附加了第四项条款：

> 4. Any redistribution, use, or modification is done solely for personal benefit and not for any commercial purpose or for monetary gain.

（大意：任何再分发、使用或修改都只能出于个人利益，不得用于任何商业目的或获取金钱收益。）

由于 `A2DPWB.exe` 包含 BTstack，此限制适用于分发的二进制文件。商业用途需要从 [BlueKitchen GmbH](https://bluekitchen-gmbh.com/)（contact@bluekitchen-gmbh.com）获得商业许可。

### LDAC 认证

libldac 的 NOTICE 中写道：“Taking the certification process is required to use LDAC in your products.”（在产品中使用 LDAC 需要经过认证流程。）请参阅 [sony.net/Products/LDAC/aosp](https://www.sony.net/Products/LDAC/aosp/)。尚未确认 Sony 的认证流程是否适用于 A2DPWB 这样免费的非商业应用；A2DPWB 未经过认证。

### LHDC 商标与认证

LHDC 是 Savitech Corp. 的编解码器，由 Hi-Res Wireless Audio (HWA) 联盟推广。A2DPWB 使用的编码器是源自 Google 在 AOSP（Android 17）中发布的编码器的 Apache-2.0 代码，但 Apache-2.0 不授予任何商标权，而宣传支持 LHDC 的产品通常需要通过 Savitech / HWA 的认证。A2DPWB 未经过认证；LHDC V5 支持是一项实验性的、非商业的互操作功能。

### libopenaptx 版本

libopenaptx 固定为 **0.2.0**，这是最后一个采用 LGPL-2.1+ 的版本。从 0.2.1 开始，它改为 GPL-3.0-or-later 并附加了额外限制，与 A2DPWB 链接的其他库不兼容。v1.0.4 及之前的发布版本误用了 0.2.1 进行构建；请使用 v1.0.5 或更高版本。

libopenaptx 为静态链接。发布的 zip 包附带 LGPL-2.1 文本，“关于”对话框显示 libopenaptx 的版权信息并指向许可证文本，完整的对应源代码（用于使用修改过的 libopenaptx 重新构建/重新链接）可从仓库中该发布版本标签对应的提交及其子模块获取。

### aptX 专利声明

aptX（经典）、aptX HD、aptX Low Latency 和 aptX Adaptive 是 Qualcomm 的商标。根据其 README，libopenaptx 是一个“derived from ffmpeg 4.0 project”（源自 ffmpeg 4.0 项目）的开源 aptX 实现；本项目将其用于 aptX、aptX HD 和 aptX Low Latency。在某些司法管辖区，aptX 编码可能受专利保护。A2DPWB 不包含 aptX Adaptive 编码器。

### fdk-aac 许可证与 AAC 专利声明

fdk-aac 许可证要求在再分发二进制文件时附带完整的许可证文本，并向二进制文件的接收者免费提供 FDK AAC Codec 的完整源代码（包括任何修改）。它还**不授予任何专利许可**：AAC 可能受专利保护（NOTICE 中提到了 Via Licensing），获取任何所需的 AAC 专利许可是用户自己的责任。

### 开发者工具 a2dpwb_decode

`a2dpwb_decode`（请参阅[使用方法](usage#verify-stream)）只能从源代码构建，不包含在发布的 zip 包中。它链接上述固定版本的 Bluedroid SBC 解码器（Apache-2.0）、fdk-aac（FDK AAC License）和 libopenaptx 0.2.0（LGPL-2.1+），**不**链接 BTstack 或 libldac。请参阅 THIRD_PARTY_LICENSES.md §12。

### 测试环境 tools/emu 和 tools/linux_sink

`tools/emu`（请参阅[构建](building)）和测量用接收器 `tools/linux_sink`（请参阅[使用方法](usage#receiver)）使用 [Bumble](https://github.com/google/bumble) 0.0.234（Apache-2.0）及其依赖项，它们以固定版本下载到本地的、被 git 忽略的环境中。这些都不会链接到 A2DPWB 中，也不随 A2DPWB 分发。软件包列表和许可证请参阅 THIRD_PARTY_LICENSES.md §13。

### Bluetooth 认证

商用 Bluetooth 产品必须通过由 Bluetooth SIG 管理的 [Bluetooth Qualification Process](https://www.bluetooth.com/)（Bluetooth 认证流程）。
