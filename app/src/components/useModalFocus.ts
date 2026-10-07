import { useEffect } from "react";

const FOCUSABLE = 'a[href], button, input, select, textarea, [tabindex], [contenteditable="true"]';

/** What Tab can reach inside `root`, in order. */
function tabbable(root: HTMLElement): HTMLElement[] {
  return [...root.querySelectorAll<HTMLElement>(FOCUSABLE)].filter(
    (el) => el.tabIndex >= 0 && !(el as HTMLButtonElement).disabled && !el.closest("[inert], [hidden]"),
  );
}

/**
 * Keeps Tab and Shift-Tab inside the open modal dialog (the last `aria-modal` one, which is the
 * one on top), wrapping at either end, for every dialog in one place.
 */
export function useModalFocus() {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Tab" || e.defaultPrevented) return;
      const modals = document.querySelectorAll<HTMLElement>('[aria-modal="true"]');
      const modal = modals[modals.length - 1];
      if (!modal) return;
      const items = tabbable(modal);
      const first = items[0];
      const last = items[items.length - 1];
      const at = document.activeElement;
      const inside = at instanceof Node && modal.contains(at);
      let next: HTMLElement | undefined;
      if (!first) next = modal;
      else if (!inside) next = e.shiftKey ? last : first;
      else if (e.shiftKey && at === first) next = last;
      else if (!e.shiftKey && at === last) next = first;
      if (!next) return;
      e.preventDefault();
      // A dialog with nothing to tab to holds the focus itself.
      if (next === modal && !modal.hasAttribute("tabindex")) modal.tabIndex = -1;
      next.focus();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);
}
