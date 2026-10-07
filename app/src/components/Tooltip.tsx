import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { comboLabel } from "../lib/shortcuts";

/**
 * The app's one tooltip. Mounted once (in `App`), it shows a short label beside whatever the
 * pointer rests on, or whatever the keyboard moves focus to, when that element has a
 * `data-tip` (and optionally `data-tip-key`, a shortcut) or a `title`.
 *
 * A `title` is taken over while its tooltip shows, so the browser's own slow tooltip doesn't
 * appear as well, and put back afterwards.
 */

/** How long the pointer rests (or focus stays) before the tooltip shows. */
export const TOOLTIP_DELAY_MS = 350;
/** Within this long of one tooltip closing, the next shows straight away (moving along a bar). */
const WARM_MS = 600;
const GAP = 6;
const TOOLTIP_ID = "pf-tooltip";

const TIPPED = "[data-tip], [title], [data-tip-title]";

interface Shown {
  el: HTMLElement;
  text: string;
  shortcut: string | null;
}

/** Whether the text already names the shortcut as a word of its own: "Save (⌘S)". */
function mentions(text: string, shortcut: string): boolean {
  const escaped = shortcut.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return new RegExp(`(^|[\\s(])${escaped}($|[\\s),])`).test(text);
}

function tipOf(el: HTMLElement): Shown | null {
  const text = el.dataset.tip ?? el.dataset.tipTitle ?? el.getAttribute("title");
  if (!text) return null;
  const keys = el.dataset.tipKey ?? el.getAttribute("aria-keyshortcuts");
  // A shortcut already in the text ("Save (⌘S)") isn't repeated.
  const shortcut = keys ? comboLabel(keys.split(" ")[0]) : null;
  return { el, text, shortcut: shortcut && !mentions(text, shortcut) ? shortcut : null };
}

/** Takes an element's `title` while its tooltip shows (the browser would show it too). */
function holdTitle(el: HTMLElement) {
  const title = el.getAttribute("title");
  if (title === null) return;
  el.dataset.tipTitle = title;
  el.removeAttribute("title");
}

function giveBackTitle(el: HTMLElement) {
  const title = el.dataset.tipTitle;
  if (title === undefined) return;
  delete el.dataset.tipTitle;
  // React may have set a new title meanwhile; that one stands.
  if (!el.hasAttribute("title")) el.setAttribute("title", title);
}

export function TooltipLayer() {
  const [shown, setShown] = useState<Shown | null>(null);
  const [place, setPlace] = useState<{ left: number; top: number } | null>(null);
  const bubble = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | null = null;
    let current: HTMLElement | null = null;
    let visible = false;
    let lastClosed = 0;
    let keyboard = false;
    let described: HTMLElement | null = null;

    const cancel = () => {
      if (timer) clearTimeout(timer);
      timer = null;
    };
    const hide = () => {
      cancel();
      if (current) giveBackTitle(current);
      if (visible) lastClosed = Date.now();
      visible = false;
      if (described) {
        described.removeAttribute("aria-describedby");
        described = null;
      }
      current = null;
      setShown(null);
    };
    const show = (el: HTMLElement) => {
      const tip = tipOf(el);
      if (!tip || !el.isConnected) return;
      holdTitle(el);
      current = el;
      // Said to screen readers too, when it says more than the element's name.
      if (!el.hasAttribute("aria-describedby") && tip.text !== el.getAttribute("aria-label")) {
        el.setAttribute("aria-describedby", TOOLTIP_ID);
        described = el;
      }
      visible = true;
      setShown(tip);
    };
    const schedule = (el: HTMLElement) => {
      if (el === current) return;
      hide();
      current = el;
      // Hold the title now, so the browser's tooltip doesn't start its own wait.
      holdTitle(el);
      const wait = Date.now() - lastClosed < WARM_MS ? 0 : TOOLTIP_DELAY_MS;
      timer = setTimeout(() => {
        timer = null;
        show(el);
      }, wait);
    };
    const target = (e: Event) => (e.target instanceof Element ? (e.target.closest(TIPPED) as HTMLElement | null) : null);

    const onOver = (e: PointerEvent) => {
      if (e.pointerType === "touch") return;
      const el = target(e);
      if (el) schedule(el);
      else if (current && !current.contains(e.target as Node)) hide();
    };
    const onOut = (e: PointerEvent) => {
      if (!current) return;
      const to = e.relatedTarget as Node | null;
      if (to && current.contains(to)) return;
      if (document.activeElement === current && keyboard) return;
      hide();
    };
    const onFocusIn = (e: FocusEvent) => {
      if (!keyboard) return;
      const el = target(e);
      if (el) schedule(el);
    };
    const onFocusOut = (e: FocusEvent) => {
      if (current && e.target === current) hide();
    };
    const onKey = (e: KeyboardEvent) => {
      keyboard = true;
      if (e.key === "Escape" && current) hide();
    };
    const onDown = () => {
      keyboard = false;
      hide();
    };
    document.addEventListener("pointerover", onOver);
    document.addEventListener("pointerout", onOut);
    document.addEventListener("focusin", onFocusIn);
    document.addEventListener("focusout", onFocusOut);
    document.addEventListener("keydown", onKey, true);
    document.addEventListener("pointerdown", onDown, true);
    window.addEventListener("scroll", hide, true);
    window.addEventListener("blur", hide);
    return () => {
      hide();
      document.removeEventListener("pointerover", onOver);
      document.removeEventListener("pointerout", onOut);
      document.removeEventListener("focusin", onFocusIn);
      document.removeEventListener("focusout", onFocusOut);
      document.removeEventListener("keydown", onKey, true);
      document.removeEventListener("pointerdown", onDown, true);
      window.removeEventListener("scroll", hide, true);
      window.removeEventListener("blur", hide);
    };
  }, []);

  // Below the element, or above it near the bottom of the window; kept inside the window.
  useLayoutEffect(() => {
    if (!shown || !bubble.current) {
      setPlace(null);
      return;
    }
    const r = shown.el.getBoundingClientRect();
    const b = bubble.current.getBoundingClientRect();
    const below = r.bottom + GAP;
    const top = below + b.height > window.innerHeight - 4 ? Math.max(4, r.top - GAP - b.height) : below;
    const left = Math.min(Math.max(4, r.left + r.width / 2 - b.width / 2), Math.max(4, window.innerWidth - b.width - 4));
    setPlace({ left: Math.round(left), top: Math.round(top) });
  }, [shown]);

  if (!shown) return null;
  return createPortal(
    <div
      ref={bubble}
      id={TOOLTIP_ID}
      role="tooltip"
      style={{ left: place?.left ?? 0, top: place?.top ?? 0, visibility: place ? "visible" : "hidden" }}
      className="pointer-events-none fixed z-[100] flex max-w-xs items-center gap-2 rounded-md bg-neutral-900 px-2 py-1 text-xs text-white shadow-lg dark:bg-neutral-100 dark:text-neutral-900"
    >
      <span>{shown.text}</span>
      {shown.shortcut && <kbd className="rounded bg-white/15 px-1 font-sans text-[11px] dark:bg-black/10">{shown.shortcut}</kbd>}
    </div>,
    document.body,
  );
}
