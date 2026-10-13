---
title: 建置
layout: default
parent: 首頁
nav_order: 3
---

# 從原始碼建置
{: .no_toc }

## 目錄
{: .no_toc .text-delta }

1. TOC
{:toc}

---

## 需求

- **Visual Studio 2022 以上**，並安裝「使用 C++ 的桌面開發」工作負載
- **CMake** 3.16+
- **Git**（用於子模組與 wxWidgets FetchContent）

## 快速建置

```bash
git clone --recursive https://github.com/SeiyaFunaokaJP/A2DP-Windows-Bridge.git
cd A2DP-Windows-Bridge
cmake -B build -A x64
cmake --build build --config Release
```

執行檔會輸出到 `build/app/Release/A2DPWB.exe`。

{: .note }
第一次建置需要數分鐘，因為 CMake FetchContent 會下載並編譯 wxWidgets (v3.2.6)。

## 若複製時未使用 `--recursive`

```bash
git submodule update --init --recursive
```

## 專案結構

```
A2DP-Windows-Bridge/
├── app/                    應用程式原始碼
│   ├── src/                C++ 原始檔
│   ├── lang/               多語系（en.json 為原文；tools/check_lang.py 會檢查其他語言）
│   ├── resources/          圖示、資訊清單、資源指令碼
│   └── CMakeLists.txt      應用程式建置設定
├── cmake/                  發行套件打包（make_dist.cmake）
├── installer/              安裝程式的 Inno Setup 指令碼（A2DPWB.iss）
├── compat/                 AOSP 程式碼用的 MSVC 相容性標頭
├── docs/                   說明文件（本網站）
├── extern/                 第三方程式庫（git 子模組）
│   ├── btstack/            BTstack 藍牙堆疊
│   ├── libldac/            AOSP LDAC 編碼器
│   ├── libopenaptx/        aptX / aptX HD / aptX LL 編碼器
│   ├── fdk-aac/            Fraunhofer AAC 編碼器
│   └── json/               nlohmann/json（僅標頭）
├── tools/
│   ├── a2dp_decode/        a2dpwb_decode：檢查 / 解碼 HCI 擷取檔（.pklg）中的媒體串流
│   ├── emu/                針對虛擬藍牙接收端的端對端測試（Python）
│   └── linux_sink/         a2dpwb_sink：Linux 上的測量用 A2DP 接收器（Python、Bumble）
├── CMakeLists.txt          根建置設定
└── build.bat.example       建置輔助指令碼（複製為 build.bat 並設定 CMake 路徑）
```

## 建置選項

| CMake 選項 | 預設 | 說明 |
|:-------------|:--------|:------------|
| `LDAC_SOFT_FLOAT` | `OFF` | libldac 使用軟體浮點運算 |
| `A2DPWB_BUILD_TOOLS` | `ON` | 建置開發者工具（`a2dpwb_decode`，請參閱 [使用方式](usage#verify-stream)）。不包含在發行套件中 |

```bash
cmake -B build -A x64 -DA2DPWB_BUILD_TOOLS=OFF
```

## 發行套件

```bash
cmake --build build --target A2DPWB_dist --config Release
```

會產生 `build/dist/A2DPWB-<version>-win64-portable.zip`；若已安裝 [Inno Setup 6](https://jrsoftware.org/isinfo.php)（`winget install JRSoftware.InnoSetup`；安裝後請重新執行 CMake configure），還會從 `installer/A2DPWB.iss` 產生 `build/dist/A2DPWB-<version>-win64-setup.exe`。兩者內含相同的檔案。

## 不使用硬體進行測試 (tools/emu) {#emu}

`tools/emu` 會在同一台 PC 上，將 A2DPWB 的每種轉碼器串流到虛擬藍牙接收端，不需要介面卡或耳機。它只能在 Windows 上執行，除了建置需求外還需要 **Python 3.11 以上**。

```bash
python tools/emu/setup_env.py     # once: creates tools/emu/.venv with pinned packages
python tools/emu/run_test.py      # after a Release build; about 35 s for all codecs
```

運作方式：

- `emu_sink.py` 在同一處理程序內的連結上執行兩個 [Bumble](https://github.com/google/bumble) 虛擬控制器。其中一個以 H4 over TCP（`127.0.0.1`）提供給 A2DPWB 使用。另一個由 Bumble A2DP 接收端使用，提供 SBC、AAC、aptX、aptX HD、aptX LL 與 LDAC，並記錄其 HCI 流量。
- `run_test.py` 會以開發用選項，針對每種轉碼器執行 `A2DPWB.exe --cli`：`--hci-tcp`（以虛擬控制器取代 WinUSB）、`--test-tone`（以左 1 kHz / 右 1.5 kHz 取代系統音訊）、`--duration`、`--hci-capture`。環境變數 `A2DPWB_CONFIG_DIR` 會讓 A2DPWB 使用另一個設定資料夾，因此與虛擬接收端配對時絕不會動到您實際的連結金鑰。
- 接著以 `a2dpwb_decode` 檢查兩份 HCI 擷取檔（接收端使用 `--received`）。

符合以下所有條件時，該轉碼器即通過：

- 協商的是所要求的轉碼器
- `a2dpwb_decode` 在傳送端與接收端都沒有發現問題
- 接收端收到 A2DPWB 傳送的每個媒體封包與訊框，且至少傳送了所要求音訊長度的 90%
- LDAC 除外（沒有開放原始碼的解碼器）：兩端解碼後的音訊完全相同，且為測試音

| 選項 | 說明 |
|:-------|:------------|
| `--codecs sbc aac ...` | 只測試這些轉碼器（`sbc aac aptx aptxhd aptxll ldac`，預設：全部） |
| `--duration <s>` | 每種轉碼器的串流秒數（預設 5） |
| `--max-packet <bytes>` | 傳給 A2DPWB（媒體封包大小上限，請參閱 [使用方式](usage)） |
| `--build-dir`, `--config` | 尋找 `A2DPWB.exe` / `a2dpwb_decode.exe` 的位置（預設 `build`、`Release`） |
| `--out <dir>` | 輸出資料夾（預設 `tools/emu/out`） |

每種轉碼器的 `tools/emu/out/<codec>/` 中包含擷取檔（`a2dpwb.pklg`、`sink.pklg`）、記錄、`a2dpwb_decode` 報告與解碼後的 WAV 檔案。所有轉碼器都通過時，結束代碼為 0。

{: .note }
虛擬連結沒有無線電：它會檢查編碼、封包化、AVDTP / L2CAP 訊號，以及與獨立藍牙堆疊 (Bumble) 的互通性，但不會檢查無線電環境、實際硬體上的時序，或實際耳機播放音訊的情形。Bumble 套件僅為測試工具，不屬於 A2DPWB 的一部分（請參閱 THIRD_PARTY_LICENSES.md §13）。

`emu_vhci.py` 則針對 Linux 藍牙堆疊進行相同的工作：在 Linux 上，它會透過 `/dev/vhci` 將虛擬介面卡連接到 BlueZ，測量用接收器 `tools/linux_sink` 會像使用實體介面卡一樣使用它 - 因此不需要硬體即可測試，也可選擇模擬遺失或延遲的媒體封包（`--drop`、`--stall`）。請參閱 `tools/linux_sink/README.md`。

## 相依套件

所有相依套件都以 git 子模組形式包含，或在建置時下載，不需要手動安裝。

| 程式庫 | 取得方式 | 固定版本 | 授權 |
|:--------|:-------|:---------------|:--------|
| BTstack | git 子模組 | v1.8.1-6-g5bc5cbdbe | BTstack License（類似 BSD-3-Clause，附非商業條款） |
| Bluedroid SBC codec | 隨附於 BTstack（`3rd-party/bluedroid`） | 同 BTstack | Apache-2.0 |
| rijndael | 隨附於 BTstack（`3rd-party/rijndael`） | 同 BTstack | 公眾領域 |
| libldac (AOSP) | git 子模組 | android-15.0.0_r36-4-geeee1a3 | Apache-2.0 |
| libopenaptx | git 子模組 | 0.2.0（請勿更新） | LGPL-2.1+ |
| fdk-aac | git 子模組 | v2.0.3-158-gd8e6b1a | FDK AAC License |
| nlohmann/json | git 子模組（僅標頭） | 3.12.0 | MIT |
| wxWidgets | CMake FetchContent | v3.2.6 | wxWindows Library Licence 3.1 |
| zlib / libpng / nanosvg | wxWidgets 內建的副本 | 1.2.13.1 / 1.6.37 / 同 wxWidgets | zlib / PNG Reference Library License v2 / zlib |

{: .warning }
由於連結了 BTstack，即使 A2DPWB 原始碼本身採用 Apache-2.0 或 MIT 雙重授權，您自行建置的任何 `A2DPWB.exe` 都只能用於個人、非商業目的的使用與再散布。

完整授權資訊請參閱 [THIRD_PARTY_LICENSES.md](https://github.com/SeiyaFunaokaJP/A2DP-Windows-Bridge/blob/main/THIRD_PARTY_LICENSES.md)。
