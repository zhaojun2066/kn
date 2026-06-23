/// <reference types="vitest" />
import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useAgent } from "../useAgent";
import type { AgentStatus, AgentSession } from "../useAgent";

// Mock Tauri invoke
const mockInvoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

function mockStatus(overrides: Partial<AgentStatus> = {}): AgentStatus {
  return {
    state: "unbound",
    crash_count: 0,
    safe_mode: false,
    ...overrides,
  };
}

function mockSessions(): { sessions: AgentSession[] } {
  return {
    sessions: [
      {
        nid: "s_test123",
        tool: "claude",
        profile: "work",
        cwd: "/tmp",
        created_at: "2025-01-01T00:00:00Z",
        status: "running",
      },
    ],
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  // Default: agent not running
  mockInvoke.mockRejectedValue(new Error("Agent not running"));
});

describe("useAgent", () => {
  // ── Initial state ──────────────────────────────────────────

  it("initial state: agentStatus is null before any invoke", () => {
    const { result } = renderHook(() => useAgent());

    expect(result.current.agentStatus).toBeNull();
    expect(result.current.isRunning).toBe(false);
    expect(result.current.isBound).toBe(false);
    expect(result.current.isBinding).toBe(false);
    expect(result.current.isConnected).toBe(false);
    expect(result.current.statusIcon).toBe("offline");
  });

  // ── fetchStatus ────────────────────────────────────────────

  it("fetchStatus success updates agentStatus and derived booleans", async () => {
    mockInvoke.mockResolvedValue(mockStatus({ state: "connected" }));

    const { result } = renderHook(() => useAgent());

    await act(async () => {
      await result.current.fetchStatus();
    });

    expect(result.current.agentStatus).toEqual(
      expect.objectContaining({ state: "connected" }),
    );
    expect(result.current.isRunning).toBe(true);
    expect(result.current.isBound).toBe(true);
    expect(result.current.isConnected).toBe(true);
    expect(result.current.statusIcon).toBe("connected");
  });

  it("fetchStatus failure keeps agentStatus null, isRunning false", async () => {
    mockInvoke.mockRejectedValue(new Error("connection refused"));

    const { result } = renderHook(() => useAgent());

    await act(async () => {
      await result.current.fetchStatus();
    });

    expect(result.current.agentStatus).toBeNull();
    expect(result.current.isRunning).toBe(false);
    expect(result.current.statusIcon).toBe("offline");
  });

  // ── fetchSessions ──────────────────────────────────────────

  it("fetchSessions populates sessions array", async () => {
    mockInvoke.mockImplementation((_cmd: string, args: { method: string }) => {
      if (args.method === "status") return Promise.resolve(mockStatus({ state: "connected" }));
      if (args.method === "sessions") return Promise.resolve(mockSessions());
      return Promise.reject(new Error("unknown"));
    });

    const { result } = renderHook(() => useAgent());

    // Wait for initial poll to settle (it calls fetchStatus + fetchSessions in the useEffect)
    await act(async () => {
      await new Promise((r) => setTimeout(r, 0));
    });

    expect(result.current.sessions).toHaveLength(1);
    expect(result.current.sessions[0].tool).toBe("claude");
  });

  // ── bindDevice ─────────────────────────────────────────────

  it("bindDevice success returns ok with bind metadata", async () => {
    mockInvoke.mockResolvedValue({
      status: "binding_started",
      bindCode: "A1B2C3",
      expiresIn: 120,
      confirmUrl: "https://shark.kim/bind",
    });

    const { result } = renderHook(() => useAgent());
    let bindResult: Awaited<ReturnType<typeof result.current.bindDevice>> = {
      ok: false,
    };

    await act(async () => {
      bindResult = await result.current.bindDevice();
    });

    expect(bindResult.ok).toBe(true);
    expect(bindResult.bindCode).toBe("A1B2C3");
    expect(bindResult.expiresIn).toBe(120);
    expect(bindResult.confirmUrl).toBe("https://shark.kim/bind");
  });

  it("bindDevice failure returns ok:false with error string", async () => {
    mockInvoke.mockRejectedValue(new Error("网络错误"));

    const { result } = renderHook(() => useAgent());
    let bindResult: Awaited<ReturnType<typeof result.current.bindDevice>> = {
      ok: false,
    };

    await act(async () => {
      bindResult = await result.current.bindDevice();
    });

    expect(bindResult.ok).toBe(false);
    expect(bindResult.error).toContain("网络错误");
  });

  // ── redeemCode ─────────────────────────────────────────────

  it("redeemCode success returns plan and days", async () => {
    mockInvoke.mockResolvedValue({
      status: "redeemed",
      plan: "pro",
      days: 90,
    });

    const { result } = renderHook(() => useAgent());
    let redeemResult: Awaited<ReturnType<typeof result.current.redeemCode>> = {
      ok: false,
    };

    await act(async () => {
      redeemResult = await result.current.redeemCode("KN-TEST-CODE");
    });

    expect(redeemResult.ok).toBe(true);
    expect(redeemResult.plan).toBe("pro");
    expect(redeemResult.days).toBe(90);
  });

  it("redeemCode failure returns ok:false with error string", async () => {
    mockInvoke.mockRejectedValue(new Error("CODE_NOT_FOUND"));

    const { result } = renderHook(() => useAgent());
    let redeemResult: Awaited<ReturnType<typeof result.current.redeemCode>> = {
      ok: false,
    };

    await act(async () => {
      redeemResult = await result.current.redeemCode("KN-BAD-CODE");
    });

    expect(redeemResult.ok).toBe(false);
    expect(redeemResult.error).toContain("CODE_NOT_FOUND");
  });

  // ── isBound state mapping ──────────────────────────────────

  it.each([
    ["connected", true],
    ["idle", true],
    ["running", true],
    ["reconnecting", true],
    ["unbound", false],
    ["starting", false],
    ["stopped", false],
  ] as const)("isBound is %s for state %s", async (state, expected) => {
    mockInvoke.mockResolvedValue(mockStatus({ state }));

    const { result } = renderHook(() => useAgent());

    await act(async () => {
      await result.current.fetchStatus();
    });

    expect(result.current.isBound).toBe(expected);
  });

  // ── isBinding state ─────────────────────────────────────────

  it("isBinding is true when state is 'binding'", async () => {
    mockInvoke.mockResolvedValue(mockStatus({ state: "binding" }));

    const { result } = renderHook(() => useAgent());

    await act(async () => {
      await result.current.fetchStatus();
    });

    expect(result.current.isBinding).toBe(true);
    expect(result.current.isBound).toBe(false);
  });

  it("isBinding is false when state is 'connected'", async () => {
    mockInvoke.mockResolvedValue(mockStatus({ state: "connected" }));

    const { result } = renderHook(() => useAgent());

    await act(async () => {
      await result.current.fetchStatus();
    });

    expect(result.current.isBinding).toBe(false);
  });

  // ── cancelBind ──────────────────────────────────────────────

  it("cancelBind calls agent_ipc with cancel_bind method", async () => {
    mockInvoke.mockResolvedValue({ status: "cancelled" });

    const { result } = renderHook(() => useAgent());

    await act(async () => {
      await result.current.cancelBind();
    });

    expect(mockInvoke).toHaveBeenCalledWith("agent_ipc", { method: "cancel_bind" });
  });

  it("cancelBind does not throw on failure", async () => {
    mockInvoke.mockRejectedValue(new Error("Agent not running"));

    const { result } = renderHook(() => useAgent());

    // Should not throw
    await act(async () => {
      await result.current.cancelBind();
    });
  });

  // ── statusIcon mapping ─────────────────────────────────────

  it.each([
    ["unbound", "unbound"],
    ["binding", "binding"],
    ["connected", "connected"],
    ["idle", "connected"],
    ["running", "connected"],
    ["reconnecting", "reconnecting"],
    ["starting", "starting"],
    ["stopped", "starting"], // isRunning=true but not matching specific states → "starting"
  ] as const)("statusIcon is '%s' when state is '%s'", async (state, expectedIcon) => {
    mockInvoke.mockResolvedValue(mockStatus({ state }));

    const { result } = renderHook(() => useAgent());

    await act(async () => {
      await result.current.fetchStatus();
    });

    expect(result.current.statusIcon).toBe(expectedIcon);
  });

  it("statusIcon is 'offline' when agent is not running", () => {
    const { result } = renderHook(() => useAgent());

    // agentStatus is null (agent not running)
    expect(result.current.isRunning).toBe(false);
    expect(result.current.statusIcon).toBe("offline");
  });
});
