---
title: 安裝設定
layout: default
parent: 首頁
nav_order: 1
---

# 安裝設定
{: .no_toc }

## 目錄
{: .no_toc .text-delta }

1. TOC
{:toc}

---

## 必要條件

- **Windows 10/11** (x64)
- **專用的 USB 藍牙介面卡**（與內建藍牙分開）
- **Zadig**（[https://zadig.akeo.ie/](https://zadig.akeo.ie/)）

## 安裝 A2DPWB

每個 [發行版本](https://github.com/SeiyaFunaokaJP/A2DP-Windows-Bridge/releases/latest) 都提供兩種形式，內含相同的檔案：

- **安裝程式**（`A2DPWB-<version>-win64-setup.exe`）：不需系統管理員權限即可為目前使用者安裝（也可選擇為所有使用者安裝），會新增「開始」功能表項目與解除安裝程式，首次安裝後會以安裝時選擇的語言啟動 A2DPWB
- **免安裝版**（`A2DPWB-<version>-win64-portable.zip`）：解壓縮到任意位置後執行 `A2DPWB.exe`

兩者都將設定、設定檔、連結金鑰與韌體儲存在 `%APPDATA%\A2DPWB`，因此可以互相切換。解除安裝時會保留該資料夾。

## 步驟 1：安裝 WinUSB 驅動程式

A2DPWB 透過 WinUSB 直接與 USB 藍牙介面卡通訊。您需要一個 **專用的介面卡** -- 內建藍牙仍可照常用於 Windows 的一般藍牙功能。

1. 插入 USB 藍牙介面卡
2. 下載並開啟 [Zadig](https://zadig.akeo.ie/)
3. 選擇 **Options > List All Devices**
4. 從下拉式選單中選取您的 USB 藍牙介面卡
5. 選擇 **WinUSB** 作為目標驅動程式
6. 按一下 **Replace Driver**

{: .warning }
介面卡處於 WinUSB 模式時，Windows 無法將它用於一般藍牙。一般周邊裝置（鍵盤、滑鼠等）請使用內建藍牙。

## 步驟 2：找出耳機的藍牙位址

您需要耳機/喇叭的藍牙 MAC 位址。

**從 Windows 設定：**
1. 開啟 **設定 > 藍牙與裝置**
2. 按一下您的音訊裝置
3. 按一下 **內容**
4. 會顯示藍牙位址（格式：`AA:BB:CC:DD:EE:FF`）

**從 A2DPWB GUI：**
- 啟動 `A2DPWB.exe`，已配對的藍牙音訊裝置會列在裝置下拉式選單中
- 或在設定檔對話方塊中使用 **掃描裝置**，以 USB 介面卡尋找附近的裝置（請將耳機切換到配對模式）

**從 CLI：**
```
A2DPWB.exe --cli -l
```

## 步驟 3：執行 A2DPWB

**GUI：**
```
A2DPWB.exe
```

**CLI：**
```
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF
```

詳細說明請參閱 [使用方式](usage)。

---

## 韌體（Realtek 介面卡）

採用 Realtek 晶片的 USB 藍牙介面卡（例如 TP-Link UB500、RTL8761BU 接收器）需要專有韌體才能運作。CSR 介面卡不需要韌體。Intel 與 Broadcom 介面卡（實驗性）可能需要各自的韌體檔案：請參閱 [建議的介面卡](#adapters)。

### GUI（下載引導）

啟動 `A2DPWB.exe` 並選擇 **操作 > 藍牙韌體選擇**。如果缺少韌體，會顯示警告。使用 **開啟下載頁面** 按鈕在瀏覽器中開啟 [linux-firmware/rtl_bt](https://git.kernel.org/pub/scm/linux/kernel/git/firmware/linux-firmware.git/tree/rtl_bt) 頁面，再使用 **開啟設定資料夾** 開啟目的資料夾。下載所需的 `.bin` 檔案並放到設定資料夾中。

此對話方塊會自動偵測介面卡的晶片組，並顯示需要哪些韌體檔案。

### 藍牙晶片設定

同一對話方塊中的 **藍牙晶片** 清單決定 A2DPWB 是否對介面卡執行 Realtek 初始化（上傳韌體）：

| 選項 | 意義 |
|:-------|:--------|
| **自動**（預設） | 每次啟動 A2DPWB 時依偵測到的介面卡決定。已知的 Realtek 介面卡會套用其晶片；其他介面卡則不執行 Realtek 初始化 |
| **非 Realtek** | 一律不執行 Realtek 初始化。若「自動」無法辨識您的 Intel、Broadcom、CSR 或其他介面卡，請選擇此項 |
| **RTL8761BU**、**RTL8822CU**、... | 指定的 Realtek 晶片。適用於「自動」無法辨識的 Realtek 介面卡（以自有 VID:PID 銷售的 OEM 產品）。狀態列會顯示其 `.bin` 檔案是否已在設定資料夾中 |
| **Custom** | 使用設定資料夾中的 `custom_fw.bin` 與 `custom_config.bin` |

請勿為非 Realtek 介面卡選擇 Realtek 晶片或 Custom：A2DPWB 會嘗試將 Realtek 韌體上傳到該介面卡，可能導致介面卡無法啟動。

### 手動下載

1. 從 [linux-firmware/rtl_bt](https://git.kernel.org/pub/scm/linux/kernel/git/firmware/linux-firmware.git/tree/rtl_bt) 下載適用於您晶片組的韌體與設定 `.bin` 檔案（例如 `rtl8761bu_fw.bin` 與 `rtl8761bu_config.bin`）
2. 將兩個檔案放到 A2DPWB 設定資料夾（`%APPDATA%\A2DPWB`，或韌體對話方塊中顯示的路徑）

{: .note }
這些韌體檔案是透過 linux-firmware 專案發佈的 Realtek 專有二進位檔，並未包含在本儲存庫中。再散布條款請參閱 [WHENCE](https://git.kernel.org/pub/scm/linux/kernel/git/firmware/linux-firmware.git/tree/WHENCE)。

---

## 建議的介面卡 {#adapters}

| 晶片組 | 產品範例 | 備註 |
|:--------|:-----------------|:------|
| Realtek | TP-Link UB500, RTL8761BU | 需要下載韌體（見上文）。測試最完整 |
| CSR | 一般的 CSR8510 接收器 | 隨插即用，不需要韌體 |
| Broadcom | ASUS BT400 (BCM20702) | *實驗性。* 回報廠商類別 `FF/01/01` 而非藍牙類別 `E0/01/01` 的介面卡（BCM20702 OEM 接收器）也能偵測。以 ROM 韌體運作；若設定資料夾中有選用的 PatchRAM `.hcd`（例如 `BCM20702A1-0b05-17cb.hcd`），會在啟動時上傳 |
| Intel | Intel 8265 / 9260 / AX200 / AX201 | *實驗性。* 處於 bootloader 模式時（例如冷開機後），設定資料夾中需要 linux-firmware `intel/` 內對應的 `ibt-*.sfi` + `ibt-*.ddc`。尚不支援 AX210 及更新的晶片（TLV bootloader） |
| MediaTek, Qualcomm | MT7921/MT7922, QCA61x4 | 不支援：需要 A2DPWB 沒有的韌體載入程式 |

USB 藍牙 5.0 以上的介面卡通常最適合 LDAC 等高位元率轉碼器。
