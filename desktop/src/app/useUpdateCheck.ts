//! Update check + download + verify flow.
//! Extracted from App.tsx to keep the main component focused on layout glue.

import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/** Compare two semver strings. */
function compareVersions(a: string, b: string): number {
  const pa = a.split(".").map(Number);
  const pb = b.split(".").map(Number);
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const na = pa[i] ?? 0, nb = pb[i] ?? 0;
    if (isNaN(na) || isNaN(nb)) return a.localeCompare(b);
    if (na !== nb) return na - nb;
  }
  return 0;
}

interface UpdatePlatform { url: string; sha256?: string; }
interface UpdateManifest { version: string; notes?: string; platforms: Record<string, UpdatePlatform>; }

export interface UpdateDialogState {
  version: string; notes: string; url: string; sha256: string;
}

export interface DownloadState {
  phase: "idle" | "downloading" | "verifying"; progress: number; error: string | null;
}

type AddToast = (type: "error" | "success", msg: string) => void;

export function useUpdateCheck(addToast: AddToast) {
  const [updateDialog, setUpdateDialog] = useState<UpdateDialogState | null>(null);
  const [downloadState, setDownloadState] = useState<DownloadState>({ phase: "idle", progress: 0, error: null });

  const handleCheckUpdate = useCallback(async (opts?: { silent?: boolean }) => {
    try {
      const config: { update_url?: string } = await invoke("read_app_config");
      if (!config.update_url) {
        if (!opts?.silent) addToast("error", "未配置更新地址。请编辑 update/update.json");
        return;
      }
      const currentVersion: string = await invoke("get_app_version");
      let manifest: UpdateManifest;
      try {
        const text = (await invoke("fetch_url", { url: config.update_url })) as string;
        if (!text.trim()) throw new Error("空响应");
        manifest = JSON.parse(text) as UpdateManifest;
      } catch (e) {
        if (!opts?.silent) addToast("error", `无法获取更新清单: ${e}\n${config.update_url}`);
        return;
      }
      if (!manifest.version || !manifest.platforms) {
        if (!opts?.silent) addToast("error", "更新清单格式无效");
        return;
      }
      if (compareVersions(manifest.version, currentVersion) <= 0) {
        if (!opts?.silent) addToast("success", `已是最新版本 (${currentVersion})`);
        return;
      }
      const platformInfo: { os: string; arch: string } = await invoke("get_platform_info");
      const platform = `darwin-${platformInfo.arch}`;
      const plat = manifest.platforms[platform] || Object.values(manifest.platforms)[0];
      if (!plat?.url) { addToast("error", `无此平台的更新包 (${platform})`); return; }
      setUpdateDialog({ version: manifest.version, notes: manifest.notes || "", url: plat.url, sha256: plat.sha256 || "" });
    } catch (e) {
      if (!opts?.silent) addToast("error", `检查更新失败: ${e}`);
    }
  }, [addToast]);

  const handleConfirmUpdate = useCallback(async () => {
    if (!updateDialog) return;
    const { version, url, sha256 } = updateDialog;
    setDownloadState({ phase: "downloading", progress: 0, error: null });
    const unlisten = await listen<number>("download-progress", (event) => {
      setDownloadState((prev) => prev.phase === "downloading" ? { ...prev, progress: event.payload } : prev);
    });
    try {
      const tmpDir: string = await invoke("temp_dir");
      const pathPart = url.split('?')[0];
      const ext = pathPart.split('.').pop() || "dmg";
      const tmpPath = `${tmpDir}/kn-update-${Date.now()}.${ext}`;
      await invoke("download_file", { url, path: tmpPath });
      setDownloadState({ phase: "verifying", progress: 100, error: null });
      if (sha256) {
        const ok = (await invoke("verify_sha256", { path: tmpPath, expected: sha256 })) as boolean;
        if (!ok) { setDownloadState({ phase: "idle", progress: 0, error: "SHA256 校验失败，文件可能损坏" }); return; }
      }
      setDownloadState({ phase: "idle", progress: 100, error: null });
      await new Promise((r) => setTimeout(r, 800));
      setUpdateDialog(null);
      setDownloadState({ phase: "idle", progress: 0, error: null });
      addToast("success", `已下载 ${version}，正在打开安装包...`);
      await invoke("open_file", { path: tmpPath });
    } catch (e: any) {
      setDownloadState({ phase: "idle", progress: 0, error: String(e) });
    }
    unlisten();
  }, [updateDialog, addToast]);

  // Auto-check on startup (silent) — intentionally runs once; handleCheckUpdate is stable
  useEffect(() => { handleCheckUpdate({ silent: true }); // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return { updateDialog, downloadState, handleCheckUpdate, handleConfirmUpdate, setUpdateDialog, setDownloadState };
}
