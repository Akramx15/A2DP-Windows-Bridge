# The xm6 spatial engine on Windows

The fork carries a private Sony 360RA chain (seven-plane upmix, HPP/HPS profiles,
XM6 bass correction) driven by the app as a child process, plus an Upmixer stage
that advertises live effect controls. Two scripts keep that portable.

| Script | When to run it |
|---|---|
| `restore.ps1` | On a fresh machine, or after deleting the local working copies. Recreates the stages, the pinned Upmixer runtime, and points the profiles at them. |
| `apply-upstream.ps1` | When upstream publishes a new tag. Moves the feature onto it, builds, installs. |

## A new upstream release

```
cd A2DP-Windows-Bridge
powershell -ExecutionPolicy Bypass -File tools/xm6/apply-upstream.ps1
```

It fetches upstream, creates `feat/xm6-360-spatial-engine-<tag>` from the new
tag, merges the previous feature branch, checks the language files, builds the
UI and the three executables, and copies them into the installed app (keeping
`*.bak` copies). On a conflict it stops, lists the files and leaves the merge in
place — nothing is discarded. Nothing is pushed.

Useful switches: `-Tag v2.0.2-beta.6`, `-Test` (run clippy, cargo test, tsc,
vitest first), `-SkipBuild` (git work only), `-SkipInstall`.

The build runs on `stable-x86_64-pc-windows-gnu` by default: upstream's
`aptx-adaptive` needs rustc 1.99, the installed MSVC toolchain is older, and
this machine has no MSVC linker. Override with `-Toolchain ''` when that
changes.

## After deleting everything

The app's stages, the Upmixer runtime and the engine build outputs are *not* in
git history of this repository. They are reproduced from the engine repository:

```
powershell -ExecutionPolicy Bypass -File tools/xm6/restore.ps1
```

Needs: `git`, `gh` (the engine repository is private), Python 3.12+, network for
the three pinned packages. It clones both repositories into the parent folder,
installs the prebuilt stages to `%APPDATA%\A2DPWB\stages`, builds
`%APPDATA%\A2DPWB\runtime\upmixer-venv`, rewrites `profiles.json`, and then
verifies all five stages byte-for-byte against their recorded PCM.

## What is where

| Thing | Location | In git? |
|---|---|---|
| App fork (this repository) | `Akramx15/A2DP-Windows-Bridge` | yes |
| Engine sources, `win_compat`, prebuilt stages, PCM verifier | `Akramx15/xm6-spatial-linux` → `engine/win-port/` | yes (private) |
| User Sony assets, `private/` work, dev venv | `xm6-spatial-linux/private/` | no (git-ignored, user data) |
| Stage binaries in use | `%APPDATA%\A2DPWB\stages` | copy of the prebuilt set |
| Upmixer interpreter | `%APPDATA%\A2DPWB\runtime\upmixer-venv` | rebuilt from pins |
| Rust build cache | `target/` | no, ~17 GB, rebuilt by cargo |
| UI dependencies | `app/src/ui/node_modules` | no, `pnpm install` |

`target/` and `node_modules/` are pure caches: deleting them costs one build,
nothing else.

## Byte-exactness

`engine/win-port/qa/verify_win_pcm.py` in the engine repository feeds a fixed
corpus to every stage and compares the produced PCM with recorded hashes. It
strips mingw from `PATH`, so a pass also proves the stages do not depend on a C
toolchain. Run it after any update; `apply-upstream.ps1` does that
automatically when the engine clone sits next to this repository.
