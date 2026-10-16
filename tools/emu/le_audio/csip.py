"""CSIP on the emulated sinks (stub: does nothing yet).

Owner: the CSIP work package. Plan: with --sinks 2 the two sinks are the
earbuds of one coordinated set: add bumble.profiles.csip.
CoordinatedSetIdentificationService (same SIRK, set size 2, rank 1 / 2) to
each sink.device, include it in a CAS (bumble.profiles.cap) instead of the
plain CAS of bap.py, and put the RSI (AD type 0x2E,
AdvertisingData.RESOLVABLE_SET_IDENTIFIER) into sink.adv_structures.
Options: --csip-... (add_arguments).
"""


def add_arguments(parser):
    pass


def configure_sink(sink, args):
    pass


async def start(sink, args):
    pass
