import { X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { SHORTCUT_GROUPS, keysLabel, searchShortcuts } from "../lib/shortcuts";
import { useShortcutSheet } from "../state/shortcutSheet";
import { IconButton } from "./ui";

/** Every keyboard shortcut, grouped by screen and searchable: opened with ? or from the palette. */
export function ShortcutSheet() {
  const open = useShortcutSheet((s) => s.open);
  if (!open) return null;
  return <Sheet />;
}

function Sheet() {
  const close = () => useShortcutSheet.getState().setOpen(false);
  const [query, setQuery] = useState("");
  const search = useRef<HTMLInputElement>(null);
  useEffect(() => {
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    search.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      close();
    };
    // Ahead of the screens' own keys, so Escape doesn't also clear a selection behind the sheet.
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      if (opener?.isConnected) opener.focus();
    };
  }, []);
  const found = searchShortcuts(query);
  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center bg-black/40 pt-[8vh]" onClick={close}>
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="shortcut-sheet-title"
        onClick={(e) => e.stopPropagation()}
        className="flex max-h-[80vh] w-full max-w-2xl flex-col rounded-xl border border-neutral-200 bg-white shadow-2xl dark:border-neutral-800 dark:bg-neutral-900"
      >
        <div className="flex items-center gap-3 border-b border-neutral-200 px-4 py-3 dark:border-neutral-800">
          <h2 id="shortcut-sheet-title" className="font-semibold">
            Keyboard shortcuts
          </h2>
          <input
            ref={search}
            type="search"
            aria-label="Search shortcuts"
            placeholder="Search…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            className="min-w-0 flex-1 rounded-md border border-neutral-300 bg-white px-2 py-1 text-sm dark:border-neutral-700 dark:bg-neutral-950"
          />
          <IconButton label="Close" onClick={close} className="rounded-md p-1 text-neutral-500 hover:bg-neutral-200/70 dark:hover:bg-neutral-800">
            <X size={16} aria-hidden />
          </IconButton>
        </div>
        <div className="overflow-auto px-4 py-3">
          {found.length === 0 && <p className="py-6 text-center text-sm text-neutral-500">No shortcuts match “{query}”.</p>}
          <div className="grid gap-x-8 gap-y-4 sm:grid-cols-2">
            {SHORTCUT_GROUPS.map((group) => {
              const list = found.filter((s) => s.group === group);
              if (list.length === 0) return null;
              return (
                <section key={group} aria-label={group}>
                  <h3 className="mb-1 text-xs font-semibold tracking-wide text-neutral-500 uppercase">{group}</h3>
                  <dl className="flex flex-col">
                    {list.map((s) => (
                      <div key={s.id} className="flex items-baseline justify-between gap-3 border-b border-neutral-100 py-1 text-sm last:border-0 dark:border-neutral-800">
                        <dt className="text-neutral-700 dark:text-neutral-200">{s.label}</dt>
                        <dd className="shrink-0">
                          <kbd className="rounded border border-neutral-300 bg-neutral-50 px-1.5 py-0.5 font-sans text-xs whitespace-nowrap text-neutral-600 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-300">
                            {keysLabel(s)}
                          </kbd>
                        </dd>
                      </div>
                    ))}
                  </dl>
                </section>
              );
            })}
          </div>
          <p className="mt-3 text-xs text-neutral-500">⌘ is Ctrl on Windows and Linux.</p>
        </div>
      </div>
    </div>
  );
}
