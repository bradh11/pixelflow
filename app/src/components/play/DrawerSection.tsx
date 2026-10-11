import { ChevronDown, ChevronRight } from "lucide-react";
import { type ReactNode, useId, useState } from "react";

const KEY = "pixelflow.playSections";

function loadFolded(): Record<string, boolean> {
  try {
    const saved = JSON.parse(localStorage.getItem(KEY) ?? "{}") as unknown;
    return saved && typeof saved === "object" ? (saved as Record<string, boolean>) : {};
  } catch {
    return {};
  }
}

function saveFolded(id: string, folded: boolean) {
  try {
    localStorage.setItem(KEY, JSON.stringify({ ...loadFolded(), [id]: folded }));
  } catch {
    // Storage unavailable: it stays as it is until the screen closes.
  }
}

/**
 * One part of the Play screen's playlist drawer, under a heading that folds it away (remembered
 * on this computer by `id`). `actions` sit at the end of the heading's row.
 */
export function DrawerSection({ id, title, actions, children }: { id: string; title: string; actions?: ReactNode; children: ReactNode }) {
  const [open, setOpen] = useState(() => loadFolded()[id] !== true);
  const region = useId();
  return (
    <section aria-label={title} className="border-b border-neutral-200 last:border-b-0 dark:border-neutral-800">
      <div className="flex min-h-10 items-center gap-2 px-3 py-1.5">
        <button
          type="button"
          aria-expanded={open}
          aria-controls={region}
          onClick={() => {
            saveFolded(id, open);
            setOpen(!open);
          }}
          className="flex min-w-0 flex-1 items-center gap-1 rounded text-left text-sm font-semibold"
        >
          {open ? <ChevronDown size={14} className="shrink-0" aria-hidden /> : <ChevronRight size={14} className="shrink-0" aria-hidden />}
          <span className="truncate">{title}</span>
        </button>
        {actions}
      </div>
      <div id={region} hidden={!open} className="flex flex-col gap-3 px-3 pb-3">
        {open && children}
      </div>
    </section>
  );
}
