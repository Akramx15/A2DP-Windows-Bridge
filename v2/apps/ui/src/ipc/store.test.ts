import { createRoot } from "solid-js";
import { describe, expect, it } from "vitest";
import { sampleSnapshot } from "./mock";
import { createCore } from "./store";
import type { ConnStatus, Transport } from "./transport";
import type { Event, ServerMessage } from "./types";

/** A transport the test drives by hand */
function fakeTransport() {
  const calls: { method: string; params: unknown }[] = [];
  // The answer to state.snapshot, released by the test
  let answerSnapshot: (s: unknown) => void = () => {};
  let deliver: (m: ServerMessage) => void = () => {};
  let status: (s: ConnStatus) => void = () => {};
  const transport: Transport = {
    window: "main",
    mica: false,
    version: "test",
    async start(onMessage, onStatus) {
      deliver = onMessage;
      status = onStatus;
    },
    async call(method, params) {
      calls.push({ method, params });
      if (method === "state.snapshot") return new Promise((r) => (answerSnapshot = r));
      return null;
    },
    async showMain() {},
    async hideWindow() {},
    async openUrl() {},
    async openFolder() {},
    async revealPath() {},
    async setTheme() {},
    onNavigate() {},
    onMinimized() {},
    async updateStatus() {
      return { state: "idle" };
    },
    async updateInstall() {},
    onUpdatePhase() {},
  };
  return {
    transport,
    calls,
    send: (m: ServerMessage) => deliver(m),
    status: (s: ConnStatus) => status(s),
    event: (rev: number, event: Event) => deliver({ type: "event", rev, event }),
    answerSnapshot: (s: unknown) => answerSnapshot(s),
  };
}

const tick = () => new Promise((r) => setTimeout(r, 0));

describe("core store", () => {
  it("takes the snapshot and applies events in order", async () => {
    const f = fakeTransport();
    const core = createRoot(() => createCore(f.transport));
    await tick();
    expect(core.state.snap).toBeNull();
    f.send({ type: "snapshot", rev: 10, state: sampleSnapshot() });
    expect(core.state.rev).toBe(10);
    expect(core.state.snap?.scanning).toBe(false);

    f.event(11, { kind: "scanning", value: true });
    expect(core.state.snap?.scanning).toBe(true);
    expect(core.state.rev).toBe(11);

    f.event(12, {
      kind: "stream",
      value: { state: "connecting", message: { key: "status.connecting", args: [] }, profile: "X", info: null, volume: null },
    });
    expect(core.state.snap?.stream.state).toBe("connecting");

    f.event(13, { kind: "profiles", value: { profiles: [], active_profile: null } });
    expect(core.state.snap?.profiles).toHaveLength(0);
    expect(core.state.snap?.active_profile).toBeNull();
    // No resync was needed
    expect(f.calls.filter((c) => c.method === "state.snapshot")).toHaveLength(0);
  });

  it("asks for a new snapshot when a revision is missing", async () => {
    const f = fakeTransport();
    const core = createRoot(() => createCore(f.transport));
    await tick();
    f.send({ type: "snapshot", rev: 1, state: sampleSnapshot() });
    f.event(2, { kind: "scanning", value: true });
    expect(f.calls).toHaveLength(0);
    // Revision 3 was lost
    f.event(4, { kind: "scanning", value: false });
    expect(f.calls.map((c) => c.method)).toEqual(["state.snapshot"]);
    // The newer event is applied meanwhile
    expect(core.state.snap?.scanning).toBe(false);
    // Only one request while it is on its way
    f.event(6, { kind: "scanning", value: true });
    expect(f.calls.filter((c) => c.method === "state.snapshot")).toHaveLength(1);
    // The answer was taken before events 4 and 6 (it says scanning: false
    // and has the lost event 3's devices): those events go on top again
    const s = sampleSnapshot();
    s.scanning = false;
    s.devices = [];
    f.answerSnapshot(s);
    await tick();
    expect(core.state.snap?.scanning).toBe(true);
    expect(core.state.snap?.devices).toEqual([]);
    expect(core.state.rev).toBe(6);
    // Older events after it are ignored, the next one applies
    f.event(5, { kind: "scanning", value: false });
    expect(core.state.snap?.scanning).toBe(true);
    f.event(7, { kind: "scanning", value: false });
    expect(core.state.snap?.scanning).toBe(false);
    expect(f.calls.filter((c) => c.method === "state.snapshot")).toHaveLength(1);
  });

  it("prefers the core's own snapshot to a requested one", async () => {
    const f = fakeTransport();
    const core = createRoot(() => createCore(f.transport));
    await tick();
    f.status({ state: "connected", server: "core" });
    await tick();
    expect(f.calls[0].method).toBe("state.snapshot");
    const own = sampleSnapshot();
    own.scanning = true;
    f.send({ type: "snapshot", rev: 10, state: own });
    f.answerSnapshot(sampleSnapshot());
    await tick();
    expect(core.state.snap?.scanning).toBe(true);
    expect(core.state.rev).toBe(10);
  });

  it("keeps link samples and log lines without advancing past state events", async () => {
    const f = fakeTransport();
    const core = createRoot(() => createCore(f.transport));
    await tick();
    f.send({ type: "snapshot", rev: 5, state: sampleSnapshot() });
    // Subscription data may carry the current revision...
    f.event(5, { kind: "log", value: { time_ms: 1, level: "info", target: "t", message: "stream: device AA codec Ldac" } });
    f.event(5, {
      kind: "link",
      value: {
        time_ms: 2,
        rssi: -3,
        afh_in_use: 79,
        afh_usable: 79,
        afh_avoided: "",
        bytes_per_second: 1000,
        packets_per_second: 10,
        queue_depth: 1,
        dropped: 0,
        capture_dropped: 0,
        airtime: 0.3,
      },
    });
    expect(core.logs()).toHaveLength(1);
    expect(core.link()).toHaveLength(1);
    expect(core.flow()?.steps.start.status).toBe("done");
    // ...and the next state event is still in order
    f.event(6, { kind: "scanning", value: true });
    expect(f.calls.filter((c) => c.method === "state.snapshot")).toHaveLength(0);
  });

  it("re-requests the snapshot when the connection comes up, and merges subscriptions", async () => {
    const f = fakeTransport();
    const core = createRoot(() => createCore(f.transport));
    await tick();
    const releaseLink = core.wantLink();
    const releaseLogs = core.wantLogs("info");
    const releaseDebug = core.wantLogs("debug");
    await tick();
    // Not connected yet: nothing sent
    expect(f.calls).toHaveLength(0);
    f.status({ state: "connected", server: "core" });
    await tick();
    expect(f.calls.map((c) => c.method)).toEqual(["state.snapshot", "diag.subscribe"]);
    expect(f.calls[1].params).toEqual({ logs: "debug", link: true });
    releaseDebug();
    releaseLink();
    await tick();
    expect(f.calls[2]).toEqual({ method: "diag.subscribe", params: { logs: "info", link: false } });
    releaseLogs();
    await tick();
    expect(f.calls[3]).toEqual({ method: "diag.subscribe", params: { logs: null, link: false } });
  });

  it("turns request failures into IpcError", async () => {
    const f = fakeTransport();
    f.transport.call = async () => {
      throw { code: "adapter.driver_missing", message: { key: "error.btstack_init", args: [] } };
    };
    const core = createRoot(() => createCore(f.transport));
    await expect(core.request("stream.start", { profile: "x" })).rejects.toMatchObject({
      code: "adapter.driver_missing",
      msg: { key: "error.btstack_init" },
    });
  });
});
