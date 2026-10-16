"""LE Audio pieces of the virtual sink (tools/emu/emu_le_audio_sink.py).

The sink side of each LE Audio profile is a module here, in the order of
MODULES. A module may define any of:

    add_arguments(parser)       its command-line options (prefix --<name>-)
    configure_sink(sink, args)  before power on, for every emulated sink
                                (sink.SinkContext): add GATT services with
                                sink.device.add_service(), AD structures to
                                sink.adv_structures, keep state in sink.state
    async start(sink, args)     after the sink is powered on and advertising

bap.py is the unicast server (PACS, ASCS, CAS) every sink has; csip, vcp,
mcp, ccp and tmap_gmap are stubs for the profile work packages, each owned by
one of them. emu_le_audio_sink.py calls the modules; add checks for
run_test.py in your module's own files, not in bap.py.
"""

from . import bap, ccp, csip, mcp, tmap_gmap, vcp

MODULES = [bap, csip, vcp, mcp, ccp, tmap_gmap]


def add_arguments(parser):
    for module in MODULES:
        if hasattr(module, 'add_arguments'):
            module.add_arguments(parser)


def configure_sink(sink, args):
    for module in MODULES:
        if hasattr(module, 'configure_sink'):
            module.configure_sink(sink, args)


async def start(sink, args):
    for module in MODULES:
        if hasattr(module, 'start'):
            await module.start(sink, args)
