//! Update check + download + verify flow.
//! Extracted from App.tsx to keep the main component focused on layout glue.

import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export interface UpdateDialogState {
  version: string; notes: string; url: string; sha256: string; mandatory: boolean;
}

export interface DownloadState {
  phase: "idle" | "downloading" | "verifying"; progress: number; downloaded: number; total: number | null; speedBytesPerSecond: number | null; etaSeconds: number | null; error: string | null;
}

type AddToast = (type: "error" | "success", msg: string) => void;

export function useUpdateCheck(addToast: AddToast) {
  const [updateDialog, setUpdateDialog] = useState<UpdateDialogState | null>(null);
  const [downloadState, setDownloadState] = useState<DownloadState>({ phase: "idle", progress: 0, downloaded: 0, total: null, speedBytesPerSecond: null, etaSeconds: null, error: null });
  const activeDownload = useRef<{ path: string | null; cancelled: boolean } | null>(null);

  const handleCheckUpdate = useCallback(async (opts?: { silent?: boolean }) => {
    try {
      const release = await invoke<UpdateDialogState | null>("check_desktop_release");
      if (!release) {
        if (!opts?.silent) addToast("success", "已是最新版本");
        return;
      }
      setUpdateDialog(release);
    } catch (e) {
      if (!opts?.silent) addToast("error", `检查更新失败: ${e}`);
    }
  }, [addToast]);

  const handleConfirmUpdate = useCallback(async () => {
    if (!updateDialog) return;
    const { version, url, sha256 } = updateDialog;
    const control = { path: null as string | null, cancelled: false };
    activeDownload.current = control;
    const startedAt = performance.now();
    setDownloadState({ phase: "downloading", progress: 0, downloaded: 0, total: null, speedBytesPerSecond: null, etaSeconds: null, error: null });
    const unlisten = await listen<{ downloaded: number; total: number | null; percent: number }>("download-progress", (event) => {
      const elapsedSeconds = Math.max((performance.now() - startedAt) / 1000, 0.001);
      const speedBytesPerSecond = event.payload.downloaded / elapsedSeconds;
      const etaSeconds = event.payload.total && speedBytesPerSecond > 0
        ? Math.max(0, (event.payload.total - event.payload.downloaded) / speedBytesPerSecond)
        : null;
      setDownloadState((prev) => prev.phase === "downloading" ? { ...prev, progress: event.payload.percent, downloaded: event.payload.downloaded, total: event.payload.total, speedBytesPerSecond, etaSeconds } : prev);
    });
    let tmpPath = "";
    try {
      const tmpDir: string = await invoke("temp_dir");
      const pathPart = url.split('?')[0];
      const ext = pathPart.split('.').pop() || "dmg";
      tmpPath = `${tmpDir}/kn-update-${Date.now()}.${ext}`;
      control.path = tmpPath;
      if (control.cancelled) throw new Error("下载已取消");
      await invoke("download_file", { url, path: tmpPath });
      setDownloadState((prev) => ({ ...prev, phase: "verifying", progress: 100, error: null }));
      if (sha256) {
        const ok = (await invoke("verify_sha256", { path: tmpPath, expected: sha256 })) as boolean;
        if (!ok) throw new Error("SHA256 校验失败，文件可能损坏");
      }
      setDownloadState((prev) => ({ ...prev, phase: "idle", progress: 100, error: null }));
      await new Promise((r) => setTimeout(r, 800));
      if (control.cancelled) throw new Error("下载已取消");
      setUpdateDialog(null);
      setDownloadState({ phase: "idle", progress: 0, downloaded: 0, total: null, speedBytesPerSecond: null, etaSeconds: null, error: null });
      addToast("success", `已下载 ${version}，正在打开安装包...`);
      await invoke("open_file", { path: tmpPath });
    } catch (e: unknown) {
      if (tmpPath) await invoke("delete_download_file", { path: tmpPath }).catch(() => undefined);
      if (activeDownload.current === control) {
        setDownloadState({ phase: "idle", progress: 0, downloaded: 0, total: null, speedBytesPerSecond: null, etaSeconds: null, error: String(e) });
      }
    }
    if (activeDownload.current === control) activeDownload.current = null;
    unlisten();
  }, [updateDialog, addToast]);

  const handleCancelUpdate = useCallback(async () => {
    const control = activeDownload.current;
    if (!control) {
      setUpdateDialog(null);
      return;
    }
    control.cancelled = true;
    const path = control.path;
    if (path) await invoke("cancel_download", { path }).catch(() => undefined);
    setUpdateDialog(null);
    setDownloadState({ phase: "idle", progress: 0, downloaded: 0, total: null, speedBytesPerSecond: null, etaSeconds: null, error: null });
  }, []);

  // Auto-check on startup (silent) — intentionally runs once; handleCheckUpdate is stable
  useEffect(() => { handleCheckUpdate({ silent: true }); // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return { updateDialog, downloadState, handleCheckUpdate, handleConfirmUpdate, handleCancelUpdate, setUpdateDialog, setDownloadState };
}
