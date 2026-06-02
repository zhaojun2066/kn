import React from "react";

type Variant = "primary" | "secondary" | "danger" | "ghost" | "icon";

interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
  size?: "sm" | "md";
  title?: string;
  prompt?: string;
}

const promptChar: Record<Variant, string> = {
  primary:   ">",
  secondary: "$",
  danger:    "!",
  ghost:     "",
  icon:      "",
};

const base =
  "inline-flex items-center justify-center gap-1.5 font-mono font-medium " +
  "transition-all duration-fast focus-visible:outline-1 focus-visible:outline-[var(--app-focus)] " +
  "disabled:cursor-not-allowed disabled:opacity-35 select-none";

const variants: Record<Variant, string> = {
  primary:
    "bg-[var(--btn-pri-bg)] text-app-amber border border-[var(--btn-pri-border)] " +
    "hover:bg-[var(--btn-pri-hover-bg)] hover:border-[var(--btn-pri-hover-border)] hover:shadow-[0_0_12px_var(--app-glow-amber)] " +
    "active:bg-[var(--btn-pri-active-bg)] active:shadow-[0_0_6px_var(--app-glow-amber)]",
  secondary:
    "bg-[var(--btn-sec-bg)] text-app-text-dim border border-[var(--btn-sec-border)] " +
    "hover:bg-[var(--btn-sec-hover-bg)] hover:text-app-text hover:border-[var(--btn-sec-hover-border)] hover:shadow-[0_0_8px_var(--app-glow)] " +
    "active:bg-[var(--btn-sec-active-bg)]",
  danger:
    "bg-[var(--btn-danger-bg)] text-app-red border border-[var(--btn-danger-border)] " +
    "hover:bg-[var(--btn-danger-hover-bg)] hover:border-[var(--btn-danger-hover-border)] hover:shadow-[0_0_12px_var(--app-glow-red)] " +
    "active:bg-[var(--btn-danger-active-bg)]",
  ghost:
    "bg-transparent text-app-text-dim hover:text-app-text hover:bg-[var(--btn-ghost-hover-bg)] " +
    "active:bg-[var(--btn-ghost-active-bg)] border border-transparent hover:border-app-border",
  icon:
    "bg-transparent text-app-text-dim hover:text-app-text hover:bg-[var(--btn-ghost-hover-bg)] " +
    "active:bg-[var(--btn-ghost-active-bg)] p-0.5 border border-transparent hover:border-app-border",
};

const sizes: Record<string, string> = {
  sm: "h-[24px] px-2.5 text-xs",
  md: "h-[28px] px-3 text-sm",
};

export function Button({
  variant = "secondary",
  size = "sm",
  className = "",
  prompt,
  children,
  ...props
}: ButtonProps) {
  const p = prompt ?? promptChar[variant];

  return (
    <button
      className={`${base} ${variants[variant]} ${sizes[size]} ${className}`}
      {...props}
    >
      {p && (
        <span
          className={
            variant === "primary"
              ? "text-app-amber opacity-60"
              : variant === "danger"
              ? "text-app-red opacity-50"
              : "text-app-text-muted opacity-50"
          }
        >
          {p}
        </span>
      )}
      {children}
    </button>
  );
}
