import type { KeyboardEvent as ReactKeyboardEvent, MouseEvent as ReactMouseEvent } from "react";
import { create } from "zustand";
import type { ShortcutId } from "../lib/shortcuts";

export interface MenuItem {
  label: string;
  run: () => unknown;
  /** The shortcut that does the same, shown beside it (from the shortcut registry). */
  shortcut?: ShortcutId;
  disabled?: boolean;
  danger?: boolean;
  /** A line above it, starting a new part of the menu. */
  separated?: boolean;
}

export interface OpenMenu {
  /** Where it opens, in window coordinates. */
  x: number;
  y: number;
  /** Its name for screen readers ("Mega Tree"). */
  label: string;
  items: MenuItem[];
  /** Where the focus goes back to when it closes. */
  opener: HTMLElement | null;
}

interface ContextMenuState {
  menu: OpenMenu | null;
  open(menu: OpenMenu): void;
  close(): void;
}

export const useContextMenu = create<ContextMenuState>((set) => ({
  menu: null,
  open: (menu) => set({ menu }),
  close: () => set({ menu: null }),
}));

/** Shift+F10, or the keyboard's menu key: the keyboard's right-click. */
export function isMenuKey(e: { key: string; shiftKey: boolean }): boolean {
  return e.key === "ContextMenu" || (e.shiftKey && e.key === "F10");
}

/**
 * Handlers that open a right-click menu on an element, from the mouse or the keyboard.
 * `items` is asked when the menu opens (null or nothing: no menu, and the browser's is held back).
 * From the keyboard it opens at the element's top left.
 */
export function contextMenuHandlers(label: string | (() => string), items: () => MenuItem[] | null) {
  const name = () => (typeof label === "function" ? label() : label);
  const show = (x: number, y: number, opener: HTMLElement) => {
    const list = items();
    if (list && list.length > 0) useContextMenu.getState().open({ x, y, label: name(), items: list, opener });
  };
  return {
    onContextMenu: (e: ReactMouseEvent<HTMLElement>) => {
      e.preventDefault();
      e.stopPropagation();
      show(e.clientX, e.clientY, e.currentTarget);
    },
    onKeyDown: (e: ReactKeyboardEvent<HTMLElement>) => {
      if (!isMenuKey(e)) return;
      e.preventDefault();
      e.stopPropagation();
      const box = e.currentTarget.getBoundingClientRect();
      show(box.left + 8, box.top + Math.min(box.height, 24), e.currentTarget);
    },
  };
}
