import { useCallback, useRef, useState } from "react";

export type Toast = {
  id: number;
  type: "error" | "success";
  message: string;
  undoAction?: () => void;
  undoLabel?: string;
};

export function useToasts() {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const toastIdRef = useRef(0);

  const addToast = useCallback((type: "error" | "success", message: string) => {
    const id = ++toastIdRef.current;
    setToasts((prev) => [...prev, { id, type, message }]);
    setTimeout(
      () => setToasts((prev) => prev.filter((toast) => toast.id !== id)),
      type === "error" ? 12000 : 4000,
    );
  }, []);

  const dismissToast = useCallback((id: number) => {
    setToasts((prev) => prev.filter((toast) => toast.id !== id));
  }, []);

  return {
    toasts,
    setToasts,
    toastIdRef,
    addToast,
    dismissToast,
  };
}
