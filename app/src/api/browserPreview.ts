// The preview's own window when PixelFlow runs in a browser (UI development, the demo): a
// browser window of the same page with `?window=preview`. The desktop app's is made by its shell.
//
// A browser has no engine for two windows to ask, so the main page shares its in-memory one:
// the preview's page takes it from the page that opened it, and the two then poll the same
// engine just as the desktop app's two windows do.

import type { Backend } from "./backend";
import type { SequencerApi } from "./sequencer";

/** The engine a page talks to. */
export interface SharedEngine {
  backend: Backend;
  sequencer: SequencerApi;
}

type Sharing = Window & { __pixelflowEngine?: SharedEngine };

/** Lets a preview window opened from this page use this page's engine. */
export function shareEngine(engine: SharedEngine) {
  (window as Sharing).__pixelflowEngine = engine;
}

/** The engine of the page that opened this one, when there is one to share. */
export function openerEngine(): SharedEngine | null {
  try {
    return (window.opener as Sharing | null)?.__pixelflowEngine ?? null;
  } catch {
    // Opened from somewhere else: nothing to share.
    return null;
  }
}

const PLACE_KEY = "pixelflow.previewWindowPlace";
const WINDOW_NAME = "pixelflow-preview";

interface Place {
  x: number;
  y: number;
  width: number;
  height: number;
}

function lastPlace(): Place | null {
  try {
    const saved = JSON.parse(localStorage.getItem(PLACE_KEY) ?? "null") as Partial<Place> | null;
    const numbers = [saved?.x, saved?.y, saved?.width, saved?.height];
    return saved && numbers.every((n) => typeof n === "number" && Number.isFinite(n)) ? (saved as Place) : null;
  } catch {
    return null;
  }
}

/** For the preview's page, as it closes: where its window is, so it opens there next time. */
export function rememberBrowserPlace() {
  try {
    const place: Place = { x: window.screenX, y: window.screenY, width: window.outerWidth, height: window.outerHeight };
    localStorage.setItem(PLACE_KEY, JSON.stringify(place));
  } catch {
    // Storage unavailable: it opens at its usual size.
  }
}

/** The preview page's address: this page's, with the flag that makes it draw only the preview. */
export function previewPageUrl(href = location.href): string {
  const url = new URL(href);
  url.searchParams.set("window", "preview");
  url.hash = "";
  return url.toString();
}

/** Opens the preview's browser window where it was last; null when the browser blocks it. */
export function openBrowserPreview(): Window | null {
  const place = lastPlace();
  const features = place ? `popup,width=${place.width},height=${place.height},left=${place.x},top=${place.y}` : "popup,width=960,height=600";
  return window.open(previewPageUrl(), WINDOW_NAME, features);
}
