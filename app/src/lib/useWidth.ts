import { type RefObject, useLayoutEffect, useState, useSyncExternalStore } from "react";

/**
 * Window and element widths, for screens that rearrange themselves as the window narrows.
 */

/** Below this window width the sidebar shows only its icons (unless the user chose otherwise). */
export const SIDEBAR_RAIL_BELOW = 1280;
/** Below this window width the assistant floats over the screen instead of taking a column. */
export const ASSISTANT_OVERLAY_BELOW = 1440;

function subscribe(onChange: () => void) {
  window.addEventListener("resize", onChange);
  return () => window.removeEventListener("resize", onChange);
}

const windowWidth = () => window.innerWidth;

/** The window's width in CSS pixels, kept up to date as it's resized. */
export function useWindowWidth(): number {
  return useSyncExternalStore(subscribe, windowWidth, () => 1920);
}

/**
 * An element's width in CSS pixels, kept up to date as it changes; null until it has been laid
 * out (so a screen keeps its full arrangement where nothing is measured, as in tests).
 */
export function useElementWidth(ref: RefObject<HTMLElement | null>): number | null {
  const [width, setWidth] = useState<number | null>(null);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const measure = () => {
      const w = Math.round(el.getBoundingClientRect().width);
      setWidth(w > 0 ? w : null);
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, [ref]);
  return width;
}
