"""Bumble virtual controller with what LE Audio needs on the in-process link.

Bumble's Controller sets up CIGs and CISes but does not carry HCI ISO data
between the two controllers of a LocalLink. LeEmuController adds that:

  - an HCI ISO data packet (complete SDU) from the host of the central is
    delivered to the host of the peripheral on the matching CIS (same CIG /
    CIS ID), with a time stamp and the sender's sequence number, at the next
    free SDU interval of that CIS (like the air schedule: one SDU per
    interval), and Number Of Completed Packets goes back to the sender then.
    So A2DPWB is paced by the "controller" as with a real one.
  - LE CIS Established reports the CIG parameters (ISO interval, max PDU,
    PHY 2M) instead of zeros.
  - pending ISO deliveries are flushed before a CIS disconnects or its CIG
    is removed, so the sink receives every SDU A2DPWB sent.

PacketLoggerSnooper also writes ISO data (PacketLogger types 0x0c / 0x0d),
which a2dpwb_decode --le-audio reads.
"""

import asyncio
import logging
import math
import os
import struct
import sys
import time

from bumble import core, hci
from bumble.link import LocalLink
from bumble.snoop import Snooper

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from emu_sink import EmuController  # noqa: E402  (classic HCI additions, ACL buffers)

log = logging.getLogger('emu_le_audio')

# CIG parameters set by the central, by (cig_id, cis_id): the peripheral's
# controller reports them in its LE CIS Established too (one central here)
CIS_PARAMS = {}
LATE_INTERVALS = 3
LATE_US = 30000      # the same tolerance in time for shorter SDU intervals


class LeEmuController(EmuController):
    """EmuController plus HCI ISO data over the LocalLink (see module doc)."""

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.iso_next_slot = {}    # CIS handle -> number of the next free SDU interval
        self.iso_pending = {}      # CIS handle -> [(TimerHandle, deliver)]
        self.iso_t0 = None
        self.iso_count = 0

    # ---- host -> controller ----

    def on_hci_packet(self, packet):
        if isinstance(packet, hci.HCI_IsoDataPacket):
            self.on_hci_iso_data_packet(packet)
            return
        super().on_hci_packet(packet)

    def on_hci_le_set_cig_parameters_command(self, command):
        for i, cis_id in enumerate(command.cis_id):
            CIS_PARAMS[(command.cig_id, cis_id)] = dict(
                sdu_interval=command.sdu_interval_c_to_p,
                latency_ms=command.max_transport_latency_c_to_p,
                max_sdu_c_to_p=command.max_sdu_c_to_p[i],
                max_sdu_p_to_c=command.max_sdu_p_to_c[i],
                rtn=command.rtn_c_to_p[i],
            )
        return super().on_hci_le_set_cig_parameters_command(command)

    def on_le_cis_established(self, cig_id, cis_id):
        cis_link = next(
            (link for link in list(self.central_cis_links.values()) + list(self.peripheral_cis_links.values())
             if link.cis_id == cis_id and link.cig_id == cig_id), None)
        if cis_link is None:
            return
        p = CIS_PARAMS.get((cig_id, cis_id), dict(sdu_interval=10000, latency_ms=20, max_sdu_c_to_p=0,
                                                  max_sdu_p_to_c=0, rtn=2))
        self.send_hci_packet(
            hci.HCI_LE_CIS_Established_Event(
                status=hci.HCI_ErrorCode.SUCCESS,
                connection_handle=cis_link.handle,
                cig_sync_delay=p['sdu_interval'] // 2,
                cis_sync_delay=p['sdu_interval'] // 2,
                transport_latency_c_to_p=p['latency_ms'] * 1000,
                transport_latency_p_to_c=p['latency_ms'] * 1000,
                phy_c_to_p=2,
                phy_p_to_c=2,
                nse=min(p['rtn'] + 1, 31),
                bn_c_to_p=1 if p['max_sdu_c_to_p'] else 0,
                bn_p_to_c=1 if p['max_sdu_p_to_c'] else 0,
                ft_c_to_p=1,
                ft_p_to_c=1,
                max_pdu_c_to_p=p['max_sdu_c_to_p'],
                max_pdu_p_to_c=p['max_sdu_p_to_c'],
                iso_interval=max(p['sdu_interval'] // 1250, 4),
            )
        )

    def _peer_cis(self, link):
        """(peer controller, peer CIS link) of a CIS link, or (None, None)"""
        if not self.link or link.acl_connection is None:
            return None, None
        peer = self.link.find_le_controller(link.acl_connection.peer_address)
        if peer is None:
            return None, None
        links = list(peer.peripheral_cis_links.values()) + list(peer.central_cis_links.values())
        peer_link = next((x for x in links if x.cig_id == link.cig_id and x.cis_id == link.cis_id), None)
        return peer, peer_link

    def on_hci_iso_data_packet(self, packet):
        handle = packet.connection_handle
        link = self.central_cis_links.get(handle) or self.peripheral_cis_links.get(handle)
        loop = asyncio.get_running_loop()

        def completed():
            self.send_hci_packet(hci.HCI_Number_Of_Completed_Packets_Event(
                connection_handles=[handle], num_completed_packets=[1]))

        if link is None:
            log.warning('ISO data for unknown handle 0x%04X', handle)
            completed()
            return
        if packet.pb_flag != 0b10:
            log.warning('ISO data with PB flag %s (only complete SDUs are carried)', bin(packet.pb_flag))
        params = CIS_PARAMS.get((link.cig_id, link.cis_id), {})
        interval_us = params.get('sdu_interval', 10000)
        interval = interval_us / 1e6
        now = loop.time()
        if self.iso_t0 is None:
            self.iso_t0 = now
        # SDU interval n of the CIS: the next free one, or the current one if
        # the host fell behind by more than LATE_INTERVALS (a real controller
        # would have flushed those). The tolerance absorbs the timer jitter of
        # this emulation (asyncio timers fire up to ~16 ms late on Windows).
        # That jitter is in time, not intervals: at least LATE_US, so 7.5 ms
        # SDU intervals (GMAP) get 4 intervals where 10 ms ones get 3. A real
        # controller only flushes an SDU after the CIG's Max Transport
        # Latency, so allow that much too: this emulation's event loop
        # sometimes stalls for 60-90 ms, which is not the host being late.
        late_us = max(LATE_US, params.get('latency_ms', 0) * 1000)
        late = max(LATE_INTERVALS, math.ceil(late_us / interval_us))
        n_now = int(math.ceil((now - self.iso_t0) / interval - 1e-6)) - late
        n = max(self.iso_next_slot.get(handle, 0), n_now)
        self.iso_next_slot[handle] = n + 1
        slot = self.iso_t0 + n * interval
        time_stamp = (n * interval_us) & 0xFFFFFFFF
        seq = packet.packet_sequence_number or 0
        data = packet.iso_sdu_fragment
        entry = []

        def deliver():
            pending = self.iso_pending.get(handle, [])
            if entry and entry[0] in pending:
                pending.remove(entry[0])
            peer, peer_link = self._peer_cis(link)
            if peer is not None and peer_link is not None:
                peer.send_hci_packet(hci.HCI_IsoDataPacket(
                    connection_handle=peer_link.handle,
                    pb_flag=0b10,
                    data_total_length=8 + len(data),
                    time_stamp=time_stamp,
                    packet_sequence_number=seq,
                    iso_sdu_length=len(data),
                    packet_status_flag=0,
                    iso_sdu_fragment=data,
                ))
            completed()
            self.iso_count += 1

        timer = loop.call_at(slot, deliver)
        entry.append((timer, deliver))
        self.iso_pending.setdefault(handle, []).append(entry[0])

    def flush_iso(self, handles):
        for handle in handles:
            for timer, deliver in list(self.iso_pending.get(handle, [])):
                timer.cancel()
                deliver()
            self.iso_pending.pop(handle, None)
            self.iso_next_slot.pop(handle, None)

    def on_le_disconnected(self, connection, reason):
        # The CISes of an ACL go down with it, each with its own Disconnection
        # Complete before the ACL's (Bumble keeps them, so BTstack would wait
        # for them for ever)
        for links, central in ((self.central_cis_links, True), (self.peripheral_cis_links, False)):
            for handle, link in list(links.items()):
                if link.acl_connection is not connection:
                    continue
                self.flush_iso([handle])
                if central:
                    link.acl_connection = None   # removed with LE Remove CIG
                else:
                    links.pop(handle)
                self.send_hci_packet(hci.HCI_Disconnection_Complete_Event(
                    status=hci.HCI_ErrorCode.SUCCESS, connection_handle=handle, reason=reason))
        super().on_le_disconnected(connection, reason)

    def on_hci_disconnect_command(self, command):
        handle = command.connection_handle
        self.flush_iso([handle])
        link = self.central_cis_links.get(handle) or self.peripheral_cis_links.get(handle)
        if link is not None and (link.acl_connection is None or
                                 link.acl_connection.peer_address not in self.le_connections):
            # A CIS whose ACL is gone (already reported down)
            self._send_hci_command_status(hci.HCI_ErrorCode.UNKNOWN_CONNECTION_IDENTIFIER_ERROR,
                                          command.op_code)
            return None
        return super().on_hci_disconnect_command(command)

    def on_hci_le_remove_cig_command(self, command):
        self.flush_iso([h for h, link in self.central_cis_links.items() if link.cig_id == command.cig_id])
        return super().on_hci_le_remove_cig_command(command)


class LeLocalLink(LocalLink):
    """LocalLink that sends LE ACL data from the address the sender's
    connection uses. Bumble's LocalLink takes the sender controller's random
    address, which does not match a connection made with a public address
    (BTstack connects with its public address)."""

    def send_acl_data(self, sender_controller, destination_address, transport, data):
        if transport == core.PhysicalTransport.LE:
            connection = sender_controller.le_connections.get(destination_address)
            destination = self.find_le_controller(destination_address)
            if connection is not None and destination is not None:
                source = connection.self_address
                asyncio.get_running_loop().call_soon(
                    lambda: destination.on_link_acl_data(source, transport, data))
                return
        super().send_acl_data(sender_controller, destination_address, transport, data)


class PacketLoggerSnooper(Snooper):
    """Writes HCI packets in PacketLogger format (as A2DPWB's own capture),
    ISO data included."""

    def __init__(self, path):
        self.file = open(path, 'wb', buffering=0)  # unbuffered: survives being killed

    def snoop(self, hci_packet, direction):
        h4_type, payload = hci_packet[0], hci_packet[1:]
        to_controller = direction == Snooper.Direction.HOST_TO_CONTROLLER
        if h4_type == hci.HCI_COMMAND_PACKET:
            pl_type = 0x00
        elif h4_type == hci.HCI_EVENT_PACKET:
            pl_type = 0x01
        elif h4_type == hci.HCI_ACL_DATA_PACKET:
            pl_type = 0x02 if to_controller else 0x03
        elif h4_type == hci.HCI_ISO_DATA_PACKET:
            pl_type = 0x0C if to_controller else 0x0D
        else:
            return
        now = time.time()
        sec, usec = int(now), int((now - int(now)) * 1e6)
        self.file.write(struct.pack('>IIIB', 9 + len(payload), sec, usec, pl_type) + payload)
