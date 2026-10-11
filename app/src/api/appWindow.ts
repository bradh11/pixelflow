// What the app asks of the window it's in (not of the engine): filling its display. Each window
// answers for itself, so the preview's own window fills whichever display it was dragged to.

import { invoke } from "@tauri-apps/api/core";
import { inTauri } from "./runtime";

export interface AppWindow {
  /** Makes this window fill its display, or puts it back. Where that can't be done, nothing happens. */
  setFullscreen(on: boolean): Promise<void>;
  /**
   * Calls `handler` when this window starts or stops filling its display, however that came
   * about (the system's own button, Esc in a browser). Returns a function that stops listening.
   */
  onFullscreenChange(handler: (on: boolean) => void): () => void;
}

/** How long after the window last changed size it's asked whether it fills its display: the
 * system's own animation has finished by then. */
const SETTLE_MS = 400;

/** The desktop app: the shell changes the window that asks (and only that one). */
const desktopWindow: AppWindow = {
  setFullscreen: (on) => invoke("set_fullscreen", { on }),
  onFullscreenChange(handler) {
    let timer: ReturnType<typeof setTimeout> | undefined;
    let known = false;
    let live = true;
    const ask = () => {
      clearTimeout(timer);
      timer = setTimeout(() => {
        invoke<boolean>("is_fullscreen").then(
          (on) => {
            if (!live || on === known) return;
            known = on;
            handler(on);
          },
          () => undefined,
        );
      }, SETTLE_MS);
    };
    window.addEventListener("resize", ask);
    return () => {
      live = false;
      clearTimeout(timer);
      window.removeEventListener("resize", ask);
    };
  },
};

/** A browser (UI development, the demo): the page itself goes full screen. */
const browserWindow: AppWindow = {
  async setFullscreen(on) {
    try {
      if (on) await document.documentElement.requestFullscreen?.();
      else if (document.fullscreenElement) await document.exitFullscreen();
    } catch {
      // Refused (not from a click or a key): the window stays as it is.
    }
  },
  onFullscreenChange(handler) {
    const changed = () => handler(document.fullscreenElement !== null);
    document.addEventListener("fullscreenchange", changed);
    return () => document.removeEventListener("fullscreenchange", changed);
  },
};

export const appWindow: AppWindow = inTauri() ? desktopWindow : browserWindow;
