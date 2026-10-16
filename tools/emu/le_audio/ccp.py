"""CCP on the emulated sinks (stub: does nothing yet).

Owner: the CCP work package. The PC is the call control server (GTBS); the
sink is the client. Plan: after A2DPWB connected, discover GTBS from
sink.device, read Bearer Provider Name / Call State and log them for
run_test.py checks (--ccp-... options).
"""


def add_arguments(parser):
    pass


def configure_sink(sink, args):
    pass


async def start(sink, args):
    pass
