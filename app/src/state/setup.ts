// What the "Set up your show" checklist remembers on this computer, per show: whether a test
// pattern has been sent, and whether the checklist was put away.

import { create } from "zustand";
import type { ShowSnapshot } from "../api/types";
import { useApp } from "./store";

const KEY = "pixelflow.setup";
/** Shows remembered at most (the oldest are forgotten). */
const LIMIT = 200;

interface Saved {
  tested: string[];
  dismissed: string[];
}

function load(): Saved {
  try {
    const saved = JSON.parse(localStorage.getItem(KEY) ?? "{}") as Partial<Saved>;
    const list = (v: unknown) => (Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : []);
    return { tested: list(saved.tested), dismissed: list(saved.dismissed) };
  } catch {
    return { tested: [], dismissed: [] };
  }
}

function save(saved: Saved) {
  try {
    localStorage.setItem(KEY, JSON.stringify(saved));
  } catch {
    // Storage unavailable: it's remembered until the app closes.
  }
}

/**
 * Which show the checklist is about: its file, or until it has one, the id the app gave this
 * show when it opened (every new show is "Untitled Show", so the name won't do).
 */
export function setupKey(snapshot: Pick<ShowSnapshot, "path"> | null, showId: string): string | null {
  if (!snapshot) return null;
  return snapshot.path ?? `unsaved:${showId}`;
}

/** The open show's key. */
export function currentSetupKey(): string | null {
  const { snapshot, showId } = useApp.getState();
  return setupKey(snapshot, showId);
}

interface SetupState extends Saved {
  markTested(key: string | null): void;
  /** Moves a show's record to its new key (a new show's first save). */
  carry(from: string, to: string): void;
  /** Puts the checklist away for the show (or brings it back). */
  setDismissed(key: string | null, dismissed: boolean): void;
}

const add = (list: string[], key: string) => [...list.filter((k) => k !== key), key].slice(-LIMIT);

export const useSetup = create<SetupState>((set, get) => ({
  ...load(),
  markTested(key) {
    if (!key || get().tested.includes(key)) return;
    const tested = add(get().tested, key);
    save({ tested, dismissed: get().dismissed });
    set({ tested });
  },
  carry(from, to) {
    const move = (list: string[]) => (list.includes(from) ? add(list.filter((k) => k !== from), to) : list);
    const next = { tested: move(get().tested), dismissed: move(get().dismissed) };
    save(next);
    set(next);
  },
  setDismissed(key, dismissed) {
    if (!key) return;
    const list = dismissed ? add(get().dismissed, key) : get().dismissed.filter((k) => k !== key);
    save({ tested: get().tested, dismissed: list });
    set({ dismissed: list });
  },
}));

// A new show saved for the first time keeps what the checklist knew about it.
useApp.subscribe((state, before) => {
  if (state.showId !== before.showId || !state.snapshot?.path || !before.snapshot || before.snapshot.path) return;
  useSetup.getState().carry(`unsaved:${state.showId}`, state.snapshot.path);
});
