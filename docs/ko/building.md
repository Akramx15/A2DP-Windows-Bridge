---
title: 빌드
layout: default
parent: 홈
nav_order: 3
---

# 소스에서 빌드
{: .no_toc }

## 목차
{: .no_toc .text-delta }

1. TOC
{:toc}

---

## 요구 사항

- "C++를 사용한 데스크톱 개발" 워크로드가 설치된 **Visual Studio 2022 이상**
- **CMake** 3.16+
- **Git** (서브모듈 및 wxWidgets FetchContent용)

## 빠른 빌드

```bash
git clone --recursive https://github.com/SeiyaFunaokaJP/A2DP-Windows-Bridge.git
cd A2DP-Windows-Bridge
cmake -B build -A x64
cmake --build build --config Release
```

실행 파일은 `build/app/Release/A2DPWB.exe`에 생성됩니다.

{: .note }
처음 빌드할 때는 CMake FetchContent가 wxWidgets(v3.2.6)를 다운로드하고 컴파일하므로 몇 분 정도 걸립니다.

## `--recursive` 없이 이미 클론한 경우

```bash
git submodule update --init --recursive
```

## 프로젝트 구조

```
A2DP-Windows-Bridge/
├── app/                    애플리케이션 소스
│   ├── src/                C++ 소스 파일
│   ├── lang/               다국어 리소스 (en.json이 원본, 나머지는 tools/check_lang.py로 검사)
│   ├── resources/          아이콘, 매니페스트, 리소스 스크립트
│   └── CMakeLists.txt      앱 빌드 설정
├── cmake/                  릴리스 패키징 (make_dist.cmake)
├── installer/              설치 프로그램용 Inno Setup 스크립트 (A2DPWB.iss)
├── compat/                 AOSP 코드용 MSVC 호환 헤더
├── docs/                   문서 (이 사이트)
├── extern/                 서드파티 라이브러리 (git 서브모듈)
│   ├── btstack/            BTstack Bluetooth 스택
│   ├── libldac/            AOSP LDAC 인코더
│   ├── libopenaptx/        aptX / aptX HD / aptX LL 인코더
│   ├── fdk-aac/            Fraunhofer AAC 인코더
│   └── json/               nlohmann/json (헤더 전용)
├── tools/
│   ├── a2dp_decode/        a2dpwb_decode: HCI 캡처(.pklg)의 미디어 스트림 검사 / 디코딩
│   ├── emu/                가상 Bluetooth 싱크 대상 엔드투엔드 테스트 (Python)
│   └── linux_sink/         a2dpwb_sink: Linux용 측정 A2DP 수신기 (Python, Bumble)
├── CMakeLists.txt          루트 빌드 설정
└── build.bat.example       빌드 도우미 스크립트 (build.bat으로 복사하고 CMake 경로 설정)
```

## 빌드 옵션

| CMake 옵션 | 기본값 | 설명 |
|:-----------|:-------|:-----|
| `LDAC_SOFT_FLOAT` | `OFF` | libldac에 소프트웨어 부동 소수점 사용 |
| `A2DPWB_BUILD_TOOLS` | `ON` | 개발자 도구 빌드 (`a2dpwb_decode`, [사용법](usage#verify-stream) 참조). 릴리스 패키지에는 포함되지 않음 |

```bash
cmake -B build -A x64 -DA2DPWB_BUILD_TOOLS=OFF
```

## 릴리스 패키지

```bash
cmake --build build --target A2DPWB_dist --config Release
```

위 명령은 `build/dist/A2DPWB-<version>-win64-portable.zip`을 생성하며, [Inno Setup 6](https://jrsoftware.org/isinfo.php)이 설치되어 있으면(`winget install JRSoftware.InnoSetup`, 설치 후 CMake configure를 다시 실행) `installer/A2DPWB.iss`로 `build/dist/A2DPWB-<version>-win64-setup.exe`도 생성합니다. 두 패키지에는 같은 파일이 들어 있습니다.

## 하드웨어 없이 테스트 (tools/emu) {#emu}

`tools/emu`는 어댑터나 헤드폰 없이 같은 PC의 가상 Bluetooth 싱크로 A2DPWB의 모든 코덱을 스트리밍합니다. Windows에서만 실행되며, 빌드 요구 사항 외에 **Python 3.11 이상**이 필요합니다.

```bash
python tools/emu/setup_env.py     # once: creates tools/emu/.venv with pinned packages
python tools/emu/run_test.py      # after a Release build; about 35 s for all codecs
```

작동 방식:

- `emu_sink.py`는 프로세스 내부 링크에서 [Bumble](https://github.com/google/bumble) 가상 컨트롤러 두 개를 실행합니다. 하나는 A2DPWB를 위해 TCP(`127.0.0.1`)를 통한 H4로 제공됩니다. 다른 하나는 SBC, AAC, aptX, aptX HD, aptX LL, LDAC을 제공하는 Bumble A2DP 싱크가 사용하며, 그 HCI 트래픽을 기록합니다.
- `run_test.py`는 각 코덱에 대해 개발용 옵션과 함께 `A2DPWB.exe --cli`를 실행합니다: `--hci-tcp`(WinUSB 대신 가상 컨트롤러), `--test-tone`(시스템 오디오 대신 왼쪽 1 kHz / 오른쪽 1.5 kHz), `--duration`, `--hci-capture`. 환경 변수 `A2DPWB_CONFIG_DIR`로 A2DPWB가 별도의 설정 폴더를 사용하게 하므로, 가상 싱크와의 페어링이 실제 링크 키에 영향을 주지 않습니다.
- 그런 다음 두 HCI 캡처를 `a2dpwb_decode`로 검사합니다(싱크 측은 `--received` 사용).

다음 조건을 모두 충족하면 코덱이 통과합니다.

- 요청한 코덱이 협상되었음
- `a2dpwb_decode`가 전송 측과 수신 측 모두에서 문제를 찾지 못함
- A2DPWB가 보낸 모든 미디어 패킷과 프레임을 싱크가 수신했으며, 요청한 오디오 길이의 90% 이상이 전송됨
- LDAC을 제외하고(오픈 소스 디코더 없음): 양쪽에서 디코딩한 오디오가 동일하며 테스트 톤임

| 옵션 | 설명 |
|:-----|:-----|
| `--codecs sbc aac ...` | 지정한 코덱만 테스트 (`sbc aac aptx aptxhd aptxll ldac`, 기본값: 전체) |
| `--duration <s>` | 코덱당 스트리밍 시간(초) (기본값 5) |
| `--max-packet <bytes>` | A2DPWB에 전달 (최대 미디어 패킷 크기, [사용법](usage) 참조) |
| `--build-dir`, `--config` | `A2DPWB.exe` / `a2dpwb_decode.exe`를 찾을 위치 (기본값 `build`, `Release`) |
| `--out <dir>` | 출력 폴더 (기본값 `tools/emu/out`) |

각 코덱의 `tools/emu/out/<codec>/`에는 캡처(`a2dpwb.pklg`, `sink.pklg`), 로그, `a2dpwb_decode` 보고서, 디코딩된 WAV 파일이 저장됩니다. 모든 코덱이 통과하면 종료 코드는 0입니다.

{: .note }
가상 링크에는 무선 구간이 없습니다. 인코딩, 패킷화, AVDTP / L2CAP 시그널링, 독립적인 Bluetooth 스택(Bumble)과의 상호 운용성은 확인하지만, 무선 환경, 실제 하드웨어에서의 타이밍, 실제 헤드폰의 재생 방식은 확인하지 않습니다. Bumble 패키지는 테스트 도구일 뿐 A2DPWB의 일부가 아닙니다(THIRD_PARTY_LICENSES.md §13 참조).

`emu_vhci.py`는 Linux Bluetooth 스택을 대상으로 같은 일을 합니다. Linux에서 `/dev/vhci`를 통해 BlueZ에 가상 어댑터를 연결하고, 측정용 수신기 `tools/linux_sink`가 이를 실제 어댑터처럼 사용합니다. 하드웨어 없이 테스트할 수 있으며, 선택적으로 미디어 패킷을 손실시키거나 지연시킬 수도 있습니다(`--drop`, `--stall`). `tools/linux_sink/README.md`를 참조하십시오.

## 종속성

모든 종속성은 git 서브모듈로 포함되거나 빌드 시 가져옵니다. 별도로 설치할 필요가 없습니다.

| 라이브러리 | 방식 | 고정 버전 | 라이선스 |
|:-----------|:-----|:----------|:---------|
| BTstack | git submodule | v1.8.1-6-g5bc5cbdbe | BTstack License (비상업적 조항이 있는 BSD-3-Clause 형식) |
| Bluedroid SBC codec | BTstack에 포함 (`3rd-party/bluedroid`) | BTstack과 동일 | Apache-2.0 |
| rijndael | BTstack에 포함 (`3rd-party/rijndael`) | BTstack과 동일 | Public domain |
| libldac (AOSP) | git submodule | android-15.0.0_r36-4-geeee1a3 | Apache-2.0 |
| libopenaptx | git submodule | 0.2.0 (업데이트 금지) | LGPL-2.1+ |
| fdk-aac | git submodule | v2.0.3-158-gd8e6b1a | FDK AAC License |
| nlohmann/json | git submodule (header-only) | 3.12.0 | MIT |
| wxWidgets | CMake FetchContent | v3.2.6 | wxWindows Library Licence 3.1 |
| zlib / libpng / nanosvg | wxWidgets 내장 사본 | 1.2.13.1 / 1.6.37 / wxWidgets와 동일 | zlib / PNG Reference Library License v2 / zlib |

{: .warning }
BTstack이 링크되어 있으므로, A2DPWB 소스 코드 자체는 Apache-2.0 또는 MIT 이중 라이선스이지만 직접 빌드한 `A2DPWB.exe`도 개인적, 비상업적 목적으로만 사용 및 재배포할 수 있습니다.

전체 라이선스 정보는 [THIRD_PARTY_LICENSES.md](https://github.com/SeiyaFunaokaJP/A2DP-Windows-Bridge/blob/main/THIRD_PARTY_LICENSES.md)를 참조하십시오.
