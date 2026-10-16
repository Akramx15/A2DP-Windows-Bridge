"""One emulated LE Audio sink (a headset, or one earbud of a pair).

SinkContext carries what the profile modules need while setting the sink up
(le_audio/__init__.py): the Bumble Device, its index and address, the audio
locations it renders, the AD structures to advertise and a dict for the
modules' own state.
"""

import dataclasses
import logging

from bumble import hci
from bumble.core import AdvertisingData
from bumble.device import Device
from bumble.host import Host
from bumble.keys import MemoryKeyStore
from bumble.pairing import PairingConfig, PairingDelegate
from bumble.transport.common import AsyncPipeSink

from .controller import LeEmuController, PacketLoggerSnooper

log = logging.getLogger('emu_le_audio')


@dataclasses.dataclass
class SinkContext:
    index: int                       # 0, 1, ...
    address: str                     # public LE address "00:A2:D0:00:00:12"
    name: str
    locations: int                   # Sink Audio Locations (bap.AudioLocation bits)
    sink_ases: int                   # number of sink ASEs
    channel_counts: list             # PAC Supported Audio Channel Counts
    device: Device = None
    adv_structures: list = dataclasses.field(default_factory=list)  # (type, bytes)
    state: dict = dataclasses.field(default_factory=dict)            # per module


def create_device(link, sink, capture):
    """Controller + host + Device of the sink (not powered on yet)."""
    controller = LeEmuController(sink.name, link=link, public_address=sink.address)
    host = Host(controller, AsyncPipeSink(controller))
    if capture:
        host.snooper = PacketLoggerSnooper(capture)
    device = Device(name=sink.name, address=sink.address, host=host)
    device.le_enabled = True
    device.classic_enabled = False
    device.cis_enabled = True
    device.keystore = MemoryKeyStore()
    # LE Secure Connections, Just Works, bonding (as headsets without a display)
    device.pairing_config_factory = lambda connection: PairingConfig(
        sc=True, mitm=False, bonding=True,
        delegate=PairingDelegate(io_capability=PairingDelegate.IoCapability.NO_OUTPUT_NO_INPUT))
    sink.device = device
    return device


def advertising_data(sink):
    """Flags, the modules' AD structures and the name in what is left of the
    31 bytes of legacy advertising (shortened if needed). Bumble's virtual
    controller sends the advertising data as scan response too, so there is
    no extra room there."""
    structures = [(AdvertisingData.FLAGS, bytes([0x06]))] + sink.adv_structures
    data = bytes(AdvertisingData(structures))
    room = 31 - len(data) - 2
    name = sink.name.encode()
    if room >= len(name):
        data += bytes(AdvertisingData([(AdvertisingData.COMPLETE_LOCAL_NAME, name)]))
    elif room >= 3:
        data += bytes(AdvertisingData([(AdvertisingData.SHORTENED_LOCAL_NAME, name[:room])]))
    if len(data) > 31:
        log.warning('%s: advertising data is %d bytes, legacy advertising takes 31', sink.name, len(data))
    return data


async def power_on_and_advertise(sink):
    device = sink.device
    await device.power_on()
    await device.start_advertising(
        own_address_type=hci.OwnAddressType.PUBLIC,
        advertising_data=advertising_data(sink),
        auto_restart=True,
    )
    device.on('connection', lambda c: log.info('%s: LE connection from %s', sink.name, c.peer_address))
