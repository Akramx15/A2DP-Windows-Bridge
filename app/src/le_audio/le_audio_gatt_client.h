/*
 * LE Audio (experimental) - serialized GATT client operations
 *
 * BTstack's gatt_client runs one query per connection at a time and returns
 * GATT_CLIENT_IN_WRONG_STATE for a second one. Every GATT client operation of
 * the core and of the profile modules therefore goes through this queue: it
 * keeps a FIFO per connection, starts the next operation when the previous
 * one completed, and calls the operation's callback with the result.
 *
 * All functions must be called on the BTstack thread (from a BTstack
 * callback, a module hook, or le_audio::Core::run_on_stack()). Callbacks run
 * on the BTstack thread too. Status values: 0 = success, otherwise an ATT
 * error code (ATT_ERROR_*, e.g. 0x1F ATT_ERROR_HCI_DISCONNECT_RECEIVED when
 * the link went down) or the BTstack status of a query that could not start.
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#ifndef LE_AUDIO_GATT_CLIENT_H
#define LE_AUDIO_GATT_CLIENT_H

#include "le_audio_device.h"

#include <cstdint>
#include <functional>
#include <vector>

namespace le_audio {

using GattDone = std::function<void(uint8_t status)>;
using GattValue = std::function<void(uint8_t status, const uint8_t *value, uint16_t len)>;
using GattNotify = std::function<void(const uint8_t *value, uint16_t len)>;
using GattDiscovered = std::function<void(uint8_t status, std::vector<GattService> services)>;

namespace gatt_client {

/* Read a characteristic value (long read: values beyond the MTU are complete) */
void read(uint16_t con, uint16_t value_handle, GattValue done);

/* Write with response (a long write when the value does not fit the MTU) */
void write(uint16_t con, uint16_t value_handle, std::vector<uint8_t> value, GattDone done = {});

/* Write without response, in order with the queued operations */
void write_without_response(uint16_t con, uint16_t value_handle, std::vector<uint8_t> value);

/* Enable notifications (or indications) of a characteristic: writes its CCC
 * descriptor. Values arrive at the listen() callbacks. */
void subscribe(uint16_t con, const GattChar &chr, bool indications, GattDone done = {});

/* Discover all primary services, their included (secondary) services and all
 * characteristics. The core runs this once per connection and stores the
 * result in LeAudioDevice::services; modules use that cache. */
void discover_all(uint16_t con, GattDiscovered done);

/* Generic escape hatch for any other gatt_client query. start() calls one
 * gatt_client_* function with the given packet handler and returns its
 * status; every GATT event of the query is passed to on_event(); done() gets
 * the ATT status of GATT_EVENT_QUERY_COMPLETE. */
using StartFn = std::function<uint8_t(void (*handler)(uint8_t, uint16_t, uint8_t *, uint16_t), uint16_t con)>;
using EventFn = std::function<void(const uint8_t *packet, uint16_t size)>;
void custom(uint16_t con, StartFn start, EventFn on_event, GattDone done);

/* Notifications / indications of value_handle on con. Registrations are
 * dropped when the connection goes down (detach). */
void listen(uint16_t con, uint16_t value_handle, GattNotify cb);

/* Number of operations queued or running on con */
size_t pending(uint16_t con);

/* Core only: a new LE connection / the connection went down (the running
 * and queued operations complete with ATT_ERROR_HCI_DISCONNECT_RECEIVED) */
void attach(uint16_t con);
void detach(uint16_t con);
void init();      /* on stack init */
void deinit();

} /* namespace gatt_client */
} /* namespace le_audio */

#endif /* LE_AUDIO_GATT_CLIENT_H */
