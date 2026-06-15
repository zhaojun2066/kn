import { useState, useCallback, type ReactNode } from "react";
import { ConfirmDialog } from "../components/ConfirmDialog";

/** Standard overwrite confirmation message for a named resource. */
export function describeOverwrite(name: string, context?: string): string {
  return context
    ? `"${name}" 已存在于${context}，是否覆盖？`
    : `"${name}" 已存在，是否覆盖？`;
}

export interface OverwriteRequest {
  /** Display name for the resource being overwritten */
  resourceName: string;
  /** Custom message override */
  message: string;
  resolve: (confirmed: boolean) => void;
}

export interface OverwriteConfirmOptions {
  /** Dialog title (defaults to "覆盖确认") */
  title?: string;
  /** Confirm button label (defaults to "覆盖") */
  confirmLabel?: string;
  /** Confirm button variant (defaults to "danger") */
  variant?: "primary" | "danger";
}

/**
 * Shared overwrite-confirmation hook.
 *
 * Returns a `requestOverwrite` function that returns a promise — resolves
 * to `true` (confirm) or `false` (cancel) — plus `overwriteDialog`, a
 * pre-configured ConfirmDialog element to place in your JSX tree.
 */
export function useOverwriteConfirm(opts?: OverwriteConfirmOptions) {
  const title = opts?.title ?? "覆盖确认";
  const confirmLabel = opts?.confirmLabel ?? "覆盖";
  const variant = opts?.variant ?? "danger";

  const [overwrite, setOverwrite] = useState<OverwriteRequest | null>(null);

  /**
   * Returns a promise that resolves to `true` (confirm) or `false` (cancel).
   *
   * @param message — Full confirm message to display.
   *   Use `describeOverwrite(name)` for the standard template.
   */
  /**
   * Request user confirmation. Returns a promise that resolves to
   * `true` (confirm) or `false` (cancel/dismiss).
   *
   * If a previous request is still pending, it is automatically
   * cancelled (resolved with `false`) before showing the new one.
   * This prevents multiple confirmation dialogs from stacking.
   */
  const requestOverwrite = useCallback(
    (message: string): Promise<boolean> => {
      return new Promise<boolean>((resolve) => {
        setOverwrite((prev) => {
          // Cancel any previously-pending request to prevent stacking
          prev?.resolve(false);
          return { resourceName: "", message, resolve };
        });
      });
    },
    [],
  );

  const dismiss = useCallback(() => {
    overwrite?.resolve(false);
    setOverwrite(null);
  }, [overwrite]);

  const confirm = useCallback(() => {
    overwrite?.resolve(true);
    setOverwrite(null);
  }, [overwrite]);

  const dialog: ReactNode = overwrite ? (
    <ConfirmDialog
      open={overwrite !== null}
      title={title}
      message={overwrite.message}
      confirmLabel={confirmLabel}
      variant={variant}
      onConfirm={confirm}
      onCancel={dismiss}
    />
  ) : null;

  return { requestOverwrite, overwriteDialog: dialog, hasPending: overwrite !== null };
}
