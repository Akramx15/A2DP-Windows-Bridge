---
title: 라이선스
layout: default
parent: 홈
nav_order: 5
---

# 라이선스
{: .no_toc }

## 목차
{: .no_toc .text-delta }

1. TOC
{:toc}

---

## 프로젝트 라이선스

A2DP Windows Bridge(A2DPWB)는 **Apache License 2.0** 또는 **MIT License** 중 원하는 것을 선택할 수 있는 이중 라이선스입니다(SPDX `Apache-2.0 OR MIT`). [LICENSE](https://github.com/SeiyaFunaokaJP/A2DP-Windows-Bridge/blob/main/LICENSE), [LICENSE-APACHE](https://github.com/SeiyaFunaokaJP/A2DP-Windows-Bridge/blob/main/LICENSE-APACHE), [LICENSE-MIT](https://github.com/SeiyaFunaokaJP/A2DP-Windows-Bridge/blob/main/LICENSE-MIT)를 참조하십시오.

{: .warning }
A2DPWB 소스 코드는 Apache-2.0 또는 MIT 이중 라이선스입니다. 배포되는 `A2DPWB.exe`에는 BTstack이 포함되어 있으므로 개인적, 비상업적 목적으로만 사용 및 재배포할 수 있습니다.

## 서드파티 라이브러리

| 라이브러리 | 고정 버전 | 라이선스 | 용도 |
|:-----------|:----------|:---------|:-----|
| [BTstack](https://github.com/bluekitchen/btstack) | v1.8.1-6-g5bc5cbdbe | BTstack License (비상업적 조항이 있는 BSD-3-Clause 형식) / 상용 | 사용자 모드 Bluetooth 스택 |
| Bluedroid SBC codec (BTstack에 포함) | BTstack과 동일 | Apache-2.0 | SBC 인코더 |
| rijndael (BTstack에 포함) | BTstack과 동일 | Public domain | AES (BTstack에서 사용) |
| [libldac (AOSP)](https://android.googlesource.com/platform/external/libldac) | android-15.0.0_r36-4-geeee1a3 | Apache-2.0 | LDAC 인코더 |
| [libopenaptx](https://github.com/pali/libopenaptx) | 0.2.0 | LGPL-2.1+ | aptX / aptX HD / aptX LL 인코더 |
| [fdk-aac](https://github.com/mstorsjo/fdk-aac) | v2.0.3-158-gd8e6b1a | FDK AAC License | AAC-LC 인코더 |
| [LHDC-V5-Encoder](https://github.com/WillyBilly06/LHDC-V5-Encoder) (AOSP 인코더의 C 포팅) | commit 3f9d198 | Apache-2.0 (FFT: BSD-3-Clause, KISS FFT) | LHDC V5 인코더 (실험적) |
| [wxWidgets](https://www.wxwidgets.org/) | 3.2.6 | wxWindows Library Licence 3.1 | GUI 프레임워크 |
| zlib (wxWidgets 내장) | 1.2.13.1 | zlib license | 압축 (PNG 지원) |
| libpng (wxWidgets 내장) | 1.6.37 | PNG Reference Library License v2 | PNG 이미지 |
| nanosvg (wxWidgets에 포함) | wxWidgets 3.2.6과 동일 | zlib license | SVG 이미지 |
| [nlohmann/json](https://github.com/nlohmann/json) | 3.12.0 | MIT | JSON 파서 |

전체 라이선스 텍스트와 준수 사항은 [THIRD_PARTY_LICENSES.md](https://github.com/SeiyaFunaokaJP/A2DP-Windows-Bridge/blob/main/THIRD_PARTY_LICENSES.md)를 참조하십시오. 릴리스 zip에는 `licenses/` 폴더에 라이선스 텍스트가, `SOURCE.txt`에 정확한 소스 커밋 목록이 들어 있습니다. Realtek 펌웨어와 Zadig는 A2DPWB와 함께 배포되지 않습니다(링크만 제공).

## 중요 참고 사항

### BTstack 라이선스 (비상업적)

BTstack은 BSD-3-Clause가 **아니며**, OSI 승인 오픈 소스 라이선스도 아닙니다. 그 라이선스(SPDX `LicenseRef-BTstack`)는 BSD-3-Clause의 세 가지 조건에 네 번째 조항이 추가된 것입니다.

> 4. Any redistribution, use, or modification is done solely for personal benefit and not for any commercial purpose or for monetary gain.

`A2DPWB.exe`에는 BTstack이 포함되어 있으므로 이 제한은 배포되는 바이너리에 적용됩니다. 상업적으로 사용하려면 [BlueKitchen GmbH](https://bluekitchen-gmbh.com/)(contact@bluekitchen-gmbh.com)의 상용 라이선스가 필요합니다.

### LDAC 인증

libldac NOTICE에는 "Taking the certification process is required to use LDAC in your products."라고 명시되어 있습니다. [sony.net/Products/LDAC/aosp](https://www.sony.net/Products/LDAC/aosp/)를 참조하십시오. Sony의 인증 절차가 A2DPWB 같은 무료 비상업적 애플리케이션에 적용되는지는 확인되지 않았으며, A2DPWB는 인증을 받지 않았습니다.

### LHDC 상표 및 인증

LHDC는 Hi-Res Wireless Audio(HWA) 얼라이언스가 보급하는 Savitech Corp.의 코덱입니다. A2DPWB가 사용하는 인코더는 Google이 AOSP(Android 17)에 공개한 코드에서 파생된 Apache-2.0 코드이지만, Apache-2.0은 상표권을 부여하지 않으며 LHDC를 내세우는 제품은 보통 Savitech / HWA 인증을 거칩니다. A2DPWB는 인증을 받지 않았으며, LHDC V5 지원은 실험적이고 비상업적인 상호 운용성 기능입니다.

### libopenaptx 버전

libopenaptx는 마지막 LGPL-2.1+ 릴리스인 **0.2.0**으로 고정되어 있습니다. 0.2.1부터는 추가 제한이 붙은 GPL-3.0-or-later 라이선스가 적용되어, A2DPWB에 링크되는 다른 라이브러리와 호환되지 않습니다. v1.0.4까지의 릴리스는 실수로 0.2.1로 빌드되었으므로 v1.0.5 이상을 사용하십시오.

libopenaptx는 정적으로 링크됩니다. 릴리스 zip에는 LGPL-2.1 텍스트가 포함되어 있고, 정보 대화 상자에 libopenaptx 저작권 표시와 라이선스 텍스트 안내가 표시되며, (수정한 libopenaptx로 다시 빌드/다시 링크하기 위한) 대응하는 전체 소스는 저장소의 해당 릴리스 태그 커밋과 서브모듈에서 얻을 수 있습니다.

### aptX 특허 고지

aptX(기본), aptX HD, aptX Low Latency, aptX Adaptive는 Qualcomm의 상표입니다. libopenaptx는 README에 따르면 "derived from ffmpeg 4.0 project"인 오픈 소스 aptX 구현이며, 여기서는 aptX, aptX HD, aptX Low Latency에 사용됩니다. aptX 인코딩은 일부 관할권에서 특허의 대상일 수 있습니다. A2DPWB에는 aptX Adaptive 인코더가 포함되어 있지 않습니다.

### fdk-aac 라이선스 및 AAC 특허 고지

fdk-aac 라이선스는 바이너리를 재배포할 때 전체 라이선스 텍스트를 함께 제공하고, FDK AAC Codec의 전체 소스 코드(및 모든 수정 사항)를 바이너리 수령자에게 무료로 제공할 것을 요구합니다. 또한 **특허 라이선스를 부여하지 않습니다**. AAC는 특허의 대상일 수 있으며(NOTICE에서 Via Licensing을 언급), 필요한 AAC 특허 라이선스를 얻는 것은 사용자의 책임입니다.

### 개발자 도구 a2dpwb_decode

`a2dpwb_decode`([사용법](usage#verify-stream) 참조)는 소스에서만 빌드되며 릴리스 zip에는 포함되지 않습니다. 위에 나온 고정 버전의 Bluedroid SBC 디코더(Apache-2.0), fdk-aac(FDK AAC License), libopenaptx 0.2.0(LGPL-2.1+)을 링크하며, BTstack과 libldac은 링크하지 **않습니다**. THIRD_PARTY_LICENSES.md §12를 참조하십시오.

### 테스트 환경 tools/emu 및 tools/linux_sink

`tools/emu`([빌드](building) 참조)와 측정용 수신기 `tools/linux_sink`([사용법](usage#receiver) 참조)는 [Bumble](https://github.com/google/bumble) 0.0.234(Apache-2.0)와 그 종속성을 사용하며, 이들은 고정 버전으로 git에서 무시되는 로컬 환경에 다운로드됩니다. 이 중 어떤 것도 A2DPWB에 링크되거나 함께 배포되지 않습니다. 패키지 목록과 라이선스는 THIRD_PARTY_LICENSES.md §13을 참조하십시오.

### Bluetooth 인증

상업용 Bluetooth 제품은 Bluetooth SIG가 관리하는 [Bluetooth Qualification Process](https://www.bluetooth.com/)를 거쳐야 합니다.
