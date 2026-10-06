// What the "Set up your show" checklist remembers on this computer, per show: whether a test
// pattern has been sent, and whether the checklist was put away.

import { create } from "zustand";
import type { ShowSnapshot } from "../api/types";

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

/** Which show the checklist is about: its file, or its name until it has one. */
export function setupKey(snapshot: Pick<ShowSnapshot, "path" | "show"> | null): string | null {
  if (!snapshot) return null;
  return snapshot.path ?? `unsaved:${snapshot.show.name}`;
}

interface SetupState extends Saved {
  markTested(key: string | null): void;
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
  setDismissed(key, dismissed) {
    if (!key) return;
    const list = dismissed ? add(get().dismissed, key) : get().dismissed.filter((k) => k !== key);
    save({ tested: get().tested, dismissed: list });
    set({ dismissed: list });
  },
}));
