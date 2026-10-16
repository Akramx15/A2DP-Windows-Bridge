"""TMAP and GMAP on the emulated sinks (stub: does nothing yet).

Owner: the TMAP + GMAP work package. Plan: add bumble.profiles.tmap
TelephonyAndMediaAudioService (role Unicast Media Receiver) and, with
--gmap-ugt, bumble.profiles.gmap GamingAudioService (role Unicast Game
Terminal) to each sink.device, with matching PAC / QoS so A2DPWB picks the
GMAP low-latency presets; read the PC's TMAS / GMAS from the sink side and
log them for run_test.py checks.
"""


def add_arguments(parser):
    pass


def configure_sink(sink, args):
    pass


async def start(sink, args):
    pass
