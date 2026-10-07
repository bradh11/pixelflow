// Every keyboard shortcut in the app, in one place. The shortcut sheet (?) is built from this
// list, and tooltips, menus and the command palette read their key hints from it, so what they
// say can't drift from each other. The key handlers themselves live with their screens
// (useShortcuts, useLayoutKeys, useLayout3dKeys, useSequenceKeys); a test checks that each one
// listed here does what it says.

export type ShortcutGroup = "Everywhere" | "Layout" | "Layout in 3D" | "Wiring" | "Sequence";

export const SHORTCUT_GROUPS: ShortcutGroup[] = ["Everywhere", "Layout", "Layout in 3D", "Wiring", "Sequence"];

export interface Shortcut {
  id: string;
  group: ShortcutGroup;
  /**
   * The keys, as `aria-keyshortcuts` writes them: "Meta+Shift+S", with alternatives separated by
   * spaces ("Delete Backspace"). ⌘ is Ctrl on Windows and Linux.
   */
  keys: string;
  /** What the keys do, in a few words. */
  label: string;
  /** Shown instead of the keys when they read better summed up ("1–5"). */
  display?: string;
}

export const SHORTCUTS = [
  // Everywhere
  { id: "palette", group: "Everywhere", keys: "Meta+K", label: "Open the command palette" },
  { id: "shortcuts", group: "Everywhere", keys: "?", label: "Show keyboard shortcuts" },
  { id: "assistant", group: "Everywhere", keys: "Meta+L", label: "Open or close the assistant" },
  { id: "new", group: "Everywhere", keys: "Meta+N", label: "New show" },
  { id: "open", group: "Everywhere", keys: "Meta+O", label: "Open a show" },
  { id: "open-recent", group: "Everywhere", keys: "Meta+Shift+O", label: "Open a recent show" },
  { id: "close-show", group: "Everywhere", keys: "Meta+W", label: "Close the show" },
  { id: "save", group: "Everywhere", keys: "Meta+S", label: "Save" },
  { id: "save-as", group: "Everywhere", keys: "Meta+Shift+S", label: "Save as" },
  { id: "undo", group: "Everywhere", keys: "Meta+Z", label: "Undo" },
  { id: "redo", group: "Everywhere", keys: "Meta+Shift+Z", label: "Redo" },
  { id: "context-menu", group: "Everywhere", keys: "Shift+F10 ContextMenu", label: "Open the right-click menu for what has focus" },
  // Layout
  { id: "layout-escape", group: "Layout", keys: "Escape", label: "Stop drawing, put the tool down, or clear the selection" },
  { id: "layout-delete", group: "Layout", keys: "Backspace Delete", label: "Delete the selected props (or the picked point)" },
  { id: "layout-nudge", group: "Layout", keys: "ArrowLeft ArrowRight ArrowUp ArrowDown", label: "Move the selection a little", display: "← → ↑ ↓" },
  { id: "layout-nudge-far", group: "Layout", keys: "Shift+ArrowLeft Shift+ArrowRight Shift+ArrowUp Shift+ArrowDown", label: "Move the selection ten times as far", display: "⇧ + arrow" }, // gitleaks:allow (key names)
  { id: "layout-select-all", group: "Layout", keys: "Meta+A", label: "Select every prop" },
  { id: "layout-cut", group: "Layout", keys: "Meta+X", label: "Cut" },
  { id: "layout-copy", group: "Layout", keys: "Meta+C", label: "Copy" },
  { id: "layout-paste", group: "Layout", keys: "Meta+V", label: "Paste" },
  { id: "layout-duplicate", group: "Layout", keys: "Meta+D", label: "Duplicate" },
  { id: "layout-group", group: "Layout", keys: "Meta+G", label: "Group the selected props" },
  { id: "layout-poly-finish", group: "Layout", keys: "Enter", label: "Finish the poly line being drawn" },
  { id: "layout-mode", group: "Layout", keys: "V", label: "Switch between the 2D and 3D views" },
  // Layout in 3D
  { id: "layout3d-fit", group: "Layout in 3D", keys: "F", label: "Fit everything in" },
  { id: "layout3d-views", group: "Layout in 3D", keys: "1 2 3 4 5", label: "Front, Top, Left, Right and Street views", display: "1–5" },
  // Wiring
  { id: "wiring-escape", group: "Wiring", keys: "Escape", label: "Leave click-to-wire (asks first if you made changes)" },
  // Sequence
  { id: "seq-play", group: "Sequence", keys: "Space", label: "Play or pause" },
  { id: "seq-step", group: "Sequence", keys: "ArrowLeft ArrowRight", label: "Move the selected effects, or the playhead, by a frame", display: "← →" },
  { id: "seq-step-beat", group: "Sequence", keys: "Shift+ArrowLeft Shift+ArrowRight", label: "Move them to the next beat instead", display: "⇧← ⇧→" }, // gitleaks:allow (key names)
  { id: "seq-row", group: "Sequence", keys: "ArrowUp ArrowDown", label: "Pick the row above or below", display: "↑ ↓" },
  { id: "seq-ends", group: "Sequence", keys: "Home End", label: "Jump to the start or the end" },
  { id: "seq-delete", group: "Sequence", keys: "Backspace Delete", label: "Delete the selected effects (or timing marks)" },
  { id: "seq-select-all", group: "Sequence", keys: "Meta+A", label: "Select every effect" },
  { id: "seq-copy", group: "Sequence", keys: "Meta+C", label: "Copy the selected effects" },
  { id: "seq-paste", group: "Sequence", keys: "Meta+V", label: "Paste at the playhead" },
  { id: "seq-duplicate", group: "Sequence", keys: "Meta+D", label: "Duplicate the selected effects" },
  { id: "seq-escape", group: "Sequence", keys: "Escape", label: "Clear the selection" },
  { id: "seq-tap", group: "Sequence", keys: "T", label: "Tap a timing mark in at the playhead" },
  { id: "seq-loop", group: "Sequence", keys: "L", label: "Loop playback on or off" },
] as const satisfies readonly Shortcut[];

export type ShortcutId = (typeof SHORTCUTS)[number]["id"];

const BY_ID = new Map<string, Shortcut>(SHORTCUTS.map((s) => [s.id, s]));

const KEY_NAMES: Record<string, string> = {
  ArrowLeft: "←",
  ArrowRight: "→",
  ArrowUp: "↑",
  ArrowDown: "↓",
  Escape: "Esc",
  Backspace: "⌫",
  Enter: "Return",
  ContextMenu: "Menu key",
};

/** One combination as the keys are labelled on a Mac: "Meta+Shift+S" is "⇧⌘S". */
export function comboLabel(combo: string): string {
  const parts = combo.split("+");
  // "Meta++" (a plus key) splits into an empty last part.
  const key = parts.at(-1) === "" ? "+" : parts.at(-1)!;
  const mods = new Set(parts.slice(0, -1));
  const named = KEY_NAMES[key] ?? (key.length === 1 ? key.toUpperCase() : key);
  return (
    (mods.has("Control") ? "Ctrl+" : "") +
    (mods.has("Alt") ? "⌥" : "") +
    (mods.has("Shift") ? "⇧" : "") +
    (mods.has("Meta") ? "⌘" : "") +
    named
  );
}

/** All of a shortcut's keys, as shown on the sheet: "Delete / ⌫". */
export function keysLabel(s: Shortcut): string {
  return s.display ?? s.keys.split(" ").map(comboLabel).join(" / ");
}

/** The shortcut with this id. */
export function shortcutOf(id: ShortcutId): Shortcut {
  return BY_ID.get(id)!;
}

/** The key hint for a tooltip, menu or palette entry: its first combination ("⌘S"). */
export function hintFor(id: ShortcutId): string {
  const s = shortcutOf(id);
  return s.display && !s.keys.includes(" ") ? s.display : comboLabel(s.keys.split(" ")[0]);
}

/** The `aria-keyshortcuts` value for a control that does the same. */
export function ariaKeysFor(id: ShortcutId): string {
  return shortcutOf(id).keys;
}

/** Shortcuts whose label, group or keys match what was typed (every word of it). */
export function searchShortcuts(query: string): Shortcut[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  const all: Shortcut[] = [...SHORTCUTS];
  if (words.length === 0) return all;
  return all.filter((s) => {
    const text = `${s.label} ${s.group} ${keysLabel(s)} ${s.keys}`.toLowerCase();
    return words.every((w) => text.includes(w));
  });
}
