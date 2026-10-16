"""A test case of run_test.py --le-audio (see __init__.py)."""

import dataclasses

STEREO_TONES = (((1000.0, 0.5),), ((1500.0, 0.5),))   # left 1 kHz, right 1.5 kHz (--test-tone)
MONO_TONES = (((1000.0, 0.25), (1500.0, 0.25)),) * 2  # downmix (L + R) / 2 on both channels


@dataclasses.dataclass
class Case:
    name: str                 # also the output folder: <out>/le-<name>
    layout: str               # emu_le_audio_sink.py --layout
    config: str               # regex for the decoder's "Configuration :" line
    # Expected test tone per WAV channel (left, right): ((freq, amp), ...),
    # or None to skip the tone check
    tones: tuple = STEREO_TONES
    sink_args: list = dataclasses.field(default_factory=list)    # extra emu arguments
    a2dpwb_args: list = dataclasses.field(default_factory=list)  # extra A2DPWB arguments
    tags: tuple = ()          # profiles exercised, e.g. ('csip',)
