---
title: 授權
layout: default
parent: 首頁
nav_order: 5
---

# 授權
{: .no_toc }

## 目錄
{: .no_toc .text-delta }

1. TOC
{:toc}

---

## 專案授權

A2DP Windows Bridge (A2DPWB) 採用 **Apache License 2.0** 或 **MIT License** 雙重授權，可任選其一（SPDX `Apache-2.0 OR MIT`）。請參閱 [LICENSE](https://github.com/SeiyaFunaokaJP/A2DP-Windows-Bridge/blob/main/LICENSE)、[LICENSE-APACHE](https://github.com/SeiyaFunaokaJP/A2DP-Windows-Bridge/blob/main/LICENSE-APACHE) 與 [LICENSE-MIT](https://github.com/SeiyaFunaokaJP/A2DP-Windows-Bridge/blob/main/LICENSE-MIT)。

{: .warning }
A2DPWB 原始碼採用 Apache-2.0 或 MIT 雙重授權。發佈的 `A2DPWB.exe` 包含 BTstack，因此只能用於個人、非商業目的的使用與再散布。

## 第三方程式庫

| 程式庫 | 固定版本 | 授權 | 用途 |
|:--------|:---------------|:--------|:------|
| [BTstack](https://github.com/bluekitchen/btstack) | v1.8.1-6-g5bc5cbdbe | BTstack License（類似 BSD-3-Clause，附非商業條款）/ 商業授權 | 使用者模式藍牙堆疊 |
| Bluedroid SBC codec（隨附於 BTstack） | 同 BTstack | Apache-2.0 | SBC 編碼器 |
| rijndael（隨附於 BTstack） | 同 BTstack | 公眾領域 | AES（BTstack 使用） |
| [libldac (AOSP)](https://android.googlesource.com/platform/external/libldac) | android-15.0.0_r36-4-geeee1a3 | Apache-2.0 | LDAC 編碼器 |
| [libopenaptx](https://github.com/pali/libopenaptx) | 0.2.0 | LGPL-2.1+ | aptX / aptX HD / aptX LL 編碼器 |
| [fdk-aac](https://github.com/mstorsjo/fdk-aac) | v2.0.3-158-gd8e6b1a | FDK AAC License | AAC-LC 編碼器 |
| [LHDC-V5-Encoder](https://github.com/WillyBilly06/LHDC-V5-Encoder)（AOSP 編碼器的 C 語言移植版） | commit 3f9d198 | Apache-2.0（FFT：BSD-3-Clause，KISS FFT） | LHDC V5 編碼器 |
| [wxWidgets](https://www.wxwidgets.org/) | 3.2.6 | wxWindows Library Licence 3.1 | GUI 框架 |
| zlib（內建於 wxWidgets） | 1.2.13.1 | zlib license | 壓縮（PNG 支援） |
| libpng（內建於 wxWidgets） | 1.6.37 | PNG Reference Library License v2 | PNG 圖片 |
| nanosvg（隨附於 wxWidgets） | 同 wxWidgets 3.2.6 | zlib license | SVG 圖片 |
| [nlohmann/json](https://github.com/nlohmann/json) | 3.12.0 | MIT | JSON 剖析器 |

完整授權條文與合規細節，請參閱 [THIRD_PARTY_LICENSES.md](https://github.com/SeiyaFunaokaJP/A2DP-Windows-Bridge/blob/main/THIRD_PARTY_LICENSES.md)。發行 zip 的 `licenses/` 資料夾中附有授權條文，另有列出確切原始碼 commit 的 `SOURCE.txt`。Realtek 韌體與 Zadig 不隨 A2DPWB 散布（僅提供連結）。

## 重要說明

### BTstack 授權（非商業）

BTstack **不是** BSD-3-Clause，也不是 OSI 認可的開放原始碼授權。其授權（SPDX `LicenseRef-BTstack`）包含 BSD-3-Clause 的三項條件，另加第四項條款：

> 4. Any redistribution, use, or modification is done solely for personal benefit and not for any commercial purpose or for monetary gain.

由於 `A2DPWB.exe` 包含 BTstack，此限制適用於發佈的二進位檔。商業使用需向 [BlueKitchen GmbH](https://bluekitchen-gmbh.com/)（contact@bluekitchen-gmbh.com）取得商業授權。

### LDAC 認證

libldac 的 NOTICE 載明："Taking the certification process is required to use LDAC in your products."（在產品中使用 LDAC 須通過認證程序）。請參閱 [sony.net/Products/LDAC/aosp](https://www.sony.net/Products/LDAC/aosp/)。目前尚未確認 Sony 的認證程序是否適用於 A2DPWB 這類免費、非商業的應用程式；A2DPWB 未經認證。

### LHDC 商標與認證

LHDC 是 Savitech Corp. 的轉碼器，由 Hi-Res Wireless Audio (HWA) 聯盟推廣。A2DPWB 使用的編碼器是衍生自 Google 在 AOSP（Android 17）中發佈之編碼器的 Apache-2.0 程式碼，但 Apache-2.0 不授予任何商標權，而標榜 LHDC 的產品通常需經過 Savitech / HWA 認證。A2DPWB 未經認證；LHDC V5 支援是非商業的互通性功能。

### libopenaptx 版本

libopenaptx 固定為 **0.2.0**，即最後一個 LGPL-2.1+ 版本。從 0.2.1 起改採 GPL-3.0-or-later 並附加額外限制，與連結到 A2DPWB 的其他程式庫不相容。v1.0.4 以前的發行版本誤以 0.2.1 建置；請使用 v1.0.5 以上版本。

libopenaptx 採靜態連結。發行 zip 附有 LGPL-2.1 條文，「關於」對話方塊會顯示 libopenaptx 的著作權聲明並指向授權條文，完整的對應原始碼（用於以修改後的 libopenaptx 重新建置/重新連結）可從儲存庫中該發行版本標記的 commit 及其子模組取得。

### aptX 專利聲明

aptX（傳統）、aptX HD、aptX Low Latency 與 aptX Adaptive 是 Qualcomm 的商標。根據其 README，libopenaptx 是 "derived from ffmpeg 4.0 project" 的開放原始碼 aptX 實作；本專案將它用於 aptX、aptX HD 與 aptX Low Latency。aptX 編碼在部分司法管轄區可能受專利保護。A2DPWB 不包含 aptX Adaptive 編碼器。

### fdk-aac 授權與 AAC 專利聲明

fdk-aac 授權要求再散布二進位檔時必須附上完整授權條文，並須免費向二進位檔的接收者提供 FDK AAC Codec（及任何修改）的完整原始碼。此授權也 **不授予任何專利授權**：AAC 可能受專利保護（NOTICE 提及 Via Licensing），取得任何必要的 AAC 專利授權是使用者的責任。

### 開發者工具 a2dpwb_decode

`a2dpwb_decode`（請參閱 [使用方式](usage#verify-stream)）只能從原始碼建置，不包含在發行 zip 中。它以上述固定版本連結 Bluedroid SBC 解碼器（Apache-2.0）、fdk-aac（FDK AAC License）與 libopenaptx 0.2.0（LGPL-2.1+），**不**連結 BTstack 或 libldac。請參閱 THIRD_PARTY_LICENSES.md §12。

### 測試環境 tools/emu 與 tools/linux_sink

`tools/emu`（請參閱 [建置](building)）與測量用接收器 `tools/linux_sink`（請參閱 [使用方式](usage#receiver)）使用 [Bumble](https://github.com/google/bumble) 0.0.234（Apache-2.0）及其相依套件，這些套件以固定版本下載到本機、被 git 忽略的環境中。它們都不會連結到 A2DPWB，也不會隨 A2DPWB 散布。套件清單與授權請參閱 THIRD_PARTY_LICENSES.md §13。

### 藍牙認證

商業藍牙產品必須通過藍牙技術聯盟 (Bluetooth SIG) 管理的 [Bluetooth Qualification Process](https://www.bluetooth.com/)。
