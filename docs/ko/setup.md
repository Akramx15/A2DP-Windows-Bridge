---
title: 설치
layout: default
parent: 홈
nav_order: 1
---

# 설치
{: .no_toc }

## 목차
{: .no_toc .text-delta }

1. TOC
{:toc}

---

## 준비 사항

- **Windows 10/11** (x64)
- **전용 USB Bluetooth 어댑터** (내장 Bluetooth와 별도)
- **Zadig** ([https://zadig.akeo.ie/](https://zadig.akeo.ie/))

## A2DPWB 설치

각 [릴리스](https://github.com/SeiyaFunaokaJP/A2DP-Windows-Bridge/releases/latest)는 같은 파일을 담은 두 가지 형식으로 제공됩니다.

- **설치 프로그램** (`A2DPWB-<version>-win64-setup.exe`): 관리자 권한 없이 현재 사용자용으로 설치하며(선택하면 모든 사용자용으로도 설치 가능), 시작 메뉴 항목과 제거 프로그램을 추가합니다. 처음 설치할 때는 설치 과정에서 선택한 언어로 A2DPWB를 시작합니다
- **포터블** (`A2DPWB-<version>-win64-portable.zip`): 원하는 위치에 압축을 풀고 `A2DPWB.exe`를 실행합니다

두 형식 모두 설정, 프로필, 링크 키, 펌웨어를 `%APPDATA%\A2DPWB`에 저장하므로 서로 전환해서 사용할 수 있습니다. 제거해도 이 폴더는 남아 있습니다.

## 1단계: WinUSB 드라이버 설치

A2DPWB는 WinUSB를 통해 USB Bluetooth 어댑터와 직접 통신합니다. **전용 어댑터**가 필요하며, 내장 Bluetooth는 일반 Windows Bluetooth로 계속 정상 작동합니다.

1. USB Bluetooth 어댑터를 꽂습니다
2. [Zadig](https://zadig.akeo.ie/)를 다운로드하여 엽니다
3. **Options > List All Devices**를 선택합니다
4. 드롭다운에서 USB Bluetooth 어댑터를 선택합니다
5. 대상 드라이버로 **WinUSB**를 선택합니다
6. **Replace Driver**를 클릭합니다

{: .warning }
어댑터가 WinUSB 모드인 동안에는 Windows가 일반 Bluetooth 용도로 사용할 수 없습니다. 일반 주변 장치(키보드, 마우스 등)에는 내장 Bluetooth를 사용하십시오.

## 2단계: 헤드폰의 Bluetooth 주소 확인

헤드폰/스피커의 Bluetooth MAC 주소가 필요합니다.

**Windows 설정에서:**
1. **설정 > Bluetooth 및 장치**를 엽니다
2. 오디오 장치를 클릭합니다
3. **속성**을 클릭합니다
4. Bluetooth 주소가 표시됩니다 (형식: `AA:BB:CC:DD:EE:FF`)

**A2DPWB GUI에서:**
- `A2DPWB.exe`를 실행하면 페어링된 Bluetooth 오디오 장치가 장치 드롭다운에 표시됩니다
- 또는 프로필 대화 상자의 **장치 검색**으로 USB 어댑터를 사용해 주변 장치를 찾습니다(헤드폰을 페어링 모드로 전환하십시오)

**CLI에서:**
```
A2DPWB.exe --cli -l
```

## 3단계: A2DPWB 실행

**GUI:**
```
A2DPWB.exe
```

**CLI:**
```
A2DPWB.exe --cli -d AA:BB:CC:DD:EE:FF
```

자세한 내용은 [사용법](usage)을 참조하십시오.

---

## 펌웨어 (Realtek 어댑터)

Realtek 기반 USB Bluetooth 어댑터(예: TP-Link UB500, RTL8761BU 동글)는 작동하려면 독점 펌웨어가 필요합니다. CSR 어댑터는 펌웨어가 필요 없습니다. Intel 및 Broadcom 어댑터(실험적)는 자체 펌웨어 파일이 필요할 수 있습니다. [권장 어댑터](#adapters)를 참조하십시오.

### GUI (안내형 다운로드)

`A2DPWB.exe`를 실행하고 **작업 > Bluetooth 펌웨어 선택**을 선택합니다. 펌웨어가 없으면 경고가 표시됩니다. **다운로드 페이지 열기** 버튼으로 브라우저에서 [linux-firmware/rtl_bt](https://git.kernel.org/pub/scm/linux/kernel/git/firmware/linux-firmware.git/tree/rtl_bt) 페이지를 열고, **설정 폴더 열기**로 저장할 폴더를 엽니다. 필요한 `.bin` 파일을 다운로드하여 설정 폴더에 넣습니다.

대화 상자는 어댑터 칩셋을 자동으로 감지하고 필요한 펌웨어 파일을 표시합니다.

### Bluetooth 칩 설정

같은 대화 상자의 **Bluetooth 칩** 목록은 A2DPWB가 어댑터에 Realtek 초기화(펌웨어 업로드)를 수행할지 결정합니다.

| 선택 | 의미 |
|:-----|:-----|
| **자동** (기본값) | A2DPWB를 시작할 때마다 감지된 어댑터를 기준으로 결정합니다. 알려진 Realtek 어댑터에는 해당 칩이 선택되고, 그 밖의 어댑터에는 Realtek 초기화를 수행하지 않습니다 |
| **Realtek 아님** | Realtek 초기화를 수행하지 않습니다. 자동이 어댑터를 인식하지 못할 때 Intel, Broadcom, CSR 및 기타 어댑터에 사용합니다 |
| **RTL8761BU**, **RTL8822CU**, ... | 해당 Realtek 칩입니다. 자동이 인식하지 못하는 Realtek 기반 어댑터(자체 VID:PID를 쓰는 OEM 제품)에 필요합니다. 상태 표시줄에 해당 `.bin` 파일이 설정 폴더에 있는지 표시됩니다 |
| **Custom** | 설정 폴더의 `custom_fw.bin`과 `custom_config.bin`을 사용합니다 |

Realtek이 아닌 어댑터에는 Realtek 칩이나 Custom을 선택하지 마십시오. A2DPWB가 Realtek 펌웨어를 업로드하려고 시도하여 어댑터가 시작되지 않을 수 있습니다.

### 수동 다운로드

1. [linux-firmware/rtl_bt](https://git.kernel.org/pub/scm/linux/kernel/git/firmware/linux-firmware.git/tree/rtl_bt)에서 칩셋에 맞는 펌웨어 및 config `.bin` 파일을 다운로드합니다 (예: `rtl8761bu_fw.bin`과 `rtl8761bu_config.bin`)
2. 두 파일을 A2DPWB 설정 폴더(`%APPDATA%\A2DPWB` 또는 펌웨어 대화 상자에 표시된 경로)에 넣습니다

{: .note }
이 펌웨어 파일은 linux-firmware 프로젝트를 통해 배포되는 Realtek 독점 바이너리이며, 이 저장소에는 포함되어 있지 않습니다. 재배포 조건은 [WHENCE](https://git.kernel.org/pub/scm/linux/kernel/git/firmware/linux-firmware.git/tree/WHENCE)를 참조하십시오.

---

## 권장 어댑터 {#adapters}

| 칩셋 | 제품 예 | 참고 |
|:-----|:--------|:-----|
| Realtek | TP-Link UB500, RTL8761BU | 펌웨어 다운로드 필요(위 참조). 가장 많이 테스트됨 |
| CSR | 일반 CSR8510 동글 | 바로 작동하며 펌웨어 불필요 |
| Broadcom | ASUS BT400 (BCM20702) | *실험적.* Bluetooth `E0/01/01` 대신 벤더 클래스 `FF/01/01`을 보고하는 어댑터(BCM20702 OEM 동글)도 감지됩니다. ROM 펌웨어로 작동하며, 설정 폴더에 선택적인 PatchRAM `.hcd`(예: `BCM20702A1-0b05-17cb.hcd`)가 있으면 시작 시 업로드됩니다 |
| Intel | Intel 8265 / 9260 / AX200 / AX201 | *실험적.* 부트로더 모드(예: 콜드 부팅 직후)에서는 linux-firmware `intel/`의 해당 `ibt-*.sfi` + `ibt-*.ddc`가 설정 폴더에 필요합니다. AX210 이후 모델(TLV 부트로더)은 아직 지원하지 않습니다 |
| MediaTek, Qualcomm | MT7921/MT7922, QCA61x4 | 미지원: A2DPWB에 없는 펌웨어 로더가 필요합니다 |

LDAC 같은 고비트레이트 코덱에는 일반적으로 USB Bluetooth 5.0 이상 어댑터가 가장 적합합니다.
