import { create } from "zustand";

/** "success": something was done ("Saved Demo House"); "info": just so you know. */
export type ToastTone = "success" | "info";

/** A short message that something happened, with an optional action (Undo). */
export interface Toast {
  id: number;
  text: string;
  action?: { label: string; run: () => unknown };
  tone: ToastTone;
}

/** How long a toast stays: a plain one briefly, one with an action long enough to use it. */
export const TOAST_MS = 2500;
export const TOAST_ACTION_MS = 6000;

interface ToastState {
  toasts: Toast[];
  show(text: string, action?: Toast["action"], tone?: ToastTone): number;
  dismiss(id: number): void;
  /** Holds a toast while it's pointed at or focused (so its action can be reached). */
  pause(id: number): void;
  /** Starts its time again once the pointer or focus has left. */
  resume(id: number): void;
}

let next = 1;
const timers = new Map<number, ReturnType<typeof setTimeout>>();

export const useToasts = create<ToastState>((set, get) => {
  const schedule = (t: Toast) => {
    clearTimeout(timers.get(t.id));
    timers.set(
      t.id,
      setTimeout(() => get().dismiss(t.id), t.action ? TOAST_ACTION_MS : TOAST_MS),
    );
  };
  return {
    toasts: [],
    show(text, action, tone = "success") {
      const toast: Toast = { id: next++, text, action, tone };
      // The newest replaces older ones beyond three, so a burst never covers the screen.
      set({ toasts: [...get().toasts.slice(-2), toast] });
      schedule(toast);
      return toast.id;
    },
    dismiss(id) {
      clearTimeout(timers.get(id));
      timers.delete(id);
      set({ toasts: get().toasts.filter((t) => t.id !== id) });
    },
    pause(id) {
      clearTimeout(timers.get(id));
      timers.delete(id);
    },
    resume(id) {
      const toast = get().toasts.find((t) => t.id === id);
      if (toast) schedule(toast);
    },
  };
});

/** Shows a toast. */
export const toast = (text: string, action?: Toast["action"], tone?: ToastTone) => useToasts.getState().show(text, action, tone);
