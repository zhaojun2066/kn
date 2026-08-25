import { invoke } from "@tauri-apps/api/core";

const STORAGE_KEY = "kn-relay-exit-outbox";
let flushInFlight: Promise<void> | null = null;

export interface RelayExitRequest {
  nid: string;
  reason: string;
}

function readOutbox(): RelayExitRequest[] {
  try {
    const value = JSON.parse(localStorage.getItem(STORAGE_KEY) || "[]");
    return Array.isArray(value) ? value.filter((item): item is RelayExitRequest =>
      typeof item?.nid === "string" && typeof item?.reason === "string",
    ) : [];
  } catch {
    return [];
  }
}

function writeOutbox(items: RelayExitRequest[]): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(items));
  } catch { /* Keep the best-effort IPC path working when storage is unavailable. */ }
}

export function queuedRelayExits(): RelayExitRequest[] {
  return readOutbox();
}

function enqueueRelayExit(request: RelayExitRequest): void {
  const items = readOutbox();
  if (items.some((item) => item.nid === request.nid)) return;
  writeOutbox([...items, request]);
}

function removeRelayExit(nid: string): void {
  writeOutbox(readOutbox().filter((item) => item.nid !== nid));
}

export async function submitRelayExit(request: RelayExitRequest): Promise<void> {
  try {
    await invoke("agent_ipc", { method: "relay_exit", params: request });
    removeRelayExit(request.nid);
  } catch {
    enqueueRelayExit(request);
  }
}

export function flushRelayExitOutbox(): Promise<void> {
  if (flushInFlight) return flushInFlight;

  const flush = async () => {
    for (const request of readOutbox()) {
      await submitRelayExit(request);
    }
  };
  flushInFlight = flush().finally(() => { flushInFlight = null; });
  return flushInFlight;
}
