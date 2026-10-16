"""VCP on the emulated sinks (stub: does nothing yet).

Owner: the VCP work package. Plan: add bumble.profiles.vcs.
VolumeControlService to each sink.device, log volume changes A2DPWB writes
(for run_test.py checks) and optionally change the volume from the sink
side (--vcp-... options) to test the other direction.
"""


def add_arguments(parser):
    pass


def configure_sink(sink, args):
    pass


async def start(sink, args):
    pass
