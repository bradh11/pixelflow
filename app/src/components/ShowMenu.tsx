import { Check, ChevronDown } from "lucide-react";
import { type KeyboardEvent, type ReactNode, useEffect, useRef, useState } from "react";
import { fileName, shownPath } from "../lib/format";
import { useApp } from "../state/store";
import { thumbnailUrl } from "./RecentShows";

/** How many recent shows the menu lists (the start page lists them all). */
const MENU_RECENT = 8;

function Item({
  children,
  shortcut,
  onSelect,
  current,
  recent,
}: {
  children: ReactNode;
  shortcut?: string;
  onSelect: () => void;
  current?: boolean;
  recent?: boolean;
}) {
  return (
    <button
      type="button"
      role="menuitem"
      data-recent-item={recent ? "" : undefined}
      aria-current={current ? "true" : undefined}
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

/** The show's name in the top bar: renames it in place (double-click), and opens the show menu. */
function NameField() {
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
          done.current = true;
          void renameShow(draft);
        } else if (e.key === "Escape") {
          e.preventDefault();
          done.current = true;
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
  const open = menu !== "closed";

  useEffect(() => {
    if (!open) return;
    void useApp.getState().refreshRecent();
    const items = panel.current?.querySelectorAll<HTMLElement>('[role="menuitem"]');
    const first = menu === "recent" ? panel.current?.querySelector<HTMLElement>("[data-recent-item]") : null;
    (first ?? items?.[0])?.focus();
    const onPointer = (e: PointerEvent) => {
      const target = e.target as Node;
      if (!panel.current?.contains(target) && !trigger.current?.contains(target)) useApp.getState().setShowMenu("closed");
    };
    window.addEventListener("pointerdown", onPointer);
    return () => window.removeEventListener("pointerdown", onPointer);
  }, [open, menu]);

  if (!snapshot) return null;
  if (renaming) return <NameField />;

  const close = (refocus = true) => {
    app.setShowMenu("closed");
    if (refocus) trigger.current?.focus();
  };
  const run = (action: () => unknown) => () => {
    close(false);
    void action();
  };
  const onKeyDown = (e: KeyboardEvent) => {
    const items = Array.from(panel.current?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? []);
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
        onDoubleClick={() => app.setRenaming(true)}
        className="flex min-w-0 items-center gap-1 rounded-md px-1.5 py-0.5 font-medium hover:bg-neutral-200/70 dark:hover:bg-neutral-800"
      >
        <span className="truncate">{snapshot.show.name}</span>
        <ChevronDown size={14} className="shrink-0 text-neutral-500" aria-hidden />
      </button>
      {open && (
        <div
          ref={panel}
          role="menu"
          aria-label="Show"
          onKeyDown={onKeyDown}
          className="absolute left-0 top-full z-40 mt-1 w-80 max-w-[calc(100vw-2rem)] rounded-xl border border-neutral-200 bg-white p-1.5 shadow-xl dark:border-neutral-800 dark:bg-neutral-900"
        >
          <div className="px-2.5 pb-2 pt-1">
            <div className="truncate text-sm font-semibold">{snapshot.show.name}</div>
            <div className="truncate text-xs text-neutral-500 dark:text-neutral-400">
              {snapshot.path ? shownPath(snapshot.path) : "(not saved yet)"}
            </div>
          </div>
          <Separator />
          <div role="group" aria-label="Recent shows">
            <div className="px-2.5 pb-0.5 pt-1.5 text-xs font-semibold uppercase tracking-wide text-neutral-500">Recent shows</div>
            {shown.length === 0 && <p className="px-2.5 py-1.5 text-sm text-neutral-500">Shows you open or save will be listed here.</p>}
            {shown.map((show) => {
              const current = show.path === snapshot.path;
              const missing = show.status === "missing";
              return (
                <Item
                  key={show.path}
                  recent
                  current={current}
                  onSelect={current ? () => close() : run(() => (missing ? app.locateRecent(show.path) : app.openRecent(show.path)))}
                >
                  <span className="flex h-6 w-9 shrink-0 items-center justify-center overflow-hidden rounded bg-neutral-900 dark:bg-black">
                    {show.thumbnail && <img src={thumbnailUrl(show.thumbnail)} alt="" className="h-full w-full object-contain" />}
                  </span>
                  <span className={`min-w-0 flex-1 truncate ${missing ? "text-neutral-400 line-through decoration-neutral-400/60" : ""}`} title={shownPath(show.path)}>
                    {show.name}
                    <span className="ml-1.5 text-xs text-neutral-500">{missing ? "moved — Locate…" : fileName(show.path)}</span>
                  </span>
                  {current && <Check size={14} className="shrink-0 text-accent-600 dark:text-accent-400" aria-label="Open now" />}
                </Item>
              );
            })}
          </div>
          <Separator />
          <Item shortcut="⌘O" onSelect={run(app.openShow)}>
            Open…
          </Item>
          <Item shortcut="⌘N" onSelect={run(app.newShow)}>
            New show
          </Item>
          <Item onSelect={run(app.importXlights)}>Import from xLights…</Item>
          <Separator />
          <Item onSelect={() => app.setRenaming(true)}>Rename…</Item>
          <Item shortcut="⌘S" onSelect={run(app.save)}>
            Save
          </Item>
          <Item shortcut="⇧⌘S" onSelect={run(app.saveAs)}>
            Save As…
          </Item>
          <Separator />
          <Item shortcut="⌘W" onSelect={run(app.closeShow)}>
            Close show
          </Item>
        </div>
      )}
    </div>
  );
}
