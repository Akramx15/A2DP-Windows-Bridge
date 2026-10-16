/*
 * LE Audio (experimental) - CCP: Generic Telephone Bearer Service on the PC
 *
 * The PC is the CCP Call Control Server: it exposes GTBS (TBS 1.0, service
 * 0x184C) and a headset (the CCP client) reads the call state from it and
 * answers / ends / holds calls through its Call Control Point.
 *
 * A2DPWB has no telephony: Windows has no generic call-state API and the LE
 * Audio path has no microphone (no source ASEs). So this is a spec-correct
 * GTBS server around an in-memory call model (le_audio::ccp below) that
 * something else drives:
 *  - the headset, through the Call Control Point (Accept, Terminate, Local
 *    Hold / Retrieve, Originate);
 *  - the telephony side, through the "events" below. Today that is only the
 *    simulation for tests / debugging (--ccp-sim, --ccp-console); a real
 *    integration (a softphone, a VoIP bridge, ...) would implement
 *    ccp::Backend and call the events.
 *
 * Simulation (A2DPWB --cli --le-audio ...; this module reads the options
 * from the process command line itself, main.cpp ignores them):
 *   --ccp-sim "<t>:<cmd>;<t>:<cmd>;..."  run each command t seconds after the
 *                                        first CCP client enabled Call State
 *                                        notifications
 *   --ccp-console                        read commands from stdin (CLI mode)
 * Commands (ccp::simulate()):
 *   incoming <uri>[|<friendly name>[|<target uri>]]   a call comes in
 *   alerting [<index>]           an outgoing call rings at the far end
 *   answered [<index>]           the far end answered an outgoing call
 *   remote-hold [<index>] / remote-retrieve [<index>]
 *   hangup [<index> [<reason>]]  the call ended (default reason 0x02, remote
 *                                party ended; 3 = the server ended it)
 *   status                       log the call list
 * <index> defaults to the most recent call.
 *
 * Content Control ID: GTBS has the fixed CCID ccp::GTBS_CCID (CCID_GTBS; MCP's GMCS
 * uses CCID_GMCS). add_ccids() puts it in the CCID List of the BAP Enable
 * metadata for streams with the Conversational context.
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#ifndef LE_AUDIO_CCP_H
#define LE_AUDIO_CCP_H

#include "le_audio_module.h"

#include <cstdint>
#include <string>
#include <vector>

namespace le_audio {

/* Registered in le_audio_modules.cpp */
Module *ccp_module();

namespace ccp {

/* Content Control ID of GTBS (CCID_GTBS from le_audio_module.h) */
constexpr uint8_t GTBS_CCID = CCID_GTBS;
inline uint8_t ccid() { return GTBS_CCID; }

/* TBS 1.0 Call State values */
enum class CallState : uint8_t {
    Incoming = 0x00,
    Dialing = 0x01,                  /* outgoing, not yet ringing at the far end */
    Alerting = 0x02,                 /* outgoing, ringing at the far end */
    Active = 0x03,
    LocallyHeld = 0x04,
    RemotelyHeld = 0x05,
    LocallyAndRemotelyHeld = 0x06,
};

/* Call Flags */
constexpr uint8_t CALL_FLAG_OUTGOING = 0x01;
constexpr uint8_t CALL_FLAG_WITHHELD_BY_SERVER = 0x02;
constexpr uint8_t CALL_FLAG_WITHHELD_BY_NETWORK = 0x04;

/* Call Control Point opcodes */
constexpr uint8_t OP_ACCEPT = 0x00;
constexpr uint8_t OP_TERMINATE = 0x01;
constexpr uint8_t OP_LOCAL_HOLD = 0x02;
constexpr uint8_t OP_LOCAL_RETRIEVE = 0x03;
constexpr uint8_t OP_ORIGINATE = 0x04;
constexpr uint8_t OP_JOIN = 0x05;

/* Call Control Point notification result codes */
constexpr uint8_t RESULT_SUCCESS = 0x00;
constexpr uint8_t RESULT_OPCODE_NOT_SUPPORTED = 0x01;
constexpr uint8_t RESULT_OPERATION_NOT_POSSIBLE = 0x02;
constexpr uint8_t RESULT_INVALID_CALL_INDEX = 0x03;
constexpr uint8_t RESULT_STATE_MISMATCH = 0x04;
constexpr uint8_t RESULT_LACK_OF_RESOURCES = 0x05;
constexpr uint8_t RESULT_INVALID_OUTGOING_URI = 0x06;

/* Termination Reason codes */
constexpr uint8_t REASON_IMPROPER_URI = 0x00;
constexpr uint8_t REASON_CALL_FAILED = 0x01;
constexpr uint8_t REASON_REMOTE_ENDED = 0x02;
constexpr uint8_t REASON_SERVER_ENDED = 0x03;
constexpr uint8_t REASON_LINE_BUSY = 0x04;
constexpr uint8_t REASON_NETWORK_CONGESTION = 0x05;
constexpr uint8_t REASON_CLIENT_TERMINATED = 0x06;
constexpr uint8_t REASON_NO_SERVICE = 0x07;
constexpr uint8_t REASON_NO_ANSWER = 0x08;
constexpr uint8_t REASON_UNSPECIFIED = 0x09;

/* ATT application error of TBS: the value changed between the parts of a long read */
constexpr uint8_t ATT_ERROR_VALUE_CHANGED_DURING_READ_LONG = 0x80;

/* Calls the server keeps at most (Originate / incoming beyond: lack of resources) */
constexpr size_t MAX_CALLS = 4;

struct Call {
    uint8_t index = 0;               /* 1-255 */
    CallState state = CallState::Incoming;
    uint8_t flags = 0;               /* CALL_FLAG_* */
    std::string uri;                 /* remote party, e.g. "tel:+15551234567" */
    std::string friendly_name;       /* may be empty */
    std::string target_uri;          /* incoming: our URI the call came in on (may be empty) */
};

/* The telephony side. Called on the BTstack thread. The default backend is
 * the simulation: it allows everything. A real integration would carry out
 * the request (e.g. tell a softphone to answer) and return a RESULT_* code;
 * on RESULT_SUCCESS the server applies the state change and notifies the
 * headset; later changes come back through the events below. */
class Backend {
public:
    virtual ~Backend() = default;
    virtual std::string provider_name() { return "A2DPWB"; }
    /* Uniform Caller Identifier (Assigned Numbers); "un000" = unknown */
    virtual std::string uci() { return "un000"; }
    /* Bearer Technology: 0x04 Wi-Fi (TBS has no "wired IP" value) */
    virtual uint8_t technology() { return 0x04; }
    /* Comma-separated URI schemes (Originate checks them) */
    virtual std::string uri_schemes() { return "tel,sip"; }
    /* Local Hold / Local Retrieve available (Call Control Point Optional Opcodes) */
    virtual bool supports_local_hold() { return true; }

    /* Requests of the headset; return a RESULT_* code */
    virtual uint8_t accept(const Call &call) { (void)call; return RESULT_SUCCESS; }
    virtual uint8_t terminate(const Call &call) { (void)call; return RESULT_SUCCESS; }
    virtual uint8_t hold(const Call &call) { (void)call; return RESULT_SUCCESS; }
    virtual uint8_t retrieve(const Call &call) { (void)call; return RESULT_SUCCESS; }
    virtual uint8_t originate(const std::string &uri) { (void)uri; return RESULT_SUCCESS; }
};

/* Replace the backend (nullptr: the simulation). BTstack thread, or before
 * the stack starts; the object must outlive its use. The bearer values
 * (name, UCI, ...) are read when a client reads them. */
void set_backend(Backend *backend);

/* ---- Events of the telephony side (BTstack thread; Core::post() from others) ---- */

/* A call comes in: returns its call index, 0 if the list is full */
uint8_t incoming_call(const std::string &uri, const std::string &friendly_name = {},
                      const std::string &target_uri = {});
/* An outgoing call rings at the far end (Dialing -> Alerting) */
bool remote_alerting(uint8_t index);
/* The far end answered an outgoing call (Dialing / Alerting -> Active) */
bool remote_answered(uint8_t index);
/* The far end put the call on hold / took it back */
bool remote_hold(uint8_t index, bool held);
/* The call ended (reason: REASON_*) */
bool call_ended(uint8_t index, uint8_t reason);
/* The current calls */
std::vector<Call> calls();

/* A simulation command (see the top of this file); false and why if it
 * cannot be done. BTstack thread. */
bool simulate(const std::string &command, std::string *error = nullptr);

} /* namespace ccp */

} /* namespace le_audio */

#endif /* LE_AUDIO_CCP_H */
