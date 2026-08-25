/// <reference types="vitest" />
import { fireEvent, render, screen } from "@testing-library/react";
import React from "react";
import { describe, expect, it, vi } from "vitest";
import { SessionPanel } from "../SessionPanel";
import type { AgentSession, AgentState } from "../../hooks/useAgent";

const remoteSession: AgentSession = {
  nid: "s_remote",
  kind: "Native",
  source: "ios",
  tool: "codex",
  profile: "default",
  cwd: "/repo",
  created_at: "2026-08-25T00:00:00Z",
  status: "running",
  remote_enabled: true,
};

function agentWith(session: AgentSession): AgentState {
  return {
    sessions: [session],
    agentStatus: null,
    fetchSessions: vi.fn(),
  } as unknown as AgentState;
}

describe("SessionPanel", () => {
  it("does not offer Agent PTY takeover for a desktop-owned Relay session", () => {
    render(<SessionPanel agent={agentWith({ ...remoteSession, kind: "Relay" })} initialTab="remote" onClose={vi.fn()} onOpenRemoteSession={vi.fn()} canOpenLocalRelaySession={() => false} />);

    expect(screen.queryByTitle("打开远程会话")).toBeNull();
  });

  it("keeps Agent PTY takeover available for a Native remote session", () => {
    const onOpenRemoteSession = vi.fn();
    render(<SessionPanel agent={agentWith(remoteSession)} initialTab="remote" onClose={vi.fn()} onOpenRemoteSession={onOpenRemoteSession} canOpenLocalRelaySession={() => false} />);

    fireEvent.click(screen.getByTitle("打开远程会话"));
    expect(onOpenRemoteSession).toHaveBeenCalledWith(remoteSession);
  });

  it("offers local terminal return only when the Relay PTY is still running", () => {
    const relay = { ...remoteSession, kind: "Relay" as const };
    const onOpenRemoteSession = vi.fn();
    render(<SessionPanel agent={agentWith(relay)} initialTab="remote" onClose={vi.fn()} onOpenRemoteSession={onOpenRemoteSession} canOpenLocalRelaySession={() => true} />);

    fireEvent.click(screen.getByTitle("打开远程会话"));
    expect(onOpenRemoteSession).toHaveBeenCalledWith(relay);
  });
});
