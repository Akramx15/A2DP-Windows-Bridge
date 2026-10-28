# The XM6 spatial engine in this fork

This fork adds one feature to A2DP Windows Bridge: the audio that the app captures can be
processed by a **spatial engine** before it is encoded, so the headphones receive a
binaural 360 mix instead of plain stereo.

It is the Windows port of the chain the author already runs on Linux: Sony's 360RA
spatial chain (Music upmix → virtualizer → UPL → limiter) plus the `WH-1000XM6` bass
correction, and an alternative "Upmixer" stage that does a stereo → spatial upmix.

## What the user sees

In **Codecs**, under the active device, a *Spatial effect* block offers three choices:

| Choice | What runs |
|---|---|
| **Sony 360** | the Sony chain with the personal HRTF profile (`personal-hps`, `personal-hpp-ba`, …) |
| **Upmixer** | the Upmixer stage (needs its runtime, see below) |
| **Turn effect off** | the captured audio goes to the encoder untouched |

The stage list is read from the stage folder (`%APPDATA%\A2DPWB\stages` by default). Each
stage is a folder with a program and a `manifest.json` that declares its `label`, its
`family` (`sony` / `upmixer`), its command line and, optionally, the controls it exposes.
Adding a stage is dropping a folder — no code change.

## How it is wired

```
capture (48 kHz or 96 kHz)
   → [resample to 48 kHz]        only when the capture is 96 kHz
   → spatial engine (child process, 1024-frame blocks, 48 kHz)
   → [resample back]             same
   → encoder → LDAC / aptX / AAC …
```

- The engine is a **child process**, not a linked library: it cannot crash the app, and it
  can be replaced without rebuilding.
- If the engine fails to start, the audio passes through and the failure is logged. A
  broken stage never kills the stream.
- The engine is fixed at **48 kHz / 1024 frames**. The 96 kHz transport resamples around
  it, which is why the capture can stay at 96 kHz for LDAC while the engine stays at 48.
- The 96 kHz path exists in `resample.rs`; check the log for
  `capturing at 96000 Hz, resampling around the 48000 Hz engine` before expecting it.

## Where the code is

| Path | Responsibility |
|---|---|
| `app/src/crates/core/src/spatial.rs` | stage discovery, manifests, controls, the launch configuration |
| `app/src/crates/pipeline/src/spatial.rs` | the child process: PCM in, PCM out, restart on reset |
| `app/src/crates/pipeline/src/resample.rs` | the 2:1 down/up sampler around the engine |
| `app/src/crates/pipeline/src/lib.rs` | the one hook in `encode_thread` |
| `app/src/ui/src/components/SpatialStage.tsx` | the picker in Codecs |
| `tools/xm6/` | stage restore, upstream sync, state file |

## Installing the stages on a machine

The stage binaries and the Upmixer runtime are **not** in this repository (they belong to
the author's private engine repository, which also holds the Sony profile material).

```powershell
powershell -ExecutionPolicy Bypass -File tools/xm6/restore.ps1
```

It clones the engine repository, installs the prebuilt stages into
`%APPDATA%\A2DPWB\stages`, rebuilds the Upmixer virtual environment, rewrites
`profiles.json`, then verifies every stage byte-for-byte against its recorded PCM.

## Limits worth knowing

- **48 kHz engine**: a 96 kHz capture is resampled twice; the transport is real but the
  engine's internal rate does not change.
- **Latency**: the engine adds a block (1024 frames ≈ 21 ms) plus the resampler.
- **Stage layout is user data**: `%APPDATA%\A2DPWB\stages` is not versioned. Losing it
  costs a `restore.ps1` run, not a rebuild.
- **The Upmixer runtime** is a Python virtual environment with pinned packages; it is
  rebuilt by `restore.ps1`, never committed.

## When upstream releases

See `docs/UPSTREAM-SYNC.md`. One command, one branch, one tag.
