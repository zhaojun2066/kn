/// <reference types="vitest" />
import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import React from "react";
import { RedeemDialog } from "../RedeemDialog";
import type { AgentState } from "../../hooks/useAgent";

// Generate a valid redeem code (min 50 chars, must start with "KN-")
const validCode = () => "KN-" + "X".repeat(47);

function mockAgentState(overrides: Partial<AgentState> = {}): AgentState {
  return {
    agentStatus: { state: "connected", crash_count: 0, safe_mode: false },
    sessions: [],
    error: null,
    isPolling: false,
    isRunning: true,
    isBound: true,
    isBinding: false,
    isConnected: true,
    statusIcon: "connected",
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

describe("RedeemDialog", () => {
  it("renders input field and confirm button initially", () => {
    render(<RedeemDialog onClose={vi.fn()} agent={mockAgentState()} />);

    expect(screen.getByPlaceholderText("KN-...")).toBeTruthy();
    expect(screen.getByText("确认兑换")).toBeTruthy();
  });

  it("button is disabled when input is empty", () => {
    render(<RedeemDialog onClose={vi.fn()} agent={mockAgentState()} />);

    const button = screen.getByText("确认兑换") as HTMLButtonElement;
    expect(button.disabled).toBe(true);
  });

  it("rejects invalid prefix — code must start with KN-", async () => {
    render(<RedeemDialog onClose={vi.fn()} agent={mockAgentState()} />);

    const input = screen.getByPlaceholderText("KN-...");
    await act(async () => {
      fireEvent.change(input, { target: { value: "ABC-1234-5678" } });
      fireEvent.click(screen.getByText("确认兑换"));
    });

    await waitFor(() => {
      expect(screen.getByText("卡密格式无效，应以 KN- 开头")).toBeTruthy();
    });
  });

  it("accepts valid KN- format and calls redeemCode", async () => {
    const redeemCode = vi.fn().mockResolvedValue({
      ok: true,
      plan: "pro",
      days: 90,
    });
    const validCode = "KN-" + "0".repeat(47); // minimum 50 chars
    render(<RedeemDialog onClose={vi.fn()} agent={mockAgentState({ redeemCode })} />);

    const input = screen.getByPlaceholderText("KN-...");
    await act(async () => {
      fireEvent.change(input, { target: { value: validCode } });
      fireEvent.click(screen.getByText("确认兑换"));
    });

    expect(redeemCode).toHaveBeenCalledWith(validCode);
  });

  it("rejects code that is too short", async () => {
    render(<RedeemDialog onClose={vi.fn()} agent={mockAgentState()} />);

    const input = screen.getByPlaceholderText("KN-...");
    await act(async () => {
      fireEvent.change(input, { target: { value: "KN-SHORT" } });
      fireEvent.click(screen.getByText("确认兑换"));
    });

    await waitFor(() => {
      expect(screen.getByText("卡密格式无效，长度不足")).toBeTruthy();
    });
  });

  it("shows spinner during redeeming phase", async () => {
    // redeemCode never resolves
    const redeemCode = vi.fn().mockImplementation(() => new Promise(() => {}));
    render(<RedeemDialog onClose={vi.fn()} agent={mockAgentState({ redeemCode })} />);

    const input = screen.getByPlaceholderText("KN-...");
    await act(async () => {
      fireEvent.change(input, { target: { value: "KN-" + "A".repeat(47) } });
      fireEvent.click(screen.getByText("确认兑换"));
    });

    // Should now show a spinner (Loader2 in redeeming phase)
    await waitFor(() => {
      // The button should no longer show "确认兑换" — it should be disabled or replaced
      expect(screen.queryByText("确认兑换")).toBeNull();
    });
  });

  it("shows success with plan and days", async () => {
    const redeemCode = vi.fn().mockResolvedValue({
      ok: true,
      plan: "premium",
      days: 365,
    });
    render(<RedeemDialog onClose={vi.fn()} agent={mockAgentState({ redeemCode })} />);

    const input = screen.getByPlaceholderText("KN-...");
    await act(async () => {
      fireEvent.change(input, { target: { value: "KN-" + "P".repeat(47) } });
      fireEvent.click(screen.getByText("确认兑换"));
    });

    // Wait for async redeem to resolve
    await act(async () => {
      await Promise.resolve();
    });

    await waitFor(() => {
      expect(screen.getByText("兑换成功！")).toBeTruthy();
      expect(screen.getByText("完成")).toBeTruthy();
    });
  });

  it("shows error on redeem failure with retry button", async () => {
    const redeemCode = vi.fn().mockResolvedValue({
      ok: false,
      error: "该卡密已被使用",
    });
    render(<RedeemDialog onClose={vi.fn()} agent={mockAgentState({ redeemCode })} />);

    const input = screen.getByPlaceholderText("KN-...");
    await act(async () => {
      fireEvent.change(input, { target: { value: validCode() } });
      fireEvent.click(screen.getByText("确认兑换"));
    });

    await act(async () => {
      await Promise.resolve();
    });

    await waitFor(() => {
      expect(screen.getByText("该卡密已被使用")).toBeTruthy();
      expect(screen.getByText("重试")).toBeTruthy();
      expect(screen.getByText("关闭")).toBeTruthy();
    });
  });

  it("retry resets back to input phase", async () => {
    const redeemCode = vi.fn().mockResolvedValue({
      ok: false,
      error: "兑换失败",
    });
    render(<RedeemDialog onClose={vi.fn()} agent={mockAgentState({ redeemCode })} />);

    const input = screen.getByPlaceholderText("KN-...");
    await act(async () => {
      fireEvent.change(input, { target: { value: validCode() } });
      fireEvent.click(screen.getByText("确认兑换"));
    });

    await act(async () => {
      await Promise.resolve();
    });

    await waitFor(() => {
      expect(screen.getByText("重试")).toBeTruthy();
    });

    // Click retry
    await act(async () => {
      fireEvent.click(screen.getByText("重试"));
    });

    // Should be back to input phase
    await waitFor(() => {
      expect(screen.getByText("确认兑换")).toBeTruthy();
    });
  });

  it("close button calls onClose from error phase", async () => {
    const onClose = vi.fn();
    const redeemCode = vi.fn().mockResolvedValue({
      ok: false,
      error: "失败",
    });
    render(<RedeemDialog onClose={onClose} agent={mockAgentState({ redeemCode })} />);

    const input = screen.getByPlaceholderText("KN-...");
    await act(async () => {
      fireEvent.change(input, { target: { value: validCode() } });
      fireEvent.click(screen.getByText("确认兑换"));
    });

    await act(async () => {
      await Promise.resolve();
    });

    await waitFor(() => {
      expect(screen.getByText("关闭")).toBeTruthy();
    });

    await act(async () => {
      fireEvent.click(screen.getByText("关闭"));
    });

    expect(onClose).toHaveBeenCalled();
  });

  it("enter key submits the code", async () => {
    const redeemCode = vi.fn().mockResolvedValue({
      ok: true,
      plan: "basic",
      days: 30,
    });
    render(<RedeemDialog onClose={vi.fn()} agent={mockAgentState({ redeemCode })} />);

    const input = screen.getByPlaceholderText("KN-...");
    await act(async () => {
      fireEvent.change(input, { target: { value: validCode() } });
      fireEvent.keyDown(input, { key: "Enter" });
    });

    expect(redeemCode).toHaveBeenCalledWith(validCode());
  });

  it("enter key with empty input shows error", async () => {
    render(<RedeemDialog onClose={vi.fn()} agent={mockAgentState()} />);

    const input = screen.getByPlaceholderText("KN-...");
    // Press Enter with empty input — this bypasses the disabled button
    await act(async () => {
      fireEvent.keyDown(input, { key: "Enter" });
    });

    await waitFor(() => {
      expect(screen.getByText("请输入卡密")).toBeTruthy();
    });
  });
});
