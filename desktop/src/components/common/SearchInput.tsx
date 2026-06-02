import React, { useRef, useEffect } from "react";
import { Search, X } from "lucide-react";

interface SearchInputProps {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
}

export function SearchInput({
  value,
  onChange,
  placeholder = "Search...",
}: SearchInputProps) {
  const ref = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "f") {
        e.preventDefault();
        ref.current?.focus();
        ref.current?.select();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  return (
    <div className="relative group">
      <Search
        size={13}
        className="absolute left-2 top-1/2 -translate-y-1/2 text-app-text-muted
          transition-colors group-focus-within:text-app-accent"
      />
      <input
        ref={ref}
        type="text"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        spellCheck={false}
        className="w-full h-[28px] pl-7 pr-6 text-xs bg-app-input
          border-app-border-light
          text-app-text placeholder:text-app-text-muted
          hover:border-app-border
          focus:border-app-accent focus:shadow-[0_0_0_1px_var(--app-accent),0_0_8px_var(--app-glow)]"
      />
      {value && (
        <button
          onClick={() => onChange("")}
          className="absolute right-1.5 top-1/2 -translate-y-1/2 p-0.5
            text-app-text-muted hover:text-app-text hover:bg-[var(--app-hover)]
            transition-fast"
        >
          <X size={11} />
        </button>
      )}
    </div>
  );
}
