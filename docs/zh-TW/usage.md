---
title: 使用方式
layout: default
parent: 首頁
nav_order: 2
---

# 使用方式
{: .no_toc }

## 目錄
{: .no_toc .text-delta }

1. TOC
{:toc}

---

## GUI 模式（預設）

```
A2DPWB.exe
```

圖形介面提供：

- **藍牙介面卡選擇** -- 選擇要使用的 WinUSB 介面卡
- **音訊裝置選擇** -- 選擇 WASAPI 回送擷取的來源
- **裝置位址** -- 手動輸入、從已配對的裝置中選取，或在設定檔對話方塊中使用 **掃描裝置**（以 USB 介面卡尋找附近的裝置）。輸入或貼上的位址會即時格式化：`AA:BB:CC:DD:EE:FF`、`aa-bb-cc-dd-ee-ff`、`AABBCCDDEEFF` 與 `AA BB CC DD EE FF` 都會變成 `AA:BB:CC:DD:EE:FF`
- **轉碼器選擇** -- 自動 / LDAC / aptX HD / aptX LL / aptX / SBC / AAC
- **LDAC 品質** -- HQ (990 kbps) / SQ (660 kbps) / MQ (330 kbps)
- **LHDC V5 品質** -- 高 (1000 kbps) / 中高 (900 kbps) / 標準 (500 kbps) / 行動 (400 kbps)，即 Android 提供的四個等級
- **ABR** -- 針對不穩定連線的自適應位元率開關（LDAC 與 LHDC V5 時顯示）
- **設定檔管理** -- 儲存與載入裝置 + 轉碼器設定
- **即時狀態** -- 轉碼器、位元率、連線狀態
- **連線品質** -- **操作 > 連線品質...**（請參閱 [連線品質視窗與 AFH](#link-quality)）
- **設定功能表** -- 語言、佈景主題、隨 Windows 啟動、最小化至系統匣、啟動時檢查更新、檢查更新時包含搶先版（也會通知 Beta 版本）、以耳機按鈕控制播放（請參閱 [耳機按鈕](#headphone-buttons)）、記住耳機音量（請參閱 [音量](#volume)）、偵錯模式（新增 **偵錯** 功能表：偵錯主控台、HCI 擷取、對端接收器測試）

## CLI 模式

加上 `--cli` 即可在沒有 GUI 視窗的情況下執行。

### 基本用法

```bash
# Auto-select best codec
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF
```

### 轉碼器選擇

```bash
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF -c ldac      # LDAC
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF -c aptxhd    # aptX HD
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF -c aptxll    # aptX Low Latency
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF -c aptx      # aptX
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF -c aac       # AAC
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF -c sbc       # SBC
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF -c lhdcv5    # LHDC V5
```

自動選擇的優先順序：LDAC > aptX HD > aptX LL > aptX > LHDC V5 > AAC > SBC

如果耳機不提供所要求的轉碼器，CLI 會改依「自動」的優先順序選擇。aptX Adaptive 永遠不會被選用 -- 請參閱 [aptX 系列與 aptX Adaptive 相容性](#aptx-compatibility)。

### LDAC 品質

```bash
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF -c ldac -q hq   # 990 kbps (default)
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF -c ldac -q sq   # 660 kbps
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF -c ldac -q mq   # 330 kbps

# LHDC V5: hq 1000 / mhq 900 / sq 500 / mq 400 kbps (Android's four levels)
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF -c lhdcv5 -q mhq  # 900 kbps
```

### ABR（自適應位元率）

```bash
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF -c ldac -a
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF -c lhdcv5 -a
```

ABR 會在藍牙連線不穩定時自動降低位元率，並在狀況改善時提高位元率。LDAC 會在其品質模式之間切換；LHDC V5 依照 Android 的策略，維持在 160 到 400 kbps 之間。

### 裝置探索

```bash
# List paired Bluetooth audio devices
A2DPWB.exe --cli -l
```

{: .note }
列出裝置時，會透過 **內建** 藍牙介面卡使用 Windows 藍牙 API，而不是 WinUSB 介面卡。

### 所有選項

| 選項 | 說明 |
|:-------|:------------|
| `-d <addr>` | 耳機的藍牙位址（`AA:BB:CC:DD:EE:FF`） |
| `-c <codec>` | `ldac`、`aptxhd`、`aptxll`、`aptx`、`aac`、`sbc`、`auto`（預設：`auto`） |
| `-q <mode>` | 品質：`hq`（預設）、`sq`、`mq`；LHDC V5 另有 `mhq`（900 kbps） |
| `-a` | ABR（LDAC 與 LHDC V5） |
| `-m <mode>` | 擷取模式：`loopback`（預設）、`virtual` |
| `--audio-device <id>` | `-m virtual` 使用的音訊裝置 ID |
| `-u <path>` | 介面卡的 USB 裝置路徑（連接多個介面卡時） |
| `-l` | 列出已配對的藍牙音訊裝置後結束 |
| `--max-packet <bytes>` | 媒體封包大小上限，679-1679（請參閱 [媒體封包大小上限](#max-packet)） |
| `--afh <auto\|off\|6,11>` | AFH 主機通道分類（請參閱 [連線品質視窗與 AFH](#link-quality)） |
| `--remote-sink <host[:port]\|auto>` | 顯示對端接收器收到的內容（請參閱 [對端接收器測試](#receiver)） |
| `-h` | 顯示說明 |

開發與測試用途另有 `--hci-tcp <host:port>`（以 TCP 連線的虛擬控制器取代 WinUSB 介面卡）、`--test-tone`（以左 1 kHz / 右 1.5 kHz 取代擷取的音訊）、`--duration <s>`（經過指定秒數後停止）與 `--hci-capture <file.pklg>`（擷取整個執行期間的 HCI）；請參閱 [建置](building#emu)。

在 GUI 模式下，`--minimized` 會讓 A2DPWB 在系統匣中啟動（**隨 Windows 啟動** 使用此選項）。

## 擷取模式 {#capture-modes}

A2DPWB 支援三種音訊擷取模式，各自錄製音訊路徑中的不同位置：

| 模式 | 錄製的內容 |
|:-----|:----------------|
| 系統回送 | Windows 預設輸出裝置播放的內容：所有應用程式混合後的音訊 |
| 虛擬裝置 | 應用程式播放到所選裝置（例如 VB-CABLE）的內容 |
| 應用程式 | 僅單一應用程式（及其子處理程序）播放的內容，經該應用程式本身處理後。需要 Windows 11 或 Windows 10 版本 2004（組建 19041）以上 |

設定檔對話方塊的 **擷取來源** 下方也有相同的說明。

系統回送是複製音訊，不會把音訊從預設輸出裝置上移走。音訊會持續在該裝置（喇叭、HDMI 電視等）以及藍牙耳機上播放。若只想從耳機聽到，請使用虛擬裝置模式，並選擇不會實際發聲的裝置，例如 VB-CABLE。

在虛擬裝置模式下，**自動切換預設裝置**（預設開啟）會在串流期間將所選裝置設為 Windows 預設輸出，讓應用程式不需變更 Windows 設定即可播放到該裝置。停止串流時，A2DPWB 會切換回先前的預設裝置。

- **音訊裝置** 清單只會顯示 A2DPWB 偵測為虛擬音訊裝置的裝置。
- 不會變更預設通訊裝置（通話應用程式使用的裝置）。
- 部分應用程式在重新啟動前，仍會在先前的裝置上播放。
- 如果 A2DPWB 意外結束，預設裝置不會切換回來。請在 Windows 音效設定中變更。

### 應用程式模式 {#application-mode}

應用程式模式會傳送單一應用程式的音訊，例如音樂播放器、瀏覽器或效果應用程式。請在 **應用程式** 清單中選擇應用程式。清單會顯示目前正在播放音訊的應用程式；若找不到您的應用程式，請先在其中開始播放，再按一下 **重新整理**。應用程式的子處理程序也包含在內，因此由獨立處理程序播放音訊的瀏覽器也能使用。

- 應用程式不必正在執行。A2DPWB 仍會連線到耳機，並在應用程式啟動後開始傳送。若應用程式在串流期間結束，A2DPWB 會保持連線，並在應用程式重新啟動時再次擷取。
- 應用程式也會繼續在其本身的輸出裝置上播放。若只想從耳機聽到，請將該輸出裝置靜音（主音量）：這不會影響 A2DPWB 擷取的內容。但請勿在 Windows 音量混音程式中將該應用程式靜音，否則耳機也會沒有聲音。
- Windows 音量混音程式中該應用程式的滑桿，會改變傳送到耳機（以及其輸出裝置）的音量。主音量與音量鍵不會改變傳送的音訊；若設定檔的 **Windows 音量同步** 設為「自動」，則會改為調整耳機的音量（請參閱 [音量](#volume)）。
- Windows 會將音訊轉換為 A2DPWB 要求的取樣率，因此不受音效設定中裝置格式的限制。
- 在 Windows 10 版本 2004（組建 19041）之前，Windows 不支援依應用程式擷取。設定檔對話方塊會將「應用程式」標示為此 PC 無法使用，以這種設定檔連線時會顯示錯誤。

### 音量 {#volume}

串流期間，若耳機支援 AVRCP 絕對音量，主視窗會顯示 **音量** 滑桿。它會設定耳機本身的音量，因此傳送的音訊能保持完整解析度。滑桿也會跟隨耳機上的音量按鈕與觸控操作。

設定檔中的 **Windows 音量同步** 會在串流期間將某個 Windows 音量與耳機音量合而為一：

| 選項 | 同步的 Windows 音量 |
|:-------|:----------------------|
| 自動（預設） | A2DPWB 擷取的裝置：系統回送與應用程式模式下為預設輸出裝置，虛擬裝置模式下為所選裝置 |
| 關閉 | 無：Windows 音量與耳機音量各自獨立 |
| 輸出裝置 | 該裝置的音量 |

- 耳機的音量按鈕與手勢，以及 **音量** 滑桿，會調整同步的 Windows 音量；而音效設定中該裝置的滑桿會調整耳機的音量。
- 音量鍵與 Windows 音量浮出視窗一律作用於預設輸出裝置，因此只有當預設輸出裝置就是同步的裝置時（如「自動」，但虛擬裝置模式且未開啟 **自動切換預設裝置** 時除外），才會改變耳機的音量。
- 開始串流時，耳機音量會套用 Windows 音量；停止串流時，裝置會恢復原本的音量。
- 使用「自動」時，音訊是在套用 Windows 音量之前擷取的，因此只會在耳機端降低一次音量。如果同步的音量也會改變擷取的音訊，音量就會套用兩次：此時請選擇「關閉」（請參閱 [系統層級效果應用程式](#effects-apps)）。
- 靜音不會同步：將輸出裝置靜音只會讓 PC 無聲，不會將耳機靜音。

**設定 > 記住耳機音量**（預設關閉）會為每個裝置儲存耳機音量，並在連線後、傳送任何音訊之前立即重新設定。否則耳機會以其本身上次的音量開始，這可能比上次搭配 A2DPWB 使用時大聲許多（例如在手機上使用過之後）。

### 耳機按鈕 {#headphone-buttons}

串流期間，耳機上的播放/暫停、下一首與上一首控制（按鈕或觸控手勢，例如 WH-1000XM4 上的點兩下）可控制 PC 上的播放：A2DPWB 會將它們以媒體鍵傳給 Windows，再送到 Windows 媒體控制項中顯示的應用程式（Spotify、瀏覽器、媒體播放器等）。停止也會一併傳遞。

- 播放與暫停都會當作 Windows 的播放/暫停鍵，因此即使耳機認知的狀態已過時，每次點按都一定會在播放與暫停之間切換。
- A2DPWB 會告知耳機是否正在播放音訊（依據是否有音訊可擷取），因此取下時會暫停的耳機能送出正確的指令。
- 快轉 / 倒轉（按住按鈕）沒有對應的 Windows 媒體鍵，因此會被忽略。
- 音量手勢會改變耳機本身的音量；啟用 **Windows 音量同步** 時，同步的 Windows 音量也會跟著改變（請參閱 [音量](#volume)）。
- 可透過 **設定 > 以耳機按鈕控制播放** 關閉此功能，例如當其他程式已經對媒體鍵作出反應、導致重複觸發時。

這需要耳機支援 AVRCP，幾乎所有耳機都支援。若不支援，串流仍會照常運作，只是無法使用按鈕。

### 系統層級效果應用程式 {#effects-apps}

系統層級效果應用程式（等化器、音效增強器、虛擬環繞音效應用程式等）通常會安裝自己的虛擬輸出裝置。其他應用程式播放到該裝置；效果應用程式處理音訊後，再將結果播放到您在效果應用程式中選擇的輸出裝置。耳機是否能聽到效果，取決於您選擇的 A2DPWB 模式：

| 設定 | 傳到耳機的內容 |
|:--------|:----------------------------|
| 虛擬裝置：效果應用程式本身的虛擬裝置 | 套用效果 **之前** 的音訊：應用程式送入效果應用程式的內容 |
| 應用程式：效果應用程式 | 套用效果 **之後** 的音訊：效果應用程式播放的內容（建議） |
| 虛擬裝置：效果應用程式的輸出裝置（例如設為其輸出的 VB-CABLE） | 套用效果之後的音訊（適用於不支援應用程式模式的 Windows 版本） |

在應用程式模式下，效果應用程式的輸出裝置可以是任何裝置：A2DPWB 擷取的是應用程式播放的內容，而不是裝置播放的內容，因此該裝置只是音訊的去處。為避免 PC 同時播放音訊，請選擇沒有人聽的輸出（例如螢幕的 HDMI 音訊），或將該裝置的主音量靜音。VB-CABLE 等虛擬裝置也可作為無聲的輸出目標，但並非必要。該裝置必須已啟用並已連接。

如果效果應用程式本身的虛擬裝置是 Windows 預設輸出裝置，其音量也會降低效果應用程式收到的音訊。此時若 **Windows 音量同步** 設為「自動」，音量變更就會套用兩次（在效果應用程式的輸入與耳機上）。請在該設定檔中將它設為「關閉」，並使用耳機音量或 **音量** 滑桿。

#### 為什麼 A2DPWB 沒有自己的虛擬輸出裝置 {#no-virtual-device}

要在 Windows 輸出裝置清單中出現一個可讓效果應用程式直接選擇的藍牙耳機項目，需要核心模式的音訊驅動程式。A2DPWB 並未包含這種驅動程式：

- 撰寫與維護核心音訊驅動程式的工作量很大，而且其中的錯誤可能導致 Windows 當機（藍色畫面）。
- Windows 10 與 11 只會載入由 Microsoft 簽署的驅動程式。取得該簽章需要 EV 程式碼簽署憑證（每年數百美元）與 Microsoft 合作夥伴中心帳戶，通常需以公司名義註冊。
- 唯一能繞過簽章的方式是 Windows 測試簽章模式，但它會降低系統安全性，並使部分軟體無法運作（例如具反作弊機制的遊戲）。A2DPWB 不會依賴此模式。

應用程式模式，或 VB-CABLE 等免費虛擬裝置，都能涵蓋相同的用途。

- Windows 沒有用來變更預設裝置的公開 API。A2DPWB 與其他音訊切換工具一樣，使用未公開的 IPolicyConfig 介面（Windows 10 與 11）。

{: .warning }
兩種模式都使用 WASAPI 共用模式，因此擷取的取樣率取決於 Windows 音效設定中該裝置的 **預設格式**（通常為 48 kHz）。若要以 96 kHz 使用 LDAC，請在 **音效設定 > 裝置內容 > 進階 > 預設格式** 中將輸出裝置的格式改為 96 kHz。

## 轉碼器比較

| 轉碼器 | 最適合 | 取捨 |
|:------|:---------|:----------|
| LDAC (HQ) | 最高音質 | 延遲較高，需要穩定的連線 |
| LDAC (ABR) | 音質 + 穩定性 | 位元率隨連線狀況變動 |
| aptX HD | 高音質，支援的裝置較多 | 固定 576 kbps |
| aptX LL | 遊戲、影片（低延遲） | 音質較低（16 位元） |
| aptX | 支援的 aptX 裝置最多 | 16 位元，352/384 kbps |
| AAC | Apple 裝置，效率佳 | 音質中等 |
| SBC | 相容性最高 | 音質最低 |

## aptX 系列與 aptX Adaptive 相容性 {#aptx-compatibility}

A2DPWB 支援三種 aptX 轉碼器，全部使用 [libopenaptx](https://github.com/pali/libopenaptx) 編碼。**不**支援 aptX Adaptive。

{: .warning }
**aptX HD 與 aptX Low Latency 為實驗性功能。** 兩者依照 Android / PipeWire 的實作，並通過編碼/解碼往返測試，但尚未在實際耳機上驗證。

{: .note }
**只需要傳統 aptX？** Windows 10 的內建藍牙堆疊已支援傳統 aptX（不含 aptX HD、aptX LL 或 aptX Adaptive），因此不需要 A2DPWB。A2DPWB 主要適用於 LDAC、aptX HD 與 aptX Low Latency。

| 轉碼器 | Vendor ID / Codec ID | RTP 標頭 | 狀態 |
|:------|:---------------------|:-----------|:-------|
| aptX | 0x0000004F / 0x0001 | 無 | 支援（44.1/48 kHz 時 352/384 kbps） |
| aptX HD | 0x000000D7 / 0x0024 | 有 | 支援（576 kbps，24 位元輸入） |
| aptX Low Latency | 0x0000000A 或 0x000000D7 / 0x0002 | 無 | 支援（352 kbps） |
| aptX Adaptive | 0x000000D7 / 0x00AD | -- | **不支援** -- 僅偵測並記錄 |

### 為什麼不支援 aptX Adaptive

目前沒有開放原始碼的 aptX Adaptive 編碼器。libopenaptx 只實作 aptX 與 aptX HD（A2DPWB 也將它用於 aptX Low Latency）。[arkq/openaptx](https://github.com/arkq/openaptx) 只重新實作 aptX / aptX HD，不含 aptX Adaptive 編碼器，且不允許散布二進位檔。因此 A2DPWB 只會偵測 aptX Adaptive 並寫入記錄。

### 不存在從 aptX Adaptive「降級」到 aptX 的情況

A2DPWB 連線時，耳機會公布一份 **轉碼器清單**（AVDTP 串流端點）。A2DPWB 從該清單中選擇一個轉碼器。aptX Adaptive 根本不會成為候選，因此也不存在從它「降級」的情況：

- **耳機同時列出傳統 aptX** -- 許多 aptX Adaptive 耳機都會如此，因為 Qualcomm 宣傳 aptX Adaptive 向下相容 aptX / aptX HD。此時 A2DPWB 會直接選擇傳統 aptX；它是耳機清單中的另一個轉碼器，而不是 aptX Adaptive 的降級模式。
- **耳機只列出 aptX Adaptive**（沒有傳統 aptX、aptX HD 或 aptX LL）-- A2DPWB 無法使用 aptX。「自動」會依優先順序選擇下一個轉碼器，通常是 AAC 或 SBC。

自動選擇的優先順序：LDAC > aptX HD > aptX LL > aptX > LHDC V5 > AAC > SBC

### 明確指定的轉碼器不在清單中 {#codec-fallback}

當您指定的轉碼器不在耳機的清單中時：

| 模式 | 行為 |
|:-----|:---------|
| GUI，已勾選 **無法使用此轉碼器時改用其他轉碼器**（預設） | 依「自動」的優先順序，以耳機提供的最佳轉碼器連線，狀態列會顯示改用了哪個轉碼器及原因。 |
| GUI，未勾選此選項 | 連線會因錯誤而停止（「裝置不支援 ...」）。如果您指定了 aptX、aptX HD 或 aptX LL，而耳機只提供 aptX Adaptive，訊息會說明此情況，並建議使用自動、AAC 或 SBC。 |
| CLI（`-c <codec>`） | 輸出 `Requested codec ... not available, falling back...`，並依「自動」的優先順序繼續。如果耳機提供 aptX Adaptive，會另外輸出一行說明它永遠不會被選用。 |

此選項位於 **編輯設定檔** 中的轉碼器旁（選擇「自動」時不使用）。

### 耳機連線到其他裝置時缺少轉碼器 {#sep-in-use}

耳機會在串流端點 (SEP) 上提供各個轉碼器。在雙重（多點）連線時，正在串流到手機或 PC 的端點會被回報為 **使用中**，在該串流停止之前無法使用其轉碼器。有些耳機也只在單一裝置連線時才提供高解析度轉碼器（LDAC、L2HC 等）。此時 A2DPWB 會回報：

```
BTstack: Remote SEP SEID=6 sink audio, IN USE (codec not queried)
BTstack: Remote SEPs in use by another connection: 1 of 6 audio sink SEP(s); ...
BTstack: Capability discovery complete (LDAC=0, ..., 1 of 6 audio sink SEP(s) in use)
```

錯誤訊息也會說明端點正由其他裝置使用。請中斷耳機與手機 / PC 的連線（或在耳機的應用程式中關閉雙重連線），然後再次連線。內建藍牙仍與耳機配對的 PC 也算是其他裝置。

### 取樣率

aptX、aptX HD 與 aptX LL 只支援 44.1 kHz 與 48 kHz。A2DPWB 會從耳機公布的取樣率中選擇一個（優先採用目前 Windows 裝置的取樣率，否則為 48 kHz），並讓 WASAPI 對擷取的音訊重新取樣。使用 aptX 時不需要變更 Windows 音效設定。

### 檢查耳機提供的轉碼器

1. 在 **設定** 功能表中勾選 **偵錯模式 (debug.log / HCI 記錄)**，然後重新啟動 A2DPWB。
2. 連線到耳機。
3. 開啟設定資料夾（`%APPDATA%\A2DPWB`）中的 `debug.log`。在 CLI 模式下，相同的內容會寫入標準錯誤輸出。

在 GUI 中，**偵錯 > 偵錯主控台...**（Ctrl+Shift+D）會即時顯示相同的記錄。其 **連線流程** 清單會將最近一次連線嘗試的各個階段（介面卡初始化、ACL / AVDTP 連線、功能探索、轉碼器選擇、WASAPI、串流設定、編碼器、串流開始、音訊傳送、中斷連線 / 重新連線）標示為 OK、警告或失敗；按兩下步驟可跳到其來源的記錄行。下方的記錄可依層級與文字篩選。**複製診斷資訊** 會將版本、作業系統、介面卡、設定檔、連線流程、警告 / 錯誤與最近的記錄複製到剪貼簿，可直接貼到錯誤回報中（藍牙位址預設會遮蔽到只剩廠商部分）。

功能相關的記錄行會確切列出耳機提供的轉碼器：

```
BTstack: Remote supports aptX (SEID=..., caps=0x..)
BTstack: Remote supports aptX Adaptive (SEID=...) — never selected (no open encoder); classic aptX is used only if the remote lists it
BTstack: Remote vendor codec vid=0x........ cid=0x.... (SEID=...) — unsupported
BTstack: Capability discovery complete (LDAC=0, aptXHD=0, aptXLL=0, aptX=1, aptXAdaptive=1 [not encodable], SBC=1, AAC=1)
```

- `aptX=1` -- 可使用傳統 aptX（即使同時為 `aptXAdaptive=1`）。
- `aptX=0`、`aptXHD=0`、`aptXLL=0` 且 `aptXAdaptive=1` -- 耳機只提供 aptX Adaptive；請使用自動、AAC 或 SBC。
- `Remote vendor codec ... — unsupported` -- A2DPWB 未實作的其他廠商轉碼器。

## 媒體封包大小上限（依設定檔） {#max-packet}

每個設定檔中的 **媒體封包大小上限**（**編輯設定檔**；CLI：`--max-packet <bytes>`）設定每個藍牙封包中音訊資料的上限，範圍為 679 到 1679 位元組，讓需要不同數值的耳機可以有各自的設定。耳機本身的上限 (MTU) 也一律適用；A2DPWB 會採用兩者中較小的值。變更會從下次連線開始生效。舊版儲存的設定檔會沿用先前的全域設定值。

**建議使用 1023（預設）。** 它剛好可放入一個藍牙基頻封包 (3-DH5)。較大的值只能節省 1-2% 的額外負擔，而且每遺失一個封包就會遺失更多音訊。679 剛好可放入一個 2-DH5 封包，也是 LDAC 接受的最小值。

## 驗證媒體串流 (a2dpwb_decode) {#verify-stream}

大多數耳機無法顯示正在使用的轉碼器。在偵錯模式下，GUI 會有 **偵錯** 功能表，內含 **開始 HCI 擷取** / **停止 HCI 擷取**。擷取檔是 HCI 封包記錄（設定資料夾中的 `hci_<date>_<time>.pklg`），包含轉碼器協商與傳送的每個媒體封包。以 LDAC 990 kbps 時每小時約增加 0.5 GB，因此請只擷取所需的時間。

您可以在連線前或串流期間開始擷取。若在連線期間開始，A2DPWB 會先寫入它記住的連線建立封包（L2CAP 通道建立與 AVDTP 訊號，包含轉碼器協商），因此檔案仍可分析。停止後，A2DPWB 會顯示檔案路徑。

可在 Wireshark 中開啟擷取檔，或使用 `a2dpwb_decode` 開發者工具檢查（需從原始碼建置，請參閱 [建置](building)）：

```
a2dpwb_decode "%APPDATA%\A2DPWB\hci_20260925_120000.pklg" -o C:\temp\check
```

此工具會追蹤 AVDTP 訊號（SET_CONFIGURATION / RECONFIGURE 以及耳機是否接受），擷取 A2DPWB 傳送的媒體封包，並依協商的設定進行檢查：

| 轉碼器 | 檢查內容 | 輸出 |
|:------|:----------------|:-------|
| SBC, AAC, aptX, aptX HD, aptX LL | 以參考解碼器完整解碼；比對訊框參數與設定；媒體酬載標頭中的訊框數 | 可試聽的 `<prefix>.<n>.<codec>.wav` |
| LDAC | 僅訊框標頭（同步字、取樣率、聲道模式、訊框長度）、訊框數、位元率。目前沒有開放原始碼的 LDAC 解碼器 | 僅報告 |

對每個串流，它也會回報 RTP 序號缺漏、RTP 時間戳記的前進速度、位元率，以及藍牙控制器回報為已完成的 ACL 封包數。最後一行為 `RESULT: OK` 或 `RESULT: PROBLEMS FOUND`（結束代碼 0 或 1）。

{: .note }
這能顯示 A2DPWB 傳送了什麼，以及這些內容對協商的轉碼器是否有效。由於接收端只能解碼協商好的轉碼器，若耳機播放的聲音正確且報告沒有問題，就能有力證明確實使用了該轉碼器。

## 連線品質視窗與 AFH {#link-quality}

**操作 > 連線品質...** 會顯示 A2DPWB 在串流期間傳送的內容：轉碼器與位元率、已傳送的封包、傳送前捨棄的封包（傳送佇列已滿：無線連結跟不上）、編碼器來不及處理的音訊、傳送佇列，以及 **無線電**：連線的 RSSI（相對於控制器的黃金接收範圍；0 = 良好，負值 = 太弱；並非絕對值），以及連結在 79 個藍牙通道中跳頻使用了多少個 (AFH)。

藍牙會自行避開繁忙的通道 (AFH)，但它只能從錯誤中得知 Wi-Fi 的存在。A2DPWB 也會在每次連線時告知控制器應避開哪些通道（AFH 主機通道分類）：

| `afh` | |
|:--|:--|
| `auto`（預設） | 此 PC 收到強訊號的 2.4 GHz Wi-Fi 網路（Windows Wi-Fi 掃描，連線期間會重複執行） |
| `off` | 不指定：由控制器自行決定 |
| `6,11` | 這些 Wi-Fi 通道 |

至少一律保留 20 個藍牙通道。可透過 `settings.json`（設定資料夾）中的 `"afh"`、對端接收器測試視窗（設定會儲存在此），或 CLI 模式的 `--afh` 設定。「無線電」列會顯示避開了哪些 Wi-Fi 通道。是否有幫助取決於環境：請使用對端接收器測試來測量。

## 對端接收器測試 (tools/linux_sink) {#receiver}

連線品質視窗顯示的是 A2DPWB 傳送的內容。若要查看實際抵達的內容，請在另一台 Linux PC 上執行 `tools/linux_sink/a2dpwb_sink.py`。它會直接驅動該 PC 的一般藍牙介面卡（內建的即可）（透過 HCI user channel 使用 Bumble），作為支援所有轉碼器（包括 LDAC）的 A2DP 接收端，測量每個抵達的媒體封包，並透過網路提供統計資料：

將 `tools/linux_sink` 資料夾複製到 Linux PC（只需此資料夾），然後執行：

```bash
sh setup.sh      # once: decoder libraries, Python environment (Ubuntu / Debian, Python 3.11+)
sudo sh run.sh   # start the receiver (root: it takes the Bluetooth adapter)
```

在偵錯模式下，**偵錯 > 對端接收器測試...** 會自動在區域網路上尋找接收器。**開始** 會以所選的轉碼器、品質、取樣率與位元深度，將穩定的測試音（或系統音訊）串流到接收器的藍牙介面卡 -- 不需要設定檔或配對步驟 -- 視窗會並列顯示傳送與抵達的內容：位元率、遺失與延遲的封包（RTP 序號）、抖動與間隔、模擬播放緩衝區的斷音與跳過、串流 / 解碼錯誤，以及接收端的 RSSI（相對於其黃金接收範圍：0 = 良好，負值 = 太弱）。每個步驟（尋找接收器、統計資料、接收器設定、藍牙連線、轉碼器、音訊抵達）都會顯示為 OK / NG，並附有可複製的記錄。在 CLI 模式下，`--remote-sink auto` 的作用相同：它會尋找接收器，除非 `-d` 指定了其他裝置，否則會串流到該接收器，並每 5 秒及結束時輸出統計資料。

兩端都從串流的第一個封包開始計算封包數，按下 **停止** 後最終計數會持續顯示（「停止時」；接收端的數值約晚一秒抵達），因此可以比較傳送與接收的封包數：兩者應該相等。藍牙會在控制器內部重新傳送遺失的無線封包，而且 A2DPWB 在連結存續期間不會放棄任何封包，因此無線連結不佳時，會在接收端呈現為間隔、抖動與斷音，而不是封包遺失；當連結跟不上時，傳送端會在傳送前捨棄封包。兩端都無法得知重新傳送的次數。

工具執行期間，接收端的介面卡無法供其本身的藍牙堆疊使用。探索使用 UDP 連接埠 51201 的廣播，因此兩台 PC 必須位於相同的網段（否則請輸入位址）。所有選項與統計通訊協定請參閱 `tools/linux_sink/README.md`。

## 疑難排解

### 藍牙介面卡沒有回應 {#adapter-no-answer}

如果介面卡開啟後沒有任何回傳（不回應 HCI Reset），A2DPWB 會在 5 秒後放棄，再開啟一次；若仍無回應，會顯示「藍牙介面卡沒有回應」。請拔下介面卡，重新插入後再試一次。不需要重新啟動 A2DPWB：啟動失敗時會釋放介面卡，每次重新啟動都會再次初始化介面卡。

為避免介面卡卡住，A2DPWB 在開啟介面卡時會重設其 USB 管道，關閉時會傳送 HCI Reset。

### 更新後的配對問題

A2DPWB 使用 SSP (Just Works) 搭配 **General Bonding** 進行配對，並儲存連結金鑰，讓耳機不需重新配對即可重新連線。如果您曾用舊版 A2DPWB 與耳機配對，請在耳機上移除 A2DPWB 的配對（清除配對清單的方法請參閱耳機說明書），然後重新配對。

### 連線逾時

A2DPWB 會註冊 SDP 記錄（A2DP Source、AVRCP Controller / Target），讓耳機能將它識別為音訊來源。如果連線嘗試逾時，半開的連線會被拆除，因此您可以在同一工作階段中直接重試，不需重新啟動 A2DPWB。請確認耳機已開機、在範圍內，且未連線到其他裝置。

當連結建立後沒有傳輸任何資料，或耳機沒有回應時，失敗的連線會自動重試（最多 3 次）。如果耳機已不認得配對（已重設，或已與其他 PC 配對），會捨棄儲存的金鑰並重新配對一次。之後錯誤訊息會說明發生了什麼事：

| 訊息 | 意義 |
|:--|:--|
| 裝置沒有回應 | 未開機、超出範圍，或無法連線（新裝置：需處於配對模式） |
| 連線已建立，但之後沒有傳輸任何資料 | 無線電環境：請再試一次，或移動介面卡 / 裝置 |
| 裝置有回應，但隨後自行結束連線 | 裝置目前不接受連線：正在關機、充電或已連線到其他裝置 |
| 驗證失敗，重新配對後仍然失敗 | 將裝置切換到配對模式，或移除其配對清單項目，然後再試一次 |
