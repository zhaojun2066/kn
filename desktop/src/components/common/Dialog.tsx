import React, { useEffect, useCallback, useRef } from "react";

interface DialogProps {
  open: boolean;
  onClose: () => void;
  width?: string;
  children: React.ReactNode;
  persistent?: boolean;
}

const FOCUSABLE = 'a[href], button:not([disabled]), textarea:not([disabled]), input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])';

export function Dialog({ open, onClose, width = "420px", children, persistent }: DialogProps) {
  const panelRef = useRef<HTMLDivElement>(null);
  const prevFocusRef = useRef<HTMLElement | null>(null);

  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (e.key === "Escape" && !persistent) {
        onClose();
        return;
      }
      if (e.key === "Tab" && panelRef.current) {
        const focusable = panelRef.current.querySelectorAll<HTMLElement>(FOCUSABLE);
        if (focusable.length === 0) return;
        const first = focusable[0];
        const last = focusable[focusable.length - 1];
        if (e.shiftKey) {
          if (document.activeElement === first) {
            e.preventDefault();
            last.focus();
          }
        } else {
          if (document.activeElement === last) {
            e.preventDefault();
            first.focus();
          }
        }
      }
    },
    [onClose, persistent]
  );

  useEffect(() => {
    if (open) {
      prevFocusRef.current = document.activeElement as HTMLElement;
      // Focus first focusable element after mount
      requestAnimationFrame(() => {
        if (panelRef.current) {
          const first = panelRef.current.querySelector<HTMLElement>(FOCUSABLE);
          first?.focus();
        }
      });
      document.addEventListener("keydown", handleKeyDown);
      return () => {
        document.removeEventListener("keydown", handleKeyDown);
        prevFocusRef.current?.focus();
      };
    }
  }, [open, handleKeyDown]);

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm animate-[fadeIn_100ms_ease-out]"
      onClick={(e) => {
        if (!persistent && e.target === e.currentTarget) onClose();
      }}
    >
      <div
        ref={panelRef}
        className="bg-[var(--app-panel)] border border-[var(--app-border)] shadow-dialog animate-[scaleIn_150ms_ease-out] flex flex-col max-h-[85vh] outline-none"
        style={{ width }}
        role="dialog"
        aria-modal="true"
        aria-label="Dialog"
        onClick={(e) => e.stopPropagation()}
      >
        {children}
      </div>
    </div>
  );
}
