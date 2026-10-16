---
title: LE Audio (developer note)
layout: default
nav_exclude: true
---

# LE Audio (experimental) - developer note

A2DPWB can stream to LE Audio devices as a **BAP unicast client** (audio
source): LC3 over Connected Isochronous Streams (CIS), on the same BTstack
session and WinUSB adapter as A2DP. It is **experimental and off by
default** (Settings > "LE Audio (experimental)", `le_audio_experimental` in
`settings.json`; CLI `--le-audio`). With the setting off none of the LE code
runs. It has only been verified against the Bumble emulator (`tools/emu`),
not yet with real headphones or a real 5.2 controller.

This note is for the work on the LE Audio profiles beyond BAP (CSIP, VCP,
MCP, CCP, TMAP / GMAP). Each has a pre-registered stub; normally only that
stub's files change.

## Files

| Path | What |
|------|------|
| `app/src/btstack_config.h` | `ENABLE_BLE`, LE central (+ `ENABLE_LE_PERIPHERAL`, needed by BTstack's extended advertising code; A2DPWB never advertises), LE Secure Connections with micro-ecc, extended scanning, `ENABLE_LE_ISOCHRONOUS_STREAMS`, `MAX_NR_CIS 4`, `HCI_ISO_PAYLOAD_SIZE 400` |
| `app/src/hci_transport_h2_winusb.c` | HCI ISO data on the ACL bulk endpoints (OUT: sent as is; IN: passed up as ACL, `hci.c` reclassifies by ISO handle) |
| `app/src/le_audio/le_audio_core.{h,cpp}` | `le_audio::Core`: stack init, LE scan, connect, SM, GATT cache, worker API, stream start / stop, module dispatch |
| `app/src/le_audio/le_audio_gatt_client.{h,cpp}` | `le_audio::gatt_client`: the per-connection GATT client queue |
| `app/src/le_audio/le_audio_gatt_server.{h,cpp}` | `le_audio::GattServer`: the PC's GATT server (att_db_util) |
| `app/src/le_audio/le_audio_bap.{h,cpp}` | PACS / ASCS client (`bap::read_device`) and `Unicast`, the BAP unicast state machine |
| `app/src/le_audio/le_audio_presets.{h,cpp}` | `StreamPreset` table (BAP Table 5.2), `StreamRequest`, `plan_stream()` |
| `app/src/le_audio/le_audio_stream.{h,cpp}` | `Stream`: PCM in, LC3 encode, jitter buffer, ISO SDUs out |
| `app/src/le_audio/le_audio_device.h`, `le_audio_defs.h`, `le_audio_util.cpp` | `LeAudioDevice`, GATT cache types, PAC / ASE parsing, UUIDs, LTV helpers |
| `app/src/le_audio/le_audio_module.h`, `le_audio_modules.cpp` | the hook interface and the module registry |
| `app/src/le_audio/le_audio_{csip,vcp,mcp,ccp,tmap_gmap}.{h,cpp}` | **the profile stubs** (one per work package) |
| `app/src/le_audio/le_audio_cli.{h,cpp}` | `A2DPWB --cli --le-audio` / `--le-scan` |
| `app/src/a2dp_service_le_audio.cpp` | the GUI's streaming thread for a profile marked "LE Audio device" |
| `tools/a2dp_decode/le_audio_decode.cpp` | `a2dpwb_decode --le-audio` |
| `tools/emu/emu_le_audio_sink.py`, `tools/emu/le_audio/` | Bumble LE Audio sink(s); `le_audio/<profile>.py` = the sink side of each profile (stubs) |
| `tools/emu/le_audio_tests/` | `run_test.py --le-audio` cases and checks; `<profile>.py` per work package (stubs) |

## Threads - the one rule that matters

BTstack is single-threaded. Everything that touches BTstack, a
`LeAudioDevice`, `gatt_client` or `GattServer` runs on the **BTstack
thread**. All module hooks are called on it except the three marked
"worker" below.

- From a worker thread (the GUI's streaming thread, the CLI) reach the BTstack
  thread with `Core::instance().run(fn, timeout_ms)` (run and wait),
  `Core::call(fn(done), timeout_ms, cancelled)` (fn calls `done(ok)` later,
  e.g. from a GATT callback; the caller waits for it) or `post(fn)` (no wait,
  any thread).
- **Never call `run()` / `call()` / `scan()` / `connect()` / `start_stream()`
  from the BTstack thread** (a hook, a GATT callback): they wait for the
  BTstack thread and would deadlock until their timeout.
- Capture data for `run()` lambdas in `std::shared_ptr`s, not by reference to
  locals: after a timeout the lambda may still run later.
- `remote_volume()` / `request_volume()` may be called from any thread: keep
  their state in atomics and `post()` the GATT work.

## Lifecycle

1. `Core::set_enabled(true)` before `BtStackTransport::init()` (A2dpService
   does it from the setting, the CLI for `--le-audio`).
2. `BtStackTransport::setup_hci_and_power_on()` calls `Core::stack_init()`
   (BTstack thread, after `l2cap_init()`): LE bonds TLV
   (`<config dir>\le_audio_bonds.tlv`) for `le_device_db_tlv`, `sm_init()`
   (Secure Connections, bonding, NoInputNoOutput), `gatt_client_init()`, HCI /
   SM / ISO handlers, then **`Module::on_stack_init()`**, then the GATT server
   (GAP, GATT, **`Module::add_gatt_services()`**), `att_server_init()`.
3. HCI working: `Core::ready()` is true; ISO buffer sizes (LE Read Buffer
   Size v2) are in `iso_packet_length()` / `iso_packet_count()`.
4. `connect(addr)`: `gap_connect` -> **`on_connected`** -> `sm_request_pairing`
   (re-encrypts with a bond; a bond the device lost is deleted and pairing
   repeated once) -> encrypted and no pairing in progress ->
   **`on_encrypted`** -> GATT discovery into `dev.services` -> `bap::read_device`
   (PACS, ASCS, notifications on) -> **`on_gatt_ready`** -> `connect()` returns
   the device id.
5. `start_stream(ids, request)`: **`prepare_group`** (worker) ->
   **`adjust_stream_request`** (worker) -> `plan_stream()` -> `Stream::configure`
   -> `Unicast::start`: Config Codec -> LE Set CIG Parameters -> Config QoS ->
   Enable -> LE Create CIS (all CIS at once, BTstack sets up the ISO data
   paths) -> sink ASEs go Streaming by themselves (the server's Receiver
   Start Ready) -> SDUs flow -> **`on_stream_state(status, true)`**.
6. `stop_stream()`: Disable -> Release -> `gap_cig_remove` (disconnects the
   CIS) -> **`on_stream_state(status, false)`**. Devices stay connected.
7. A link or CIS lost while streaming: `stream_lost()` turns true once; the
   worker stops and sets the stream up again (CLI: 3 tries, GUI: 10).
   **`on_disconnected(dev, reason)`** for every LE link that goes down.
8. `Core::stack_deinit()` after the run loop stopped:
   **`on_stack_deinit()`** - no BTstack calls any more. The stack can be
   started again later (sleep, adapter restart): device ids start from 0
   again, so drop per-device state here.

## Module hook API (`le_audio_module.h`)

Derive from `le_audio::Module` in your stub, override what you need; the
factory (`csip_module()` etc.) is already registered in
`le_audio_modules.cpp` (order: CSIP, VCP, MCP, CCP, TMAP/GMAP).

| Hook | Thread | When / what for |
|------|--------|-----------------|
| `const char *name() const` | - | log name |
| `void on_stack_init(Core &)` | BTstack | session start, before HCI power on; e.g. `add_stream_preset()` |
| `void add_gatt_services(GattServer &)` | BTstack | build the PC-side services of the profile (only here) |
| `void on_stack_deinit()` | BTstack (loop stopped) | drop all state |
| `void on_scan_result(ScanResult &, const uint8_t *ad, uint16_t len)` | BTstack | every advertising report while `scan()` / `find()` run; may set `result.le_audio`, `result.set_id`, `result.note` |
| `void on_connected(LeAudioDevice &)` | BTstack | LE link up, not yet encrypted (no GATT yet) |
| `void on_encrypted(LeAudioDevice &)` | BTstack | link encrypted (paired / bonded) |
| `void on_gatt_ready(LeAudioDevice &)` | BTstack | `dev.services` filled, PACS / ASCS read: queue your reads / subscriptions here |
| `void on_disconnected(LeAudioDevice &, uint8_t reason)` | BTstack | link down; `dev` still has its last data |
| `void on_stream_state(const StreamStatus &, bool streaming)` | BTstack | audio flows (true) / stopped or lost (false) |
| `void prepare_group(Core &, std::vector<int> &ids, const std::function<bool()> &cancelled)` | **worker** | before planning: CSIP appends the ids of the other set members (may scan / connect) |
| `void adjust_stream_request(Core &, const std::vector<int> &ids, StreamRequest &)` | **worker** | GMAP: low-latency presets first, `contexts = CONTEXT_GAME` |
| `int remote_volume()` | any | headset volume 0-127 (AVRCP scale), -1 unknown; first module answering >= 0 wins |
| `bool request_volume(uint8_t)` | any | set the headset volume; true if handled |

`LeAudioDevice` (`le_audio_device.h`): `id`, `addr` (`LeAddr`: BTstack-order
bytes + type), `name`, `acl` (con handle, `CON_HANDLE_INVALID` when down),
`encrypted`, `gatt_ready`, `ready`, `services` (with `find_service(uuid16)`,
`find_char(svc, chr)`, `find_chars(svc, chr)`; secondary / included services
are in the list with `primary == false`, `included_by`), PACS results
(`sink_pacs`, `sink_locations`, `available_sink_contexts`, ...), ASCS
(`ases`, `ase_cp_handle`). Keep your own state in a
`std::map<int /* dev.id */, ...>` and erase it in `on_disconnected()` and
`on_stack_deinit()`.

## GATT client: always through the queue

BTstack's `gatt_client` allows one query per connection; a second one fails
with `GATT_CLIENT_IN_WRONG_STATE`. **Do not call `gatt_client_*` directly** -
use `le_audio::gatt_client` (`le_audio_gatt_client.h`, BTstack thread):

```cpp
using namespace le_audio;
void VcpModule::on_gatt_ready(LeAudioDevice &dev) {
    const GattChar *state = dev.find_char(UUID_SERVICE_VCS, 0x2B7D);   // Volume State
    if (!state) return;
    const int id = dev.id;
    const uint16_t con = dev.acl;
    gatt_client::listen(con, state->value_handle, [this, id](const uint8_t *v, uint16_t n) {
        if (n >= 3) volume_.store(v[0] * 127 / 255);                    // notifications
    });
    gatt_client::read(con, state->value_handle, [this](uint8_t status, const uint8_t *v, uint16_t n) {
        if (status == 0 && n >= 3) volume_.store(v[0] * 127 / 255);
    });
    gatt_client::subscribe(con, *state, /*indications=*/false, [](uint8_t status) { /* 0 = ok */ });
}
// later, from any thread:
Core::instance().post([=]() { gatt_client::write(con, cp_handle, {0x04, counter, level}, nullptr); });
```

- `read` (long reads included), `write` (with response; a long write when it
  does not fit the MTU), `write_without_response`, `subscribe` (writes the
  CCC), `discover_all`, `custom(start, on_event, done)` for anything else
  (start calls one `gatt_client_*` with the given handler; `done` gets the
  ATT status of `GATT_EVENT_QUERY_COMPLETE`).
- Operations run in order per connection; callbacks get `status` 0 or an ATT
  error / BTstack status; on disconnect everything pending completes with
  `ATT_ERROR_HCI_DISCONNECT_RECEIVED` (0x1F) and listeners are dropped.
- Check in callbacks that the device is still the same link
  (`Core::instance().device(id)` and `d->acl == con`), as `bap::read_device`
  does.

## GATT server: services of the PC

In `add_gatt_services()` only (the DB is fixed once `att_server_init()` ran):

```cpp
void McpModule::add_gatt_services(GattServer &server) {
    server.begin_service(UUID_SERVICE_GMCS);
    name_ = server.add_characteristic(0x2B93, ATT_PROPERTY_READ,          // Media Player Name
        [](uint16_t con) { return std::vector<uint8_t>{'A', '2', 'D', 'P', 'W', 'B'}; }, nullptr);
    state_ = server.add_characteristic(0x2BA3, ATT_PROPERTY_READ | ATT_PROPERTY_NOTIFY,
        [this](uint16_t con) { return std::vector<uint8_t>{media_state_}; }, nullptr);
    cp_ = server.add_characteristic(0x2BA4, ATT_PROPERTY_WRITE | ATT_PROPERTY_WRITE_WITHOUT_RESPONSE | ATT_PROPERTY_NOTIFY,
        nullptr, [this](uint16_t con, const uint8_t *v, uint16_t n) -> uint8_t { return on_cp(con, v, n); });
    server.end_service();   // returns the service's last handle (for include_service)
}
// BTstack thread, e.g. in on_stream_state():
Core::instance().gatt_server().notify_all(state_, {0x01 /* playing */});
```

- Read callbacks return the whole value (offsets / long reads are handled);
  write callbacks get the whole value (prepared writes are assembled) and
  return 0 or an ATT error code.
- A NOTIFY / INDICATE characteristic gets its CCC descriptor at value
  handle + 1, handled by the server: `notify(con, handle, value)` /
  `indicate(...)` send only to clients that enabled it and queue until ATT
  can send; `client_config(con, handle)` tells what a client enabled.
- 128-bit UUIDs: `begin_service128` / `add_characteristic128`; secondary
  services: `begin_service(uuid, false)` + `include_service(start, end, uuid)`.
- Security: the `read_security` / `write_security` arguments
  (`ATT_SECURITY_ENCRYPTED` etc.); LE Audio links are encrypted anyway.

## Stream configuration (`le_audio_presets.h`)

`stream_presets()` holds BAP Table 5.2 settings by name: `16_2_1` ... `48_6_1`
(low latency, incl. 7.5 ms) and `16_2_2` ... `48_6_2` (high reliability,
media). `StreamRequest::preset_order` (default `48_6_2, 48_4_2, 48_2_2,
32_2_2, 24_2_2, 16_2_2`) is tried in order; the first preset every device's
sink PAC records allow (sampling rate, frame duration, octets, channel count,
frames per SDU) wins. GMAP adds presets with `add_stream_preset()` and puts
them first from `adjust_stream_request()`. `StreamPreset` fields: `name,
sampling_hz, frame_us, octets, target_latency (TARGET_LATENCY_*), framing,
rtn, max_transport_latency_ms, presentation_delay_us`.

Channels: a device with Front Left + Right gets one 2-channel ASE when its
PAC allows 2 channels and 2 frames per SDU (`prefer_two_channel_ase`), else
two 1-channel ASEs; a device with one location gets one ASE with it; no
location = mono. Left / right audio go to FL / FR only when the group has
both; otherwise every channel carries the downmix. One CIG (id 1), CIS ids in
plan order, at most `MAX_NR_CIS` (4). The presentation delay is clamped into
every ASE's range; the max transport latency is capped by the servers'
preference.

## Audio path

`Core::stream().push_pcm(data, frames, channels, rate, bits)` has the
`AudioCallback` signature (float32 or int16, any channel count, any rate):
downmix to stereo, linear resampling to the LC3 rate if needed (WASAPI is
asked for the LC3 rate, so normally none), LC3 per channel, one SDU set per
10 ms into a jitter buffer (5 sets before audio starts / after an underrun;
16 at most, the oldest dropped beyond). The BTstack thread sends on
`HCI_EVENT_CIS_CAN_SEND_NOW`: complete SDUs, sequence numbers counting per
CIS, **no time stamp** (the controller places SDUs by sequence number), 2
SDUs queued per CIS (`hci_set_num_iso_packets_to_queue`), and a guard that
keeps the sent sets at most a few intervals ahead of real time. Underrun ->
encoded silence.

## Emulator (`tools/emu`)

`emu_le_audio_sink.py --port P --capture sink.pklg --layout L` runs the A2DPWB
controller (H4 over TCP) and one or two sinks on a Bumble `LocalLink`:

| Layout | Sinks |
|--------|-------|
| `stereo` | 1 sink `00:A2:D0:00:00:12`, FL+FR, 2 sink ASEs, 1 or 2 channels per ASE |
| `two-ase` | 1 sink, FL+FR, 2 sink ASEs of 1 channel |
| `earbuds` | 2 sinks (`...:12` FL, `...:13` FR), 1 ASE each |
| `mono` | 1 sink, no location, 1 ASE |

It prints `READY 127.0.0.1:P sinks=<addr>[,<addr>]`. Captures:
`sink.pklg`, `sink-2.pklg`, ...

Profile pieces of the sinks go in `tools/emu/le_audio/<profile>.py`
(`csip.py`, `vcp.py`, `mcp.py`, `ccp.py`, `tmap_gmap.py` exist as stubs):

```python
def add_arguments(parser):          # e.g. parser.add_argument('--vcp-volume', type=int)
def configure_sink(sink, args):     # before power on: sink.device.add_service(...),
                                    # sink.adv_structures.append((type, bytes)), sink.state['vcp'] = ...
async def start(sink, args):        # after power on / advertising
```

`sink` is a `SinkContext` (`le_audio/sink.py`): `index, address, name,
locations, sink_ases, channel_counts, device` (Bumble `Device`),
`adv_structures`, `state`. Log with `logging.getLogger('emu_le_audio')` at
INFO (it goes to `sink.log`, which checks can read).

Emulator facts to know:
- Legacy advertising, **31 bytes**: flags + ASCS + CAS service data take 19;
  the name gets what is left (shortened if needed). Bumble's controller sends
  the advertising data as scan response too, so there is no more room there.
  An RSI (8 bytes) still fits with a short name.
- `LeEmuController` carries ISO data between the controllers, one SDU per
  interval per CIS, with time stamps; it tolerates the host being up to 3
  intervals late (`LATE_INTERVALS`, Windows timer jitter) before it moves an
  SDU to a later interval. `LeLocalLink` fixes LE ACL routing for public
  addresses. Both in `le_audio/controller.py`.
- Bumble's ASCS goes to Streaming right after the CIS is up (sink ASE), and
  its pairing is Just Works with an in-memory key store (fresh per run).
- Sinks are Bumble `Device`s: `bumble.profiles.vcs`, `csip`, `mcp`, `tmap`,
  `gmap` etc. are available in the venv (Bumble 0.0.234). No new pip
  dependencies were needed so far; if you add one, pin it in
  `tools/emu/requirements.txt` and list it in THIRD_PARTY_LICENSES.md §13.

## Tests

Build (once per worktree; wx compiles once, ~5 min):

```
cmake -B build -G "Visual Studio 17 2022" -A x64 -DFETCHCONTENT_SOURCE_DIR_WXWIDGETS=<main checkout>/build/_deps/wxwidgets-src
cmake --build build --config Release -- -m
```

Run (PowerShell; the venv of the main checkout has Bumble; `run_test.py`
uses the Python it runs under when `tools\emu\.venv` is missing):

```
I:\A2DP-Windows-Bridge\tools\emu\.venv\Scripts\python.exe tools\emu\run_test.py --build-dir build --out $env:TEMP\le --le-audio
I:\A2DP-Windows-Bridge\tools\emu\.venv\Scripts\python.exe tools\emu\run_test.py --build-dir build --out $env:TEMP\a2dp
```

The first runs all LE Audio cases (`stereo`, `two-ase`, `earbuds`,
`mono-16k`, plus yours; or name them: `--le-audio stereo earbuds`), the second
the A2DP codecs (must keep passing). Per case `<out>/le-<case>/` has
`a2dpwb.log`, `sink.log`, the captures and `sent_report.txt` /
`received_report.txt` from `a2dpwb_decode --le-audio`.

Each case checks: A2DPWB exits 0; one stream of the expected configuration
on both sides; no decoder problems (consecutive SDU sequence numbers,
lengths, LC3 decoding, time stamp steps); the sinks received every SDU A2DPWB
sent; decoded audio identical on both sides; the test tone (1 kHz left,
1.5 kHz right; mono: both in each); the teardown steps in the log; then the
checks of every `le_audio_tests/<profile>.py`.

Add your cases and checks to `tools/emu/le_audio_tests/<profile>.py`
(standard library only):

```python
from .case import Case

CASES = [Case('earbuds-csip', 'earbuds', r'LC3 48000 Hz 10 ms 155 octets, 2 channel\(s\) over 2 CIS',
              sink_args=['--csip-set'], a2dpwb_args=[], tags=('csip',))]

def check(case, ctx):        # ctx: out, a2dpwb_log, sink_log, sent, received, sink_addresses
    if 'csip' not in case.tags:
        return []
    return [] if 'CSIP: set of 2' in ctx['a2dpwb_log'] else ['A2DPWB did not find the set']
```

A case runs `A2DPWB.exe --cli --hci-tcp ... --le-audio -d <first sink>
--le-device <other sinks> --test-tone --duration N --hci-capture ...` plus
`a2dpwb_args`. A CSIP case that should find the second earbud itself can
pass only the first address: add an emu / A2DPWB option for it rather than
changing the default flow for the other cases.

## Working in parallel without conflicts

- Your files: `app/src/le_audio/le_audio_<profile>.{h,cpp}`,
  `tools/emu/le_audio/<profile>.py`, `tools/emu/le_audio_tests/<profile>.py`.
  New source files of yours: add them to `app/CMakeLists.txt` next to your
  stub's line (one line each).
- Shared files - touch only if unavoidable, in a small separate commit:
  `le_audio_presets.h` (e.g. a CCID field in `StreamRequest` for MCP / CCP),
  `le_audio_bap.cpp` (`Unicast::enter(Phase::Enable)` builds the Enable
  metadata), `le_audio_module.h` (a new hook: add it with an empty default
  at the end of its section). `le_audio_modules.cpp` and `le_audio/__init__.py`
  / `le_audio_tests/__init__.py` already list every profile.
- Strings for the UI: add keys to all `app/lang/*.json` next to the existing
  `*.le_audio*` keys (run `python tools/check_lang.py`).

## Pitfalls

- BTstack thread only, and never block it (see Threads).
- `gap_cis_create()` takes all CIS of the CIG at once and a CIG cannot get
  new CIS later: to change the CIS set, `stop_stream()` (removes the CIG) and
  start again. Do not call `gap_cig_remove()` yourself.
- Only one LE connection can be set up at a time (`Core::connect` returns an
  error for a second concurrent one): connect set members one after the
  other in `prepare_group()`.
- The LE ACL buffers of real controllers can be small (27 bytes in the
  emulator): GATT writes are fragmented; the ATT MTU is negotiated by BTstack
  before the first query.
- `Core::scan()` / `find()` interrupt nothing else, but scanning while
  streaming costs radio time on a real controller.
- `ScanResult::addr.type` matters: connect with the type the advertisement
  showed (`find()` does that for the CLI / GUI).
- The device / CIS handles in captures differ between A2DPWB's and the sink's
  controller; `a2dpwb_decode --le-audio` maps them by CIG / CIS id.

## Known limitations

- Sink ASEs only (no microphone / source ASE, no conversational streams);
  ISO data from the controller is ignored.
- No broadcast (BIS), no BASS, no CAP procedures beyond what BAP needs; CAS is
  only advertised by the emulator.
- SDUs without time stamps; no use of LE Read ISO TX Sync; the jitter buffer
  is simple (fixed 5 sets).
- Application capture (one app) is not supported for LE Audio yet (system
  loopback, virtual device and the test tone are).
- Not verified with real LE Audio headphones or a real controller's CIS
  scheduling; extended advertising scanning is enabled but only exercised
  through BTstack's fallback to legacy scanning in the emulator.
