import { Fragment, type KeyboardEvent, useEffect, useLayoutEffect, useRef, useState } from "react";
import { hintFor } from "../lib/shortcuts";
import { type OpenMenu, useContextMenu } from "../state/contextMenu";

const EDGE = 8;

/**
 * The app's one right-click menu (opened with `contextMenuHandlers`): at the pointer, or at the
 * focused element for Shift+F10 and the menu key. Arrow keys, Home and End move through it,
 * Enter or Space runs an item, and Escape, Tab or a click elsewhere closes it, giving the focus back.
 */
export function ContextMenuLayer() {
  const menu = useContextMenu((s) => s.menu);
  return menu ? <Menu key={`${menu.x},${menu.y},${menu.label}`} menu={menu} /> : null;
}

function Menu({ menu }: { menu: OpenMenu }) {
  const box = useRef<HTMLDivElement>(null);
  const [place, setPlace] = useState({ left: menu.x, top: menu.y });

  const close = (refocus = true) => {
    useContextMenu.getState().close();
    if (refocus && menu.opener?.isConnected) menu.opener.focus({ preventScroll: true });
  };
  const closeRef = useRef(close);
  closeRef.current = close;

  // Kept inside the window.
  useLayoutEffect(() => {
    const el = box.current;
    if (!el) return;
    const { width, height } = el.getBoundingClientRect();
    setPlace({
      left: Math.max(EDGE, Math.min(menu.x, window.innerWidth - width - EDGE)),
      top: Math.max(EDGE, Math.min(menu.y, window.innerHeight - height - EDGE)),
    });
  }, [menu]);

  useEffect(() => {
    box.current?.querySelector<HTMLButtonElement>("[role=menuitem]:not(:disabled)")?.focus({ preventScroll: true });
    const onKey = (e: globalThis.KeyboardEvent) => {
      if (e.key !== "Escape") return;
      // Ahead of the screens' keys, so Escape doesn't clear the selection too.
      e.preventDefault();
      e.stopPropagation();
      closeRef.current();
    };
    const onDown = (e: PointerEvent) => {
      if (!box.current?.contains(e.target as Node)) closeRef.current(false);
    };
    const onAway = () => closeRef.current(false);
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("pointerdown", onDown, true);
    window.addEventListener("blur", onAway);
    window.addEventListener("resize", onAway);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("pointerdown", onDown, true);
      window.removeEventListener("blur", onAway);
      window.removeEventListener("resize", onAway);
    };
  }, []);

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    // The menu has the keys: Delete or an arrow here doesn't reach the screen behind it.
    e.stopPropagation();
    const items = [...(box.current?.querySelectorAll<HTMLButtonElement>("[role=menuitem]:not(:disabled)") ?? [])];
    const at = items.indexOf(document.activeElement as HTMLButtonElement);
    const go = (i: number) => items[(i + items.length) % items.length]?.focus();
    if (e.key === "ArrowDown") go(at + 1);
    else if (e.key === "ArrowUp") go(at < 0 ? items.length - 1 : at - 1);
    else if (e.key === "Home") go(0);
    else if (e.key === "End") go(items.length - 1);
    else if (e.key === "Tab") close();
    else return;
    e.preventDefault();
  };

  return (
    <div
      ref={box}
      role="menu"
      aria-label={menu.label}
      onKeyDown={onKeyDown}
      onContextMenu={(e) => e.preventDefault()}
      style={{ left: place.left, top: place.top }}
      className="fixed z-[60] flex min-w-48 flex-col rounded-lg border border-neutral-200 bg-white p-1 text-sm text-neutral-800 shadow-xl dark:border-neutral-800 dark:bg-neutral-900 dark:text-neutral-100"
    >
      {menu.items.map((item) => (
        <Fragment key={item.label}>
          {item.separated && <div role="separator" className="my-1 h-px bg-neutral-200 dark:bg-neutral-800" />}
          <button
            type="button"
            role="menuitem"
            disabled={item.disabled}
            className={`flex items-center justify-between gap-6 rounded px-2 py-1 text-left hover:bg-neutral-100 focus:bg-neutral-100 focus:outline-none disabled:opacity-40 disabled:hover:bg-transparent dark:hover:bg-neutral-800 dark:focus:bg-neutral-800 ${
              item.danger ? "text-red-600 dark:text-red-400" : ""
            }`}
            onClick={() => {
              close();
              void item.run();
            }}
          >
            <span>{item.label}</span>
            {item.shortcut && <kbd className="font-sans text-xs text-neutral-400">{hintFor(item.shortcut)}</kbd>}
          </button>
        </Fragment>
      ))}
    </div>
  );
}
