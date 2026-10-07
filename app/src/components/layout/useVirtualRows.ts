import { type RefObject, useCallback, useLayoutEffect, useState } from "react";

/** Rows drawn above and below the visible ones, so a quick scroll doesn't show blanks. */
const OVERSCAN = 8;
/** The height assumed before the list has been measured (and where nothing is laid out, as in tests). */
const UNMEASURED_PX = 600;

/**
 * Only the rows in (or near) view of a scrolling list of `count` rows, each `rowPx` tall: which to
 * draw, how tall the whole list is, and a way to scroll a row into view. Thousands of rows cost no
 * more than the few dozen on screen.
 */
export function useVirtualRows(ref: RefObject<HTMLElement | null>, count: number, rowPx: number) {
  const [scrollTop, setScrollTop] = useState(0);
  const [height, setHeight] = useState(0);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const measure = () => setHeight(el.clientHeight);
    const onScroll = () => setScrollTop(el.scrollTop);
    measure();
    el.addEventListener("scroll", onScroll, { passive: true });
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => {
      el.removeEventListener("scroll", onScroll);
      observer.disconnect();
    };
  }, [ref]);
  const view = height || UNMEASURED_PX;
  const first = Math.max(0, Math.floor(scrollTop / rowPx) - OVERSCAN);
  const last = Math.min(count, Math.ceil((scrollTop + view) / rowPx) + OVERSCAN);
  /** Scrolls just enough to show row `index` (nothing when it's already in view). */
  const reveal = useCallback(
    (index: number) => {
      const el = ref.current;
      if (!el || index < 0) return;
      const shown = el.clientHeight || UNMEASURED_PX;
      const top = index * rowPx;
      let next: number | null = null;
      if (top < el.scrollTop) next = top;
      else if (top + rowPx > el.scrollTop + shown) next = top + rowPx - shown;
      if (next !== null) {
        el.scrollTop = next;
        setScrollTop(next);
      }
    },
    [ref, rowPx],
  );
  return { first, last, total: count * rowPx, reveal };
}
