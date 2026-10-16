"""VCP on the emulated sinks: a Volume Control Service (VCS) renderer.

With --vcp every sink gets Bumble's VolumeControlService
(bumble.profiles.vcs) and logs, for run_test.py's checks (sink.log):

  VCS <sink>: Volume State read: volume V, mute M, counter C
              (A2DPWB's reads, and Bumble's for each notification)
  VCS <sink>: A2DPWB wrote <OPCODE> [V] (counter C): volume V, mute M, counter C
  VCS <sink>: A2DPWB wrote <OPCODE> [V] (counter C): rejected, error 0x80
  VCS <sink>: button: volume V, counter C (notified)
  VCS <sink>: change counter bumped to C without a notification

Options:
  --vcp                      VCS on every sink
  --vcp-volume V[,V2]        initial Volume_Setting (0-255) per sink (the last
                             one for the remaining sinks; default 100)
  --vcp-muted                the sinks start muted
  --vcp-press S:V[@N]        "button press": S seconds after A2DPWB connected,
                             sink N (default 0) sets volume V and notifies it
                             (repeatable)
  --vcp-bump-counter S       S seconds after A2DPWB first read Volume State,
                             every sink increments its change counter without
                             telling A2DPWB (a missed notification): A2DPWB's
                             next Control Point write gets "Invalid Change
                             Counter" (0x80) and has to read the state again
"""

import asyncio
import logging

from bumble import att
from bumble.profiles.vcs import VolumeControlPointOpcode, VolumeControlService

log = logging.getLogger('emu_le_audio')


class LoggingVolumeControlService(VolumeControlService):
    """Bumble's VCS server, logging what A2DPWB reads and writes."""

    def __init__(self, sink, **kwargs):
        super().__init__(**kwargs)
        self.sink = sink
        self.first_read = asyncio.Event()

    def describe(self):
        return f'volume {self.volume_setting}, mute {self.muted}, counter {self.change_counter}'

    def _on_read_volume_state(self, connection):
        log.info('VCS %s: Volume State read: %s', self.sink.name, self.describe())
        self.first_read.set()
        return super()._on_read_volume_state(connection)

    def _on_write_volume_control_point(self, connection, value):
        try:
            name = VolumeControlPointOpcode(value[0]).name
        except (ValueError, IndexError):
            name = f'opcode 0x{value[0]:02x}' if value else 'empty'
        operand = f' {value[2]}' if len(value) > 2 else ''
        counter = value[1] if len(value) > 1 else -1
        what = f'VCS {self.sink.name}: A2DPWB wrote {name}{operand} (counter {counter})'
        try:
            super()._on_write_volume_control_point(connection, value)
        except att.ATT_Error as error:
            log.info('%s: rejected, error 0x%02X', what, int(error.error_code))
            raise
        log.info('%s: %s', what, self.describe())

    async def press(self, volume):
        """A button on the device: a new volume, notified to the subscribers."""
        self.volume_setting = volume
        self.change_counter = (self.change_counter + 1) % 256
        log.info('VCS %s: button: volume %d, counter %d (notified)', self.sink.name, volume,
                 self.change_counter)
        await self.sink.device.notify_subscribers(attribute=self.volume_state)

    def bump_counter(self):
        self.change_counter = (self.change_counter + 1) % 256
        log.info('VCS %s: change counter bumped to %d without a notification', self.sink.name,
                 self.change_counter)


def add_arguments(parser):
    parser.add_argument('--vcp', action='store_true', help='Volume Control Service on every sink')
    parser.add_argument('--vcp-volume', default='100', metavar='V[,V2]',
                        help='initial volume 0-255 per sink (default 100)')
    parser.add_argument('--vcp-muted', action='store_true', help='the sinks start muted')
    parser.add_argument('--vcp-press', action='append', default=[], metavar='S:V[@N]',
                        help='S seconds after A2DPWB connected, sink N sets volume V (button)')
    parser.add_argument('--vcp-bump-counter', type=float, metavar='S',
                        help='S seconds after the first Volume State read, bump the change counter silently')


def _volume_for(args, index):
    values = [int(v) for v in args.vcp_volume.split(',')]
    return values[min(index, len(values) - 1)]


def configure_sink(sink, args):
    if not args.vcp:
        return
    vcs = LoggingVolumeControlService(sink, volume_setting=_volume_for(args, sink.index),
                                      muted=1 if args.vcp_muted else 0, volume_flags=0x01)
    sink.device.add_service(vcs)
    sink.state['vcp'] = vcs


async def start(sink, args):
    vcs = sink.state.get('vcp')
    if not vcs:
        return
    log.info('VCS %s: Volume Control Service, %s', sink.name, vcs.describe())
    loop = asyncio.get_running_loop()

    presses = []
    for item in args.vcp_press:
        spec, _, index = item.partition('@')
        seconds, _, volume = spec.partition(':')
        if int(index or 0) == sink.index:
            presses.append((float(seconds), int(volume)))
    if presses:
        started = []

        def on_connection(connection):
            if started:
                return
            started.append(True)

            async def run():
                start_time = loop.time()
                for seconds, volume in sorted(presses):
                    await asyncio.sleep(max(0.0, start_time + seconds - loop.time()))
                    await vcs.press(volume)
            loop.create_task(run())
        sink.device.on('connection', on_connection)

    if args.vcp_bump_counter is not None:
        async def bump():
            await vcs.first_read.wait()
            await asyncio.sleep(args.vcp_bump_counter)
            vcs.bump_counter()
        loop.create_task(bump())
