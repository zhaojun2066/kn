export const projectLikeListRowChrome =
  "mx-1.5 my-0.5 rounded-lg border-l-[3px] transition-all duration-fast";

export function projectLikeListRowState({
  selected,
  checked,
  focused,
}: {
  selected?: boolean;
  checked?: boolean;
  focused?: boolean;
}) {
  if (selected) {
    return "bg-app-selected text-app-text border-l-app-accent shadow-sm";
  }
  if (checked) {
    return "bg-app-hover text-app-text border-l-app-amber";
  }
  if (focused) {
    return "bg-app-hover text-app-text border-l-app-text-muted";
  }
  return "text-app-text border-l-transparent hover:bg-app-hover";
}
