// Full screen on the Play screen: the app's own chrome (top bar, sidebar, status bar) is put
// away so only the lights and a transport that hides itself are left, and the window is asked to
// fill its display. Where the window can't (or the system says no), the lights still fill the
// window.

import { create } from "zustand";
import { appWindow } from "../api/appWindow";

interface TheatreState {
  on: boolean;
  enter(): void;
  exit(): void;
}

export const useTheatre = create<TheatreState>((set, get) => ({
  on: false,
  enter() {
    if (get().on) return;
    set({ on: true });
    // Asked for straight from the click or key press: a browser refuses it otherwise.
    void appWindow.setFullscreen(true).catch(() => undefined);
  },
  exit() {
    if (!get().on) return;
    set({ on: false });
    void appWindow.setFullscreen(false).catch(() => undefined);
  },
}));

/**
 * Leaves full screen when the window stops filling its display some other way (the system's own
 * button, Esc in a browser). Returns a function that stops watching.
 */
export function watchDisplay(): () => void {
  let filled = false;
  return appWindow.onFullscreenChange((on) => {
    if (on) filled = true;
    else if (filled) {
      filled = false;
      useTheatre.setState({ on: false });
    }
  });
}
