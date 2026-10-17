---
title: セットアップ
layout: default
parent: 日本語
nav_order: 1
---

# セットアップ
{: .no_toc }

## 目次
{: .no_toc .text-delta }

1. TOC
{:toc}

---

## 前提条件

- **Windows 10 (22H2) / 11** (x64)
- **専用 USB Bluetooth アダプター**（内蔵 Bluetooth とは別）

A2DPWB はアダプターへの WinUSB ドライバーの割り当てを自分で行います。[Zadig](https://zadig.akeo.ie/) はもう必要ありません（使うこともできます。[Zadig を使う場合](#zadig)を参照）。

## A2DPWB のインストール

[リリース](https://github.com/SeiyaFunaokaJP/A2DP-Windows-Bridge/releases/latest)ごとに、中身が同じ 2 つの形式があります。

- **インストーラー版**（`A2DPWB-<バージョン>-win64-setup.exe`）：管理者権限なしで現在のユーザーにインストールします。スタートメニュー（**A2DP Windows Bridge**）とアンインストーラーを追加し、Microsoft Edge WebView2 ランタイムがなければ一緒にインストールします（Windows 11 には標準で入っています）。更新はアプリの中から入れられます（**設定 > 更新 > 今すぐ更新**）
- **ポータブル版**（`A2DPWB-<バージョン>-win64-portable.zip`）：任意の場所に展開して `a2dpwb.exe` を実行します。3 つのプログラム（`a2dpwb.exe`、`a2dpwb-core.exe`、`a2dpwb-driver.exe`）は同じフォルダーに置いたままにしてください。ポータブル版は更新を知らせるだけです。WebView2 ランタイムが入っている必要があります

どちらも設定・プロファイル・リンクキー・ファームウェアを `%APPDATA%\A2DPWB` に保存するため、相互に切り替えられます。アンインストールしてもこのフォルダーは残ります。

### v1 からの乗り換え {#from-v1}

v1 の上から v2 のインストーラーを実行してください。v1 が動いていれば終了を促し、v1 をアンインストールしてから、設定フォルダーはそのまま残します。プロファイル、ヘッドホンとのペアリング、ファームウェアのファイルは引き継がれ、ヘッドホンのペアリングし直しは要りません。（Zadig で）WinUSB を入れてあるアダプターはそのまま使えます。**Windows 起動時に開始** がオンだった場合は、v2 が起動するように変わります。

v2 は初回起動時に、v1 の設定を `%APPDATA%\A2DPWB\backup-v1\` にコピーしてから読み込みます。

## 手順 1: 初回セットアップ {#first-run}

USB Bluetooth アダプターを挿して A2DPWB を起動します。プロファイルがまだないとき、または WinUSB のアダプターがないときは、**A2DPWB のセットアップ** が開きます。手順は 4 つです。

1. **アダプタを選ぶ**: 見つかった USB Bluetooth アダプターの一覧です。見当たらなければ、アダプターを挿してから **再検出** を押します
2. **ドライバとファームウェア**: **WinUSB を割り当てる**（[手順 2](#winusb)を参照）。Realtek のアダプターはファームウェアのファイルも用意します（[ファームウェア](#firmware)を参照）
3. **ヘッドホンを探す**: ヘッドホンをペアリングモードにして **デバイスを検索** し、選んでから **プロファイルを作成** します（[手順 3](#headphones)を参照）
4. **テスト**: 接続してから **テスト音を鳴らす**。右耳に高めの音、左耳に低めの音が聞こえます

**あとで設定する** で残りを飛ばせます。**設定 > セットアップを実行** でもう一度開けます。

## 手順 2: アダプターに WinUSB を割り当てる {#winusb}

A2DPWB は、Windows に標準で入っている汎用 USB ドライバー WinUSB を通して USB Bluetooth アダプターと直接通信します。**専用のアダプター**が必要です。内蔵 Bluetooth は通常の Windows Bluetooth として引き続き使用できます。

1. **アダプタ** 画面（またはセットアップの **ドライバとファームウェア**）を開きます
2. アダプターの **ドライバ** が **Windows の Bluetooth**（または **ほかのドライバ** / **ドライバなし**）になっています。**WinUSB を割り当てる** を押します
3. 確認の内容を読んで続けると、Windows が管理者の許可（UAC）を求めます
4. ドライバが **WinUSB（A2DPWB が使用）** になります

{: .warning }
WinUSB を割り当てている間、Windows の Bluetooth からそのアダプターは見えなくなり、そのアダプターで Windows にペアリングしたデバイスはドライバーを戻すまで使えません。通常の周辺機器（キーボード、マウス等）には内蔵 Bluetooth を使用し、**PC 内蔵の Bluetooth アダプターには絶対に WinUSB を割り当てないでください**。

同じ画面の **元のドライバに戻す** で、アダプターを Windows に戻せます。Windows がドライバー（通常は Windows の Bluetooth ドライバー）を選び直し、Windows の Bluetooth でそのアダプターを使えるようになります。WinUSB を割り当て直すまで、A2DPWB はそのアダプターで配信できません。アダプターを Windows でまた使うつもりなら、A2DPWB をアンインストールする前にドライバーを戻してください。

### Zadig を使う場合 {#zadig}

[Zadig](https://zadig.akeo.ie/) で入れた WinUSB ドライバーでも同じように動きます。

1. Zadig を開き、**Options > List All Devices** を選択
2. ドロップダウンから USB Bluetooth アダプターを選択
3. ターゲットドライバーとして **WinUSB** を選び、**Replace Driver** をクリック

## 手順 3: ヘッドホンを探す {#headphones}

**デバイスを検索**（セットアップとプロファイルの編集画面にあります）は、USB アダプターで近くのデバイスを約 10 秒探します。先にヘッドホンをペアリングモードにしてください。A2DPWB で以前ペアリングしたデバイスには **ペアリング済み** と付きます。

検索で見つからないときは、**手動入力:** に Bluetooth アドレスを入れて **使う** を押します。入力・貼り付けたアドレスはその場で整形されます: `AA:BB:CC:DD:EE:FF`、`aa-bb-cc-dd-ee-ff`、`AABBCCDDEEFF`、`AA BB CC DD EE FF` はどれも `AA:BB:CC:DD:EE:FF` になります。

**Windows 設定から確認**（ヘッドホンを PC 内蔵の Bluetooth とペアリングしている場合）:
1. **設定 > Bluetooth とデバイス** を開く
2. オーディオデバイスをクリック
3. **プロパティ** をクリック
4. Bluetooth アドレスが表示される（形式: `AA:BB:CC:DD:EE:FF`）

**コマンドラインから確認**（先に A2DPWB を終了してください。トレイ > **終了**）:
```
a2dpwb-core.exe --cli --scan
```

新しいプロファイルは、コーデックが **自動**、キャプチャソースがシステムの音で始まります。初期設定のままで一番良い音になります。そのほかは[使い方](usage)をご覧ください。

---

## ファームウェア（Realtek アダプター） {#firmware}

Realtek ベースの USB Bluetooth アダプター（TP-Link UB500、RTL8761BU ドングル等）は動作に専用ファームウェアが必要です。CSR アダプターはファームウェア不要です。Intel と Broadcom のアダプター（試験的）は、それぞれのファームウェアファイルが必要な場合があります。[推奨アダプター](#adapters)を参照してください。

### アプリでの案内（ガイド付きダウンロード）

**アダプタ** 画面（とセットアップの **ドライバとファームウェア**）に、アダプターのファームウェアの状態が出ます: **ファームウェアOK**、**ファームウェアのファイルは不要です**、または必要なファイル名付きの **ファームウェアが見つかりません**。**ダウンロードページを開く** ボタンでブラウザの [linux-firmware/rtl_bt](https://git.kernel.org/pub/scm/linux/kernel/git/firmware/linux-firmware.git/tree/rtl_bt) ページを開き、**設定フォルダを開く** で保存先フォルダーを開きます。必要な `.bin` ファイルをダウンロードしてフォルダーに配置します。

A2DPWB はアダプターのチップセットを検出し、必要なファームウェアファイル名を表示します。

### Bluetooth チップの設定

**アダプタ** 画面の **Bluetooth チップ** の一覧で、アダプターに Realtek の初期化（ファームウェアの書き込み）を行うかどうかを決めます。

| 選択肢 | 意味 |
|:-------|:-----|
| **自動**（既定） | A2DPWB の起動ごとに、検出したアダプターから判定します。既知の Realtek アダプターならそのチップ、それ以外のアダプターなら Realtek の初期化を行いません |
| **Realtek 以外** | Realtek の初期化を行いません。Intel、Broadcom、CSR などのアダプターを自動で判別できない場合に選びます |
| **RTL8761BU**、**RTL8822CU** など | その Realtek チップとして扱います。自動で判別できない Realtek 系アダプター（独自の VID:PID を持つ OEM 製品）で必要です。状態欄に `.bin` ファイルが設定フォルダーにあるかが表示されます |
| **Custom** | 設定フォルダーの `custom_fw.bin` と `custom_config.bin` を使います |

Realtek 以外のアダプターに Realtek チップや Custom を選ばないでください。Realtek ファームウェアを書き込もうとするため、アダプターが起動しない場合があります。

### 手動ダウンロード

1. [linux-firmware/rtl_bt](https://git.kernel.org/pub/scm/linux/kernel/git/firmware/linux-firmware.git/tree/rtl_bt) からチップセット用のファームウェアと設定の `.bin` ファイルをダウンロード（例: `rtl8761bu_fw.bin` と `rtl8761bu_config.bin`）
2. A2DPWB 設定フォルダー（`%APPDATA%\A2DPWB`。**設定 > 設定フォルダ** にも表示されます）に両ファイルを配置

{: .note }
これらのファームウェアファイルは linux-firmware プロジェクト経由で配布される Realtek 独自のバイナリです。本リポジトリには含まれていません。再配布条件については [WHENCE](https://git.kernel.org/pub/scm/linux/kernel/git/firmware/linux-firmware.git/tree/WHENCE) を参照してください。

---

## 推奨アダプター {#adapters}

| チップセット | 製品例 | 備考 |
|:-------------|:-------|:-----|
| Realtek | TP-Link UB500、RTL8761BU | ファームウェアのダウンロードが必要（上記参照）。最も検証済み |
| CSR | CSR8510 汎用ドングル | そのまま動作、ファームウェア不要 |
| Broadcom | ASUS BT400 (BCM20702) | *試験的。* Bluetooth クラス `E0/01/01` ではなくベンダークラス `FF/01/01` を名乗るアダプタ（BCM20702 の OEM ドングル）も検出。ROM ファームウェアで動作。設定フォルダに PatchRAM `.hcd`（例: `BCM20702A1-0b05-17cb.hcd`）があれば起動時に適用 |
| Intel | Intel 8265 / 9260 / AX200 / AX201 | *試験的。* ブートローダーモード（コールドブート後など）では linux-firmware `intel/` の対応する `ibt-*.sfi` + `ibt-*.ddc` を設定フォルダに配置する必要あり。AX210 以降（TLV ブートローダー）は未対応 |
| MediaTek、Qualcomm | MT7921/MT7922、QCA61x4 | 未対応: A2DPWB にないファームウェアローダーが必要 |

USB Bluetooth 5.0 以上のアダプターは、LDAC のような高ビットレートコーデックに最適です。

複数のアダプターに WinUSB を割り当てていると、A2DPWB は最初に見つけたものを使います。WinUSB は A2DPWB で使うアダプターだけにしてください。
