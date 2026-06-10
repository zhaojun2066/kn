import { useEffect, type MutableRefObject } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

interface TerminalCloseTarget {
  activeTabId: string | null;
  tabs: Array<{ id: string; rootNode: { type: string } }>;
  closePane: (tabId: string) => void;
}

function closeSplitPaneIfPresent(terminal: TerminalCloseTarget) {
  if (!terminal.activeTabId) return;
  const tab = terminal.tabs.find((item) => item.id === terminal.activeTabId);
  if (tab?.rootNode.type === "split") {
    terminal.closePane(terminal.activeTabId);
  }
}

export function useTerminalCloseGuard(
  rightRef: MutableRefObject<TerminalCloseTarget>,
  bottomRef: MutableRefObject<TerminalCloseTarget>,
) {
  useEffect(() => {
    let lastKey = "";
    const keyHandler = (event: KeyboardEvent) => {
      lastKey = event.key;
      setTimeout(() => { lastKey = ""; }, 200);
    };

    window.addEventListener("keydown", keyHandler, true);

    const currentWindow = getCurrentWindow();
    const unlisten = currentWindow.onCloseRequested((event) => {
      if (lastKey !== "w") return;

      const el = document.activeElement as HTMLElement | null;
      const panel = el?.closest("[data-panel]") as HTMLElement | null;
      if (!panel) return;

      event.preventDefault();
      lastKey = "";
      if (panel.dataset.panel === "right") {
        closeSplitPaneIfPresent(rightRef.current);
      } else if (panel.dataset.panel === "bottom") {
        closeSplitPaneIfPresent(bottomRef.current);
      }
    });

    return () => {
      window.removeEventListener("keydown", keyHandler, true);
      unlisten.then((fn) => fn());
    };
  }, [rightRef, bottomRef]);
}
