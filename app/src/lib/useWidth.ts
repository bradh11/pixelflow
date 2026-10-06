import { type RefObject, useLayoutEffect, useRef, useState, useSyncExternalStore } from "react";

/**
 * Window and element widths, for screens that rearrange themselves as the window narrows. Each
 * hook re-renders only when what's worked out from the width changes, not for every pixel of a
 * resize.
 */

/** Below this window width the assistant floats over the screen instead of taking a column. */
export const ASSISTANT_OVERLAY_BELOW = 1200;
/** Below this window width the sidebar shows only its icons (unless the user chose otherwise). */
export const SIDEBAR_RAIL_BELOW = 1280;
/** Below this window width a docked assistant is narrower, and folds the sidebar to icons. */
export const ASSISTANT_COMPACT_BELOW = 1440;

/**
 * The window's band: "narrow" (the assistant floats), "medium" (the sidebar shows icons),
 * "laptop" (a docked assistant is compact), or "wide".
 */
export type WindowBand = "narrow" | "medium" | "laptop" | "wide";

function bandOf(width: number): WindowBand {
  if (width < ASSISTANT_OVERLAY_BELOW) return "narrow";
  if (width < SIDEBAR_RAIL_BELOW) return "medium";
  if (width < ASSISTANT_COMPACT_BELOW) return "laptop";
  return "wide";
}

function subscribe(onChange: () => void) {
  window.addEventListener("resize", onChange);
  return () => window.removeEventListener("resize", onChange);
}

const currentBand = () => bandOf(window.innerWidth);

/** The window's band, kept up to date as it's resized. */
export function useWindowBand(): WindowBand {
  return useSyncExternalStore(subscribe, currentBand, () => "wide");
}

/**
 * Something worked out from an element's width (`derive` gets null until it has been laid out,
 * as in tests), kept up to date as the element changes size. Re-renders only when the derived
 * value changes (compared by its JSON).
 */
export function useElementWidth<T>(ref: RefObject<HTMLElement | null>, derive: (width: number | null) => T): T {
  const [value, setValue] = useState(() => derive(null));
  const derived = useRef(derive);
  derived.current = derive;
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    let last = JSON.stringify(derived.current(null));
    const measure = () => {
      const w = Math.round(el.getBoundingClientRect().width);
      const next = derived.current(w > 0 ? w : null);
      const key = JSON.stringify(next);
      if (key === last) return;
      last = key;
      setValue(next);
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, [ref]);
  return value;
}
