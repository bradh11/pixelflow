import { Check, ChevronDown } from "lucide-react";
import { type KeyboardEvent, type ReactNode, useEffect, useRef, useState } from "react";
import { fileName, shownPath } from "../lib/format";
import { hintFor } from "../lib/shortcuts";
import { useApp } from "../state/store";
import { thumbnailUrl } from "./RecentShows";

/** How many recent shows the menu lists (the start page lists them all). */
const MENU_RECENT = 8;

/** What the menu's items are found by (plain items, and the recent shows' radio items). */
const ITEMS = '[role="menuitem"], [role="menuitemradio"]';

function Item({
  children,
  shortcut,
  onSelect,
  checked,
  recent,
}: {
  children: ReactNode;
  shortcut?: string;
  onSelect: () => void;
  /** A recent show: whether it's the one open now. */
  checked?: boolean;
  recent?: boolean;
}) {
  return (
    <button
      type="button"
      role={recent ? "menuitemradio" : "menuitem"}
      data-recent-item={recent ? "" : undefined}
      aria-checked={recent ? Boolean(checked) : undefined}
      tabIndex={-1}
      onClick={onSelect}
      className="flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-sm outline-none hover:bg-neutral-100 focus:bg-accent-50 focus:text-accent-700 dark:hover:bg-neutral-800 dark:focus:bg-accent-600/20 dark:focus:text-accent-300"
    >
      <span className="flex min-w-0 flex-1 items-center gap-2">{children}</span>
      {shortcut && <kbd className="shrink-0 font-sans text-xs text-neutral-400">{shortcut}</kbd>}
    </button>
  );
}

function Separator() {
  return <div role="separator" className="my-1 border-t border-neutral-200 dark:border-neutral-800" />;
}

/** The show's name in the top bar: renames it in place (double-click), and opens the show menu.
 * `onDone` runs when Enter or Escape finishes the rename. */
function NameField({ onDone }: { onDone: () => void }) {
  const name = useApp((s) => s.snapshot?.show.name ?? "");
  const { renameShow, setRenaming } = useApp.getState();
  const [draft, setDraft] = useState(name);
  const done = useRef(false);
  return (
    <input
      autoFocus
      aria-label="Show name"
      value={draft}
      onFocus={(e) => e.currentTarget.select()}
      onChange={(e) => setDraft(e.target.value)}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          // Not passed on to the name, which gets focus back meanwhile (it would open the menu).
          e.preventDefault();
          done.current = true;
          onDone();
          void renameShow(draft);
        } else if (e.key === "Escape") {
          // Only cancels the rename (not, say, the layout's selection too).
          e.preventDefault();
          e.stopPropagation();
          done.current = true;
          onDone();
          setRenaming(false);
        }
      }}
      onBlur={() => {
        if (!done.current) void renameShow(draft);
      }}
      className="w-56 min-w-0 rounded-md border border-accent-500 bg-white px-2 py-0.5 font-medium outline-none ring-2 ring-accent-500/30 dark:bg-neutral-950"
    />
  );
}

/**
 * The show menu, on the show's name in the top bar: the recent shows (this one marked), Open…,
 * New show, Import from xLights…, Rename…, Save, Save As…, and Close show.
 */
export function ShowMenu() {
  const snapshot = useApp((s) => s.snapshot);
  const menu = useApp((s) => s.showMenu);
  const renaming = useApp((s) => s.renaming);
  const recent = useApp((s) => s.recent);
  const app = useApp.getState();
  const trigger = useRef<HTMLButtonElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  /** Opened with ArrowUp: start at the last item. */
  const fromEnd = useRef(false);
  /** Set when the name should get focus back once it's shown again (after a rename). */
  const refocus = useRef(false);
  const open = menu !== "closed";

  useEffect(() => {
    if (renaming || !refocus.current) return;
    refocus.current = false;
    trigger.current?.focus();
  }, [renaming]);

  useEffect(() => {
    if (!open) return;
    const firstRecent = () => panel.current?.querySelector<HTMLElement>("[data-recent-item]");
    const items = panel.current?.querySelectorAll<HTMLElement>(ITEMS);
    const end = fromEnd.current ? items?.[items.length - 1] : items?.[0];
    fromEnd.current = false;
    ((menu === "recent" ? firstRecent() : null) ?? end)?.focus();
    void useApp
      .getState()
      .refreshRecent()
      .then(() => {
        // Asked for the recent shows before the list had come in.
        const focused = document.activeElement;
        if (menu === "recent" && !focused?.hasAttribute("data-recent-item")) firstRecent()?.focus();
      });
    const onPointer = (e: PointerEvent) => {
      const target = e.target as Node;
      if (!panel.current?.contains(target) && !trigger.current?.contains(target)) useApp.getState().setShowMenu("closed");
    };
    window.addEventListener("pointerdown", onPointer);
    return () => window.removeEventListener("pointerdown", onPointer);
  }, [open, menu]);

  if (!snapshot) return null;
  if (renaming) return <NameField onDone={() => (refocus.current = true)} />;

  const close = (refocusTrigger = true) => {
    app.setShowMenu("closed");
    if (refocusTrigger) trigger.current?.focus();
  };
  /** Closes the menu and runs `action`; once it's done, the name gets focus back unless
   * something else took it (a question, a dialog). */
  const run = (action: () => unknown) => () => {
    close(false);
    void Promise.resolve(action())
      .catch(() => undefined)
      .then(() => {
        const focused = document.activeElement;
        if (!focused || focused === document.body) trigger.current?.focus();
      });
  };
  const onTriggerKey = (e: KeyboardEvent) => {
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    e.preventDefault();
    fromEnd.current = e.key === "ArrowUp";
    app.setShowMenu("open");
  };
  const onKeyDown = (e: KeyboardEvent) => {
    const items = Array.from(panel.current?.querySelectorAll<HTMLElement>(ITEMS) ?? []);
    const at = items.indexOf(document.activeElement as HTMLElement);
    const move = (to: number) => {
      e.preventDefault();
      items[(to + items.length) % items.length]?.focus();
    };
    if (e.key === "ArrowDown") move(at + 1);
    else if (e.key === "ArrowUp") move(at - 1);
    else if (e.key === "Home") move(0);
    else if (e.key === "End") move(items.length - 1);
    else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      close();
    } else if (e.key === "Tab") close(false);
  };
  const shown = recent.slice(0, MENU_RECENT);

  return (
    <div className="relative min-w-0">
      <button
        ref={trigger}
        type="button"
        aria-haspopup="menu"
        aria-expanded={open}
        title={`${snapshot.path ? shownPath(snapshot.path) : "Not saved yet"} — double-click to rename`}
        onClick={() => app.setShowMenu(open ? "closed" : "open")}
        onKeyDown={onTriggerKey}
        onDoubleClick={() => app.setRenaming(true)}
        className="flex min-w-0 items-center gap-1 rounded-md px-1.5 py-0.5 font-medium hover:bg-neutral-200/70 dark:hover:bg-neutral-800"
      >
        <span className="truncate">{snapshot.show.name}</span>
        <ChevronDown size={14} className="shrink-0 text-neutral-500" aria-hidden />
      </button>
      {open && (
        <div
          ref={panel}
          onKeyDown={onKeyDown}
          className="absolute left-0 top-full z-40 mt-1 w-80 max-w-[calc(100vw-2rem)] rounded-xl border border-neutral-200 bg-white p-1.5 shadow-xl dark:border-neutral-800 dark:bg-neutral-900"
        >
          {/* The show and its file, outside the menu itself (which holds only items): its
              description. */}
          <div className="px-2.5 pb-2 pt-1">
            <div className="truncate text-sm font-semibold">{snapshot.show.name}</div>
            <div id="show-menu-path" className="truncate text-xs text-neutral-500 dark:text-neutral-400">
              {snapshot.path ? shownPath(snapshot.path) : "(not saved yet)"}
            </div>
          </div>
          <div role="menu" aria-label="Show" aria-describedby="show-menu-path">
            <Separator />
            <div role="group" aria-labelledby="show-menu-recent">
              <div id="show-menu-recent" aria-hidden="true" className="px-2.5 pb-0.5 pt-1.5 text-xs font-semibold uppercase tracking-wide text-neutral-500">
                Recent shows
              </div>
              {shown.length === 0 && (
                <div role="menuitem" aria-disabled="true" tabIndex={-1} className="px-2.5 py-1.5 text-sm text-neutral-500 outline-none focus:bg-neutral-100 dark:focus:bg-neutral-800">
                  Shows you open or save will be listed here.
                </div>
              )}
              {shown.map((show) => {
                const current = show.path === snapshot.path;
                const missing = show.status === "missing";
                return (
                  <Item
                    key={show.path}
                    recent
                    checked={current}
                    onSelect={current ? () => close() : run(() => (missing ? app.locateRecent(show.path) : app.openRecent(show.path)))}
                  >
                    <span className="flex h-6 w-9 shrink-0 items-center justify-center overflow-hidden rounded bg-neutral-900 dark:bg-black">
                      {show.thumbnail && <img src={thumbnailUrl(show.thumbnail)} alt="" className="h-full w-full object-contain" />}
                    </span>
                    <span className="min-w-0 flex-1 truncate" title={shownPath(show.path)}>
                      <span className={missing ? "text-neutral-400 line-through decoration-neutral-400/60" : ""}>{show.name}</span>
                      <span className="ml-1.5 text-xs text-neutral-500">{missing ? "moved — Locate…" : fileName(show.path)}</span>
                    </span>
                    {current && <Check size={14} className="shrink-0 text-accent-600 dark:text-accent-400" aria-hidden />}
                  </Item>
                );
              })}
            </div>
            <Separator />
            <Item shortcut={hintFor("open")} onSelect={run(app.openShow)}>
              Open…
            </Item>
            <Item shortcut={hintFor("new")} onSelect={run(app.newShow)}>
              New show
            </Item>
            <Item onSelect={run(app.importXlights)}>Import from xLights…</Item>
            <Separator />
            <Item onSelect={() => app.setRenaming(true)}>Rename…</Item>
            <Item shortcut={hintFor("save")} onSelect={run(app.save)}>
              Save
            </Item>
            <Item shortcut={hintFor("save-as")} onSelect={run(app.saveAs)}>
              Save As…
            </Item>
            <Separator />
            <Item shortcut={hintFor("close-show")} onSelect={run(app.closeShow)}>
              Close show
            </Item>
          </div>
        </div>
      )}
    </div>
  );
}
