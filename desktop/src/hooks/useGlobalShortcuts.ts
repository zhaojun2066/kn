import { useEffect } from "react";
import { formatShortcut } from "../utils/shortcut";

interface GlobalShortcutOptions {
  selectedName: string | null;
  onDeselect: () => void;
  onToggleBottomTerminal: () => void;
  showAddDialog: boolean;
  showDeleteConfirm: boolean;
  showNameDialog: boolean;
  showQuickSwitcher: boolean;
  profileDrawerOpen: boolean;
  resourceDrawerOpen: boolean;
  setShowAddDialog: (show: boolean) => void;
  setShowDeleteConfirm: (show: boolean) => void;
  setShowNameDialog: (show: boolean) => void;
  setShowQuickSwitcher: (show: boolean) => void;
  setShowShortcuts: (updater: (show: boolean) => boolean) => void;
  setQuickSwitcherMode: (mode: "profile" | "history") => void;
  setSidebarVisible: (updater: (visible: boolean) => boolean) => void;
  setRightMaximized: (updater: (maximized: boolean) => boolean) => void;
  setBottomMaximized: (updater: (maximized: boolean) => boolean) => void;
  setProfileDrawerOpen: (open: boolean) => void;
  setResourceDrawerOpen: (open: boolean) => void;
  addToast: (type: "error" | "success", message: string) => void;
}

function isTextInput(target: EventTarget | null) {
  return target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement;
}

export function useGlobalShortcuts({
  selectedName,
  onDeselect,
  onToggleBottomTerminal,
  showAddDialog,
  showDeleteConfirm,
  showNameDialog,
  showQuickSwitcher,
  profileDrawerOpen,
  resourceDrawerOpen,
  setShowAddDialog,
  setShowDeleteConfirm,
  setShowNameDialog,
  setShowQuickSwitcher,
  setShowShortcuts,
  setQuickSwitcherMode,
  setSidebarVisible,
  setRightMaximized,
  setBottomMaximized,
  setProfileDrawerOpen,
  setResourceDrawerOpen,
  addToast,
}: GlobalShortcutOptions) {
  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && !event.shiftKey && (event.key === "p" || event.key === "P")) {
        event.preventDefault();
        setQuickSwitcherMode("profile");
        setShowQuickSwitcher(true);
        return;
      }
      if ((event.metaKey || event.ctrlKey) && event.shiftKey && (event.key === "p" || event.key === "P")) {
        event.preventDefault();
        setQuickSwitcherMode("history");
        setShowQuickSwitcher(true);
        return;
      }
      if ((event.metaKey || event.ctrlKey) && event.key === "k") {
        event.preventDefault();
        setShowShortcuts((prev) => !prev);
      }
      if ((event.metaKey || event.ctrlKey) && event.key === "n") {
        event.preventDefault();
        setShowAddDialog(true);
      }
      if (event.key === "Escape") {
        if (showQuickSwitcher) setShowQuickSwitcher(false);
        else if (showAddDialog) setShowAddDialog(false);
        else if (showDeleteConfirm) setShowDeleteConfirm(false);
        else if (showNameDialog) setShowNameDialog(false);
        else if (profileDrawerOpen) setProfileDrawerOpen(false);
        else if (resourceDrawerOpen) setResourceDrawerOpen(false);
        else if (selectedName) onDeselect();
      }
      if (event.key === "Backspace" && selectedName && !isTextInput(event.target)) {
        setShowDeleteConfirm(true);
      }
      if (event.ctrlKey && event.key === "`") {
        event.preventDefault();
        onToggleBottomTerminal();
      }
      if ((event.metaKey || event.ctrlKey) && !event.shiftKey && event.key === "b") {
        event.preventDefault();
        setSidebarVisible((visible) => !visible);
      }
      if ((event.metaKey || event.ctrlKey) && !event.shiftKey && event.key === "j") {
        event.preventDefault();
        onToggleBottomTerminal();
      }
      if ((event.metaKey || event.ctrlKey) && event.shiftKey && (event.key === "m" || event.key === "M")) {
        event.preventDefault();
        const el = document.activeElement as HTMLElement | null;
        const panel = el?.closest("[data-panel]") as HTMLElement | null;
        if (panel?.dataset.panel === "right") {
          setRightMaximized((value) => !value);
          setBottomMaximized(() => false);
        } else if (panel?.dataset.panel === "bottom") {
          setBottomMaximized((value) => !value);
          setRightMaximized(() => false);
        } else {
          addToast("success", `💡 请先点击终端面板，再使用 ${formatShortcut("mod+⇧M")} 最大化`);
        }
      }
      if ((event.metaKey || event.ctrlKey) && event.shiftKey && event.code === "KeyG") {
        event.preventDefault();
        setProfileDrawerOpen(!profileDrawerOpen);
      }
      if ((event.metaKey || event.ctrlKey) && event.shiftKey && event.code === "KeyY") {
        event.preventDefault();
        setResourceDrawerOpen(!resourceDrawerOpen);
      }
    };

    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [
    selectedName,
    onDeselect,
    onToggleBottomTerminal,
    showAddDialog,
    showDeleteConfirm,
    showNameDialog,
    showQuickSwitcher,
    profileDrawerOpen,
    resourceDrawerOpen,
    setShowAddDialog,
    setShowDeleteConfirm,
    setShowNameDialog,
    setShowQuickSwitcher,
    setShowShortcuts,
    setQuickSwitcherMode,
    setSidebarVisible,
    setRightMaximized,
    setBottomMaximized,
    setProfileDrawerOpen,
    setResourceDrawerOpen,
    addToast,
  ]);
}
