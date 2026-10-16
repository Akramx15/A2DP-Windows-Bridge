"""MCP on the emulated sinks: a Media Control Client of A2DPWB's GMCS.

The PC is the media control server (GMCS); with --mcp-client the first sink
is the client (as earbuds are): once A2DPWB's link is encrypted it discovers
GMCS with Bumble's GenericMediaControlServiceProxy, reads every
characteristic, subscribes and writes the --mcp-ops control point
operations, logging what it reads, the notifications and the result codes
(run_test.py checks sink.log, le_audio_tests/mcp.py).

Every sink also logs the metadata of each ASE Enable (Streaming Audio
Contexts, CCID List) for the CCID check, with or without --mcp-client.
"""

import asyncio
import logging
import struct

from bumble.att import ATT_Error
from bumble.device import Peer
from bumble.profiles.ascs import AseStateMachine
from bumble.profiles.mcp import (
    GenericMediaControlServiceProxy,
    MediaControlPointOpcode,
    MediaControlPointResultCode,
    MediaState,
)

log = logging.getLogger('emu_le_audio')

OPS = {
    'play': 0x01, 'pause': 0x02, 'fast-rewind': 0x03, 'fast-forward': 0x04, 'stop': 0x05,
    'move-relative': 0x10, 'first-segment': 0x22, 'prev': 0x30, 'next': 0x31, 'goto-track': 0x34,
}
NOTIFY_TIMEOUT = 5.0


def add_arguments(parser):
    parser.add_argument('--mcp-client', action='store_true',
                        help='the first sink is a Media Control Client of the GMCS of A2DPWB')
    parser.add_argument('--mcp-ops', default='',
                        help='control point writes after reading, comma separated: play, pause, stop, '
                             'next, prev, move-relative:<0.01 s>, any opcode as 0xNN, raw:<hex bytes>; '
                             'a trailing ! writes with response')
    parser.add_argument('--mcp-unsubscribed-write', action='store_true',
                        help='first write Play before subscribing to the control point (expects ATT 0xFD)')
    parser.add_argument('--mcp-delay', type=float, default=0.5,
                        help='seconds between encryption and the first GMCS request')
    parser.add_argument('--mcp-mtu', type=int,
                        help='largest ATT MTU the sink accepts (A2DPWB negotiates the MTU)')


def configure_sink(sink, args):
    if args.mcp_mtu and sink.index == 0:
        sink.device.gatt_server.max_mtu = args.mcp_mtu
    ascs = sink.state.get('bap', {}).get('ascs')
    if ascs is None:
        return

    def on_state(ase):
        if ase.state != AseStateMachine.State.ENABLING:
            return
        parts = []
        for entry in ase.metadata.entries:
            value = entry.decode()
            if isinstance(value, bytes):
                value = value.hex()
            elif hasattr(value, 'name') and value.name:
                value = value.name
            parts.append(f'{entry.tag.name}={value}')
        log.info('%s: MCP: ASE %d Enable metadata: %s', sink.name, ase.ase_id, ' '.join(parts) or '(none)')

    for ase in ascs.ase_state_machines.values():
        ase.on('state_change', lambda ase=ase: on_state(ase))


async def start(sink, args):
    if not args.mcp_client or sink.index != 0:
        return
    loop = asyncio.get_running_loop()

    def on_connection(connection):
        loop.create_task(run_client(sink, args, connection))

    sink.device.on('connection', on_connection)


def text(value):
    return value.decode('utf-8', errors='replace')


def parse_op(token):
    """('name', bytes, with_response) of an --mcp-ops item"""
    with_response = token.endswith('!')
    token = token.rstrip('!')
    if token.startswith('raw:'):
        return token, bytes.fromhex(token[4:]), with_response
    name, _, arg = token.partition(':')
    opcode = OPS[name] if name in OPS else int(name, 0)
    data = bytes([opcode])
    if arg:
        data += struct.pack('<i', int(arg, 0))
    return token, data, with_response


async def run_client(sink, args, connection):
    name = sink.name
    try:
        if not connection.is_encrypted:
            encrypted = asyncio.get_running_loop().create_future()
            connection.once(connection.EVENT_CONNECTION_ENCRYPTION_CHANGE,
                            lambda: encrypted.done() or encrypted.set_result(True))
            await asyncio.wait_for(encrypted, 30)
        await asyncio.sleep(args.mcp_delay)
        peer = Peer(connection)
        gmcs = await peer.discover_service_and_create_proxy(GenericMediaControlServiceProxy)
        if gmcs is None:
            log.info('%s: MCP: no GMCS on %s', name, connection.peer_address)
            return
        log.info('%s: MCP: GMCS found on %s, ATT MTU %d', name, connection.peer_address, connection.att_mtu)

        values = {}
        for field, decode in (
            ('media_player_name', text), ('track_title', text),
            ('track_duration', lambda v: struct.unpack('<i', v)[0]),
            ('track_position', lambda v: struct.unpack('<i', v)[0]),
            ('playback_speed', lambda v: struct.unpack('<b', v)[0]),
            ('seeking_speed', lambda v: struct.unpack('<b', v)[0]),
            ('media_state', lambda v: MediaState(v[0]).name),
            ('media_control_point_opcodes_supported', lambda v: f'0x{struct.unpack("<I", v)[0]:08x}'),
            ('content_control_id', lambda v: v[0]),
        ):
            proxy = getattr(gmcs, field)
            if proxy is None:
                log.info('%s: MCP: %s missing', name, field)
                continue
            raw = await proxy.read_value()
            values[field] = decode(raw)
            log.info('%s: MCP: read %s = %r (%d bytes)', name, field, values[field], len(raw))
        for field in ('track_changed', 'media_control_point'):
            log.info('%s: MCP: %s %s', name, field, 'present' if getattr(gmcs, field) else 'missing')

        if args.mcp_unsubscribed_write:
            try:
                await gmcs.media_control_point.write_value(bytes([OPS['play']]), with_response=True)
                log.info('%s: MCP: unsubscribed write accepted', name)
            except ATT_Error as e:
                log.info('%s: MCP: unsubscribed write -> ATT error 0x%02X', name, e.error_code)

        gmcs.on(gmcs.EVENT_MEDIA_STATE, lambda s: log.info('%s: MCP: notify media_state = %s', name, s.name))
        gmcs.on(gmcs.EVENT_TRACK_CHANGED, lambda: log.info('%s: MCP: notify track_changed', name))
        gmcs.on(gmcs.EVENT_TRACK_TITLE,
                lambda t: log.info('%s: MCP: notify track_title = %r (%d bytes)', name, t, len(t.encode())))
        gmcs.on(gmcs.EVENT_TRACK_DURATION, lambda d: log.info('%s: MCP: notify track_duration = %d', name, d))
        gmcs.on(gmcs.EVENT_TRACK_POSITION, lambda p: log.info('%s: MCP: notify track_position = %d', name, p))
        await gmcs.subscribe_characteristics()
        for field in ('media_player_name', 'media_control_point_opcodes_supported'):
            proxy = getattr(gmcs, field)
            if proxy is not None:
                await proxy.subscribe(lambda v, f=field: log.info('%s: MCP: notify %s = %s', name, f, v.hex()))
        log.info('%s: MCP: subscribed', name)

        for token in [t.strip() for t in args.mcp_ops.split(',') if t.strip()]:
            label, data, with_response = parse_op(token)
            while not gmcs.media_control_point_notifications.empty():
                gmcs.media_control_point_notifications.get_nowait()
            try:
                await gmcs.media_control_point.write_value(data, with_response=with_response)
            except ATT_Error as e:
                log.info('%s: MCP: control point %s -> ATT error 0x%02X', name, label, e.error_code)
                continue
            try:
                answer = await asyncio.wait_for(gmcs.media_control_point_notifications.get(), NOTIFY_TIMEOUT)
            except asyncio.TimeoutError:
                log.info('%s: MCP: control point %s -> no notification', name, label)
                continue
            opcode = MediaControlPointOpcode(answer[0])
            result = MediaControlPointResultCode(answer[1]) if len(answer) > 1 else None
            log.info('%s: MCP: control point %s -> opcode 0x%02x %s', name, label, opcode,
                     result.name if result is not None else answer.hex())
            await asyncio.sleep(0.3)        # let state notifications arrive in order
        position = struct.unpack('<i', await gmcs.track_position.read_value())[0]
        state = MediaState((await gmcs.media_state.read_value())[0]).name
        title = text(await gmcs.track_title.read_value())
        log.info('%s: MCP: final media_state = %s, track_position = %d, track_title = %r', name, state, position,
                 title)
        log.info('%s: MCP: client done', name)
    except Exception as e:  # noqa: BLE001  (log it for the check)
        log.info('%s: MCP: client failed: %r', name, e)
