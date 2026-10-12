import { create } from "zustand";

/** The app's own panels: About (version, licence, credits), and the ways to support the project. */
export type AboutPanel = "about" | "support";

/** Which of them is up, if either. Nothing opens one but the user asking for it. */
export const useAbout = create<{ open: AboutPanel | null; show(panel: AboutPanel): void; close(): void }>((set) => ({
  open: null,
  show: (open) => set({ open }),
  close: () => set({ open: null }),
}));
