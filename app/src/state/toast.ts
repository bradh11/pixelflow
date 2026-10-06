import { create } from "zustand";

/** A short message that something happened ("Saved Demo House"), with an optional action (Undo). */
export interface Toast {
  id: number;
  text: string;
  action?: { label: string; run: () => unknown };
}

/** How long a toast stays: a plain one briefly, one with an action long enough to use it. */
export const TOAST_MS = 2500;
export const TOAST_ACTION_MS = 6000;

interface ToastState {
  toasts: Toast[];
  show(text: string, action?: Toast["action"]): number;
  dismiss(id: number): void;
}

let next = 1;
const timers = new Map<number, ReturnType<typeof setTimeout>>();

export const useToasts = create<ToastState>((set, get) => ({
  toasts: [],
  show(text, action) {
    const id = next++;
    // The newest replaces older ones beyond three, so a burst never covers the screen.
    set({ toasts: [...get().toasts.slice(-2), { id, text, action }] });
    timers.set(
      id,
      setTimeout(() => get().dismiss(id), action ? TOAST_ACTION_MS : TOAST_MS),
    );
    return id;
  },
  dismiss(id) {
    clearTimeout(timers.get(id));
    timers.delete(id);
    set({ toasts: get().toasts.filter((t) => t.id !== id) });
  },
}));

/** Shows a toast. */
export const toast = (text: string, action?: Toast["action"]) => useToasts.getState().show(text, action);
