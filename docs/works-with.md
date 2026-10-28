---
title: Works With
layout: default
nav_order: 3.6
english_only: true
---

# Works With
{: .no_toc }

Third-party apps and tools that work together with A2DPWB, as told to us by their authors or users in the issues.

{: .warning }
> These are **not part of A2DPWB** and are not made, tested or supported by the A2DPWB project. They are listed only because someone reported that they work with A2DPWB. We take no responsibility if one does not work, stops working after an update of either program, or causes any problem. Please check each project's own license and documentation, and report problems with a tool to that tool's project first.

## Table of contents
{: .no_toc .text-delta }

1. TOC
{:toc}

## Apps and tools

| Tool | What it does with A2DPWB | Needs | License | Reported by |
|:--|:--|:--|:--|:--|
| [EncoDeck](https://github.com/TIANLI0/EncoDeck) | Earbud and headphone control (battery, noise control, EQ, gestures) for OPPO, Sony, HUAWEI, Xiaomi and others. Connects to the core's pipe as a second client: shows the stream state and codec, starts and stops profiles, sets the AVRCP volume and helps with the WinUSB assignment. Earbud settings over A2DPWB's own Bluetooth link use the RFCOMM passthrough | EncoDeck 1.11.0 or later. RFCOMM passthrough: A2DPWB 2.0.2-beta.10 or later (tried only in the emulator so far) | GPL-3.0 | Its author, [#194](https://github.com/SeiyaFunaokaJP/A2DP-Windows-Bridge/issues/194) |

## Adding a tool

Made or found a tool that works with A2DPWB? Tell us in an [issue](https://github.com/SeiyaFunaokaJP/A2DP-Windows-Bridge/issues) with its name, a link, its license, what it does with A2DPWB and the versions you tried. Being listed here is not a review or an endorsement.

## For tool authors

Tools talk to the core over the same per-user named pipe as the A2DPWB window; the protocol is described in [Core and IPC](architecture#core-and-ipc). Things to keep in mind:

- **Feature check**: the core's `welcome` lists optional features in `features` (for example `"rfcomm"`). Older cores leave the field out, so check it before using a feature.
- **Unknown requests**: a request the core cannot read gets an `ipc.bad_request` error. Cores before 2.0.2-beta.10 close the connection instead.
- **Rename in 3.0.0** ([#193](https://github.com/SeiyaFunaokaJP/A2DP-Windows-Bridge/issues/193)): the pipe becomes `\\.\pipe\bcfw-<SID>`. The core keeps listening on `\\.\pipe\a2dpwb-<SID>` as well, with the same protocol, for at least 6 months after 3.0.0; its removal is announced in the release notes first. Try `bcfw-<SID>` first and fall back to `a2dpwb-<SID>`. Use the pipe, not the mutex, to check whether the core is running: the mutex name changes without a transition.
- The protocol and the core may still change during the 2.0.2 betas.
