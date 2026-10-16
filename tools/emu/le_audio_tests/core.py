"""BAP unicast cases: the stream set-ups A2DPWB plans from the sink's PACS /
ASCS (le_audio_presets.cpp), one per emu_le_audio_sink.py layout."""

import re

from .case import MONO_TONES, Case

CASES = [
    # One headset, one 2-channel ASE: 1 CIS, LC3 48_6_2 (the best media preset)
    Case('stereo', 'stereo', r'LC3 48000 Hz 10 ms 155 octets, 2 channel\(s\) over 1 CIS'),
    # One headset, two 1-channel ASEs: 2 CIS on one link
    Case('two-ase', 'two-ase', r'LC3 48000 Hz 10 ms 155 octets, 2 channel\(s\) over 2 CIS'),
    # Two earbuds (Front Left / Front Right), one ASE each: 2 CIS on two links
    Case('earbuds', 'earbuds', r'LC3 48000 Hz 10 ms 155 octets, 2 channel\(s\) over 2 CIS'),
    # Mono sink with a forced low-latency preset (--le-preset): downmix, 16 kHz
    Case('mono-16k', 'mono', r'LC3 16000 Hz 10 ms 40 octets, 1 channel\(s\) over 1 CIS',
         tones=MONO_TONES, a2dpwb_args=['--le-preset', '16_2_1']),
    # The sink drops the LE link 3 s after A2DPWB connected: A2DPWB tears the
    # stream down (CIS, CIG) and sets it up again on a new link (bond reused)
    Case('link-loss', 'stereo', r'LC3 48000 Hz 10 ms 155 octets, 2 channel\(s\) over 1 CIS',
         sink_args=['--drop-link-after', '3'], streams=2, lossy=True, duration=14),
    # The sink keeps released ASEs in Releasing until the client drops the
    # CIS: A2DPWB disconnects the CIS after Disable, so stop does not wait
    # out the Release timeout
    Case('release-cis', 'two-ase', r'LC3 48000 Hz 10 ms 155 octets, 2 channel\(s\) over 2 CIS',
         sink_args=['--ascs-release-waits-for-cis'], tags=('release-cis',)),
    # No sink context available until 3 s after the connection: Enable waits
    # for the Available Audio Contexts notification instead of failing
    Case('contexts-later', 'stereo', r'LC3 48000 Hz 10 ms 155 octets, 2 channel\(s\) over 1 CIS',
         sink_args=['--available-contexts-after', '3'], tags=('contexts-later',)),
]


def check(case, ctx):
    failures = []
    log = ctx['a2dpwb_log']
    if 'Streaming LE Audio:' not in log:
        failures.append('A2DPWB did not report "Streaming LE Audio"')
    # The stream must be torn down in order: Disable / Release / Remove CIG
    for step in ('unicast: Disable', 'unicast: Release', 'CIG 1 removed'):
        if step not in log:
            failures.append(f'teardown step missing from the log: {step}')
    m = re.search(r'LE Audio: (\d+) SDU sets sent .*?(\d+) underruns', log)
    if not m:
        failures.append('no LE Audio statistics line')
    if case.name == 'link-loss' and 'LE Audio stream restored' not in log:
        failures.append('the stream was not set up again after the link loss')
    for timeout in ('Disable timed out', 'Release timed out', 'CIG removal timed out'):
        if timeout in log and not case.lossy:
            failures.append(f'teardown: {timeout}')
    if 'release-cis' in case.tags and 'disconnecting CIS' not in log:
        failures.append('A2DPWB did not disconnect the CIS before Release')
    if 'contexts-later' in case.tags and 'no audio context available for Enable' not in log:
        failures.append('A2DPWB did not wait for an available audio context')
    return failures
