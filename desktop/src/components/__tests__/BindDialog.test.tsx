/// <reference types="vitest" />
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, render, screen, waitFor } from "@testing-library/react";
import React from "react";
import { BindDialog } from "../BindDialog";
import type { AgentState, AgentStatus } from "../../hooks/useAgent";

// Mock QRCode
vi.mock("qrcode", () => ({
  default: {
    toDataURL: vi.fn().mockResolvedValue("data:image/png;base64,mockqr"),
  },
}));

function mockAgentState(overrides: Partial<AgentState> = {}): AgentState {
  return {
    agentStatus: null,
    sessions: [],
    error: null,
    isPolling: false,
    isRunning: false,
    isBound: false,
    isBinding: false,
    isConnected: false,
    statusIcon: "offline",
    bindDevice: vi.fn(),
    cancelBind: vi.fn(),
    redeemCode: vi.fn(),
    fetchStatus: vi.fn(),
    fetchSessions: vi.fn(),
    pausePolling: vi.fn(),
    resumePolling: vi.fn(),
    tokenRevoked: false,
    clearTokenRevoked: vi.fn(),
    ...overrides,
  } as unknown as AgentState;
}

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("BindDialog", () => {
  it("renders binding spinner initially", async () => {
    const bindDevice = vi.fn().mockImplementation(() => new Promise(() => {}));
    const agent = mockAgentState({ bindDevice });

    render(<BindDialog onClose={vi.fn()} agent={agent} />);

    await waitFor(() => {
      expect(screen.getByText("正在获取绑定码...")).toBeTruthy();
      expect(screen.getByText("正在与服务器建立连接")).toBeTruthy();
    });
  });

  it("shows QR code and bind code in polling phase", async () => {
    const bindDevice = vi.fn().mockResolvedValue({
      ok: true,
      bindCode: "ABC123",
      expiresIn: 120,
      confirmUrl: "https://shark.kim/bind",
    });
    const agent = mockAgentState({ bindDevice, fetchStatus: vi.fn() });

    render(<BindDialog onClose={vi.fn()} agent={agent} />);

    await waitFor(
      () => {
        expect(screen.getByText("请用 KN App 扫码绑定")).toBeTruthy();
        expect(screen.getByText(/二维码有效期/)).toBeTruthy();
      },
      { timeout: 3000 },
    );
  });

  it("transitions to success when agentStatus becomes connected", async () => {
    const bindDevice = vi.fn().mockResolvedValue({
      ok: true,
      bindCode: "ABC123",
      expiresIn: 120,
      confirmUrl: "https://shark.kim/bind",
    });

    const connectedStatus: AgentStatus = { state: "connected", crash_count: 0, safe_mode: false };
    const baseAgent = mockAgentState({ bindDevice, fetchStatus: vi.fn() });

    const { rerender } = render(<BindDialog onClose={vi.fn()} agent={baseAgent} />);

    // Wait for polling phase
    await waitFor(() => {
      expect(screen.getByText("请用 KN App 扫码绑定")).toBeTruthy();
    }, { timeout: 3000 });

    // Rerender with connected agent
    rerender(
      <BindDialog
        onClose={vi.fn()}
        agent={mockAgentState({
          bindDevice: vi.fn(),
          fetchStatus: vi.fn(),
          agentStatus: connectedStatus,
          isRunning: true,
          isBound: true,
          isConnected: true,
          statusIcon: "connected",
        })}
      />,
    );

    await waitFor(() => {
      expect(screen.getByText("绑定成功")).toBeTruthy();
    }, { timeout: 3000 });
  });

  it("shows error on bind failure", async () => {
    const bindDevice = vi.fn().mockResolvedValue({
      ok: false,
      error: "网络连接失败",
    });
    const agent = mockAgentState({ bindDevice });

    render(<BindDialog onClose={vi.fn()} agent={agent} />);

    await waitFor(() => {
      expect(screen.getByText("绑定失败")).toBeTruthy();
      expect(screen.getByText("网络连接失败")).toBeTruthy();
    }, { timeout: 3000 });
  });

  it("shows close button in error phase", async () => {
    const bindDevice = vi.fn().mockResolvedValue({
      ok: false,
      error: "连接超时",
    });
    const agent = mockAgentState({ bindDevice });

    render(<BindDialog onClose={vi.fn()} agent={agent} />);

    await waitFor(() => {
      expect(screen.getByText("关闭")).toBeTruthy();
    }, { timeout: 3000 });
  });

  it("cleans up interval on unmount during polling", async () => {
    const bindDevice = vi.fn().mockResolvedValue({
      ok: true,
      bindCode: "ABC123",
      expiresIn: 120,
      confirmUrl: "https://shark.kim/bind",
    });
    const agent = mockAgentState({ bindDevice, fetchStatus: vi.fn() });

    const { unmount } = render(<BindDialog onClose={vi.fn()} agent={agent} />);

    await waitFor(() => {
      expect(screen.getByText("请用 KN App 扫码绑定")).toBeTruthy();
    }, { timeout: 3000 });

    // Unmount should not throw
    unmount();
  });

  it("shows spinner before QR code is generated", async () => {
    // Make QRCode.toDataURL never resolve — spinner stays visible
    vi.doMock("qrcode", () => ({
      default: {
        toDataURL: vi.fn().mockImplementation(() => new Promise(() => {})),
      },
    }));

    const bindDevice = vi.fn().mockResolvedValue({
      ok: true,
      bindCode: "SLOW01",
      expiresIn: 120,
      confirmUrl: "https://shark.kim/bind",
    });
    const agent = mockAgentState({ bindDevice, fetchStatus: vi.fn() });

    render(<BindDialog onClose={vi.fn()} agent={agent} />);

    await waitFor(() => {
      expect(screen.getByText("请用 KN App 扫码绑定")).toBeTruthy();
      expect(screen.getByText(/二维码有效期/)).toBeTruthy();
      // No QR image shown while toDataURL is pending
      expect(screen.queryByAltText("绑定二维码")).toBeNull();
    }, { timeout: 3000 });
  });
});

describe("BindDialog with fake timers", () => {
  beforeEach(() => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
  });

  // Helper to flush all pending async effects
  async function settle() {
    await act(async () => {
      // vi.runAllTimers would run the 120s timeout, so we use small steps
      vi.advanceTimersByTime(100);
    });
  }

  it("calls fetchStatus periodically during polling", async () => {
    const bindDevice = vi.fn().mockResolvedValue({
      ok: true,
      bindCode: "ABC123",
      expiresIn: 120,
      confirmUrl: "https://shark.kim/bind",
    });
    const fetchStatus = vi.fn();
    const agent = mockAgentState({ bindDevice, fetchStatus });

    render(<BindDialog onClose={vi.fn()} agent={agent} />);
    await settle();

    await waitFor(() => {
      expect(screen.getByText("请用 KN App 扫码绑定")).toBeTruthy();
    });

    // Recursive setTimeout: first fetchStatus fires immediately when polling starts,
    // then after 2s the recursive schedule fires again.
    // settle() already consumed some time; advance 2s to trigger second call.
    await act(async () => {
      vi.advanceTimersByTime(2000);
    });
    expect(fetchStatus).toHaveBeenCalledTimes(2);

    await act(async () => {
      vi.advanceTimersByTime(2000);
    });
    expect(fetchStatus).toHaveBeenCalledTimes(3);
  });

  it("auto-closes after success delay", async () => {
    const onClose = vi.fn();
    const bindDevice = vi.fn().mockResolvedValue({
      ok: true,
      bindCode: "ABC123",
      expiresIn: 120,
      confirmUrl: "https://shark.kim/bind",
    });
    const connectedStatus: AgentStatus = { state: "connected", crash_count: 0, safe_mode: false };

    const { rerender } = render(
      <BindDialog onClose={onClose} agent={mockAgentState({ bindDevice, fetchStatus: vi.fn() })} />,
    );
    await settle();

    await waitFor(() => {
      expect(screen.getByText("请用 KN App 扫码绑定")).toBeTruthy();
    });

    // Rerender with connected state
    rerender(
      <BindDialog
        onClose={onClose}
        agent={mockAgentState({
          bindDevice: vi.fn(),
          fetchStatus: vi.fn(),
          agentStatus: connectedStatus,
          isRunning: true,
          isBound: true,
          isConnected: true,
          statusIcon: "connected",
        })}
      />,
    );

    await waitFor(() => {
      expect(screen.getByText("绑定成功")).toBeTruthy();
    });

    // Advance past 1500ms auto-close
    await act(async () => {
      vi.advanceTimersByTime(1600);
    });

    expect(onClose).toHaveBeenCalled();
  });

  it("shows timeout after 300 seconds with no status change", async () => {
    const bindDevice = vi.fn().mockResolvedValue({
      ok: true,
      bindCode: "ABC123",
      expiresIn: 300,
      confirmUrl: "https://shark.kim/bind",
    });
    const agent = mockAgentState({ bindDevice, fetchStatus: vi.fn(), cancelBind: vi.fn() });

    render(<BindDialog onClose={vi.fn()} agent={agent} />);
    await settle();

    await waitFor(() => {
      expect(screen.getByText("请用 KN App 扫码绑定")).toBeTruthy();
    });

    // Advance past expiresIn (300s) + grace (10s) = 310s
    await act(async () => {
      vi.advanceTimersByTime(311_000);
    });

    await waitFor(() => {
      expect(screen.getByText("绑定超时")).toBeTruthy();
    });
  });
});
