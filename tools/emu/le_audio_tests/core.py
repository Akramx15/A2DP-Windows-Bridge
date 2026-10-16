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
    return failures
