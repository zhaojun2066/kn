import { useState, useEffect, useCallback } from "react";

const SCALE_KEY = "kn-ui-font-scale";
const BASE_FONT_SIZE = 13; // matches tailwind.config base
export const MIN_SCALE = 0.8;
export const MAX_SCALE = 1.3;

export function useFontScale() {
  const [scale, setScaleState] = useState(() => {
    try {
      const v = parseFloat(localStorage.getItem(SCALE_KEY) || "1.0");
      return Math.min(MAX_SCALE, Math.max(MIN_SCALE, v));
    } catch {
      return 1.0;
    }
  });

  // Apply scale to <html> element — Tailwind uses rem, so this scales everything
  useEffect(() => {
    document.documentElement.style.fontSize = `${BASE_FONT_SIZE * scale}px`;
  }, [scale]);

  const setScale = useCallback((s: number) => {
    const clamped = Math.min(MAX_SCALE, Math.max(MIN_SCALE, s));
    setScaleState(clamped);
    try {
      localStorage.setItem(SCALE_KEY, String(clamped));
    } catch {
      /* localStorage may fail in some contexts */
    }
  }, []);

  return { scale, setScale };
}
