"""MCP on the emulated sinks (stub: does nothing yet).

Owner: the MCP work package. The PC is the media control server (GMCS);
the sink is the client. Plan: once A2DPWB has connected (sink.device
'connection' event), discover GMCS with a bumble.gatt_client / profiles.mcp
client, read the player state and write a control point opcode (e.g. Pause)
so run_test.py can check A2DPWB's log for it (--mcp-... options).
"""


def add_arguments(parser):
    pass


def configure_sink(sink, args):
    pass


async def start(sink, args):
    pass
