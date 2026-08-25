/// <reference types="vitest" />
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));

import { flushRelayExitOutbox, queuedRelayExits, submitRelayExit } from "../relayExitOutbox";

describe("relay exit outbox", () => {
  beforeEach(() => {
    localStorage.clear();
    mocks.invoke.mockReset();
  });

  it("persists a failed relay exit and removes it after a later successful retry", async () => {
    mocks.invoke.mockRejectedValueOnce(new Error("Agent restarting"));
    await submitRelayExit({ nid: "s_relay", reason: "user_closed_tab" });

    expect(queuedRelayExits()).toEqual([{ nid: "s_relay", reason: "user_closed_tab" }]);

    mocks.invoke.mockResolvedValueOnce({ ok: true });
    await flushRelayExitOutbox();

    expect(mocks.invoke).toHaveBeenLastCalledWith("agent_ipc", {
      method: "relay_exit",
      params: { nid: "s_relay", reason: "user_closed_tab" },
    });
    expect(queuedRelayExits()).toEqual([]);
  });

  it("shares one in-flight flush across the right and bottom terminal instances", async () => {
    mocks.invoke.mockRejectedValueOnce(new Error("Agent restarting"));
    await submitRelayExit({ nid: "s_relay", reason: "user_closed_tab" });

    let resolveInvoke: (() => void) | undefined;
    mocks.invoke.mockImplementationOnce(() => new Promise<void>((resolve) => { resolveInvoke = resolve; }));
    const first = flushRelayExitOutbox();
    const second = flushRelayExitOutbox();

    expect(second).toBe(first);
    expect(mocks.invoke).toHaveBeenCalledTimes(2);
    resolveInvoke?.();
    await first;
  });
});
