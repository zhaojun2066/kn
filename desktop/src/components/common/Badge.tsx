import React from "react";

interface BadgeProps {
  children: React.ReactNode;
  variant?: "default" | "primary" | "green" | "red";
}

const variants = {
  default: "bg-[var(--app-hover)] text-app-text-dim border border-app-border",
  primary: "bg-[var(--app-selected)] text-app-accent border border-[var(--app-accent)]/30",
  green:   "bg-app-green-bg text-app-green border border-[var(--app-green-bg)]",
  red:     "bg-app-red-bg text-app-red border border-[var(--app-red-bg)]",
};

export function Badge({ children, variant = "default" }: BadgeProps) {
  return (
    <span className={`inline-flex items-center gap-1 px-1.5 py-px text-2xs font-medium font-mono ${variants[variant]}`}>
      {children}
    </span>
  );
}
