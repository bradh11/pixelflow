import { FileQuestion, Lightbulb, X } from "lucide-react";
import type { RecentShow } from "../api/types";
import { ago, plural, shownPath } from "../lib/format";
import { folderOf } from "../lib/showFiles";
import { useApp } from "../state/store";

/** The shell's SVG thumbnail as an image address (an <img> never runs anything in it). */
export function thumbnailUrl(svg: string): string {
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}

/** "4 props · 1,234 pixels · 2 controllers". */
export function showCounts(show: RecentShow): string {
  return [plural(show.props, "prop"), plural(show.pixels, "pixel"), plural(show.controllers, "controller")].join(" · ");
}

function Thumbnail({ show, className }: { show: RecentShow; className: string }) {
  return (
    <span className={`flex shrink-0 items-center justify-center overflow-hidden rounded-md bg-neutral-900 ring-1 ring-black/5 dark:bg-black dark:ring-white/10 ${className}`}>
      {show.thumbnail ? (
        <img src={thumbnailUrl(show.thumbnail)} alt="" className="h-full w-full object-contain" />
      ) : show.status === "missing" ? (
        <FileQuestion size={20} className="text-neutral-500" aria-hidden />
      ) : (
        <Lightbulb size={20} className="text-amber-300/70" aria-hidden />
      )}
    </span>
  );
}

/**
 * One recent show. Click or Enter opens it (through the same open as the Open dialog). A show
 * whose file is gone stays, greyed, with Locate… and Remove from list.
 */
export function RecentShowCard({ show, now = Date.now() }: { show: RecentShow; now?: number }) {
  const { openRecent, locateRecent, forgetRecent } = useApp.getState();
  const busy = useApp((s) => s.opening !== null);
  const folder = shownPath(folderOf(show.path));
  const missing = show.status === "missing";
  const details = (
    <span className="min-w-0 flex-1">
      <span className="block truncate font-medium">{show.name}</span>
      <span className="block truncate text-xs text-neutral-500 dark:text-neutral-400" title={shownPath(show.path)}>
        {folder || shownPath(show.path)}
      </span>
      <span className="mt-0.5 block text-xs text-neutral-500 sm:truncate dark:text-neutral-400">
        {missing ? "Moved or deleted" : `Opened ${ago(show.openedAt, now)}`} · {showCounts(show)}
      </span>
    </span>
  );
  if (missing) {
    return (
      <li className="flex flex-wrap items-center gap-3 rounded-xl border border-dashed border-neutral-300 p-3 dark:border-neutral-700" aria-label={`${show.name} (moved or deleted)`}>
        <span className="flex min-w-[14rem] flex-1 items-center gap-3 opacity-60">
          <Thumbnail show={show} className="h-14 w-[5.5rem]" />
          {details}
        </span>
        <span className="ml-auto flex gap-1">
          <button
            type="button"
            disabled={busy}
            onClick={() => void locateRecent(show.path)}
            aria-label={`Locate ${show.name}…`}
            className="rounded-md border border-neutral-300 px-2.5 py-1 text-sm font-medium hover:bg-neutral-100 disabled:opacity-50 dark:border-neutral-700 dark:hover:bg-neutral-800"
          >
            Locate…
          </button>
          <button
            type="button"
            onClick={() => void forgetRecent(show.path)}
            aria-label={`Remove ${show.name} from the list`}
            className="rounded-md px-2.5 py-1 text-sm text-neutral-600 hover:bg-neutral-100 dark:text-neutral-300 dark:hover:bg-neutral-800"
          >
            Remove from list
          </button>
        </span>
      </li>
    );
  }
  return (
    <li className="group relative">
      <button
        type="button"
        data-recent-show
        disabled={busy}
        onClick={() => void openRecent(show.path)}
        aria-label={`Open ${show.name}, ${folder}, opened ${ago(show.openedAt, now)}, ${showCounts(show)}`}
        className="flex w-full items-center gap-3 rounded-xl border border-neutral-200 bg-white p-3 pr-10 text-left transition hover:border-accent-500 hover:shadow-md focus-visible:border-accent-500 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-500/40 disabled:cursor-wait dark:border-neutral-800 dark:bg-neutral-900"
      >
        <Thumbnail show={show} className="h-14 w-[5.5rem]" />
        {details}
      </button>
      <button
        type="button"
        onClick={() => void forgetRecent(show.path)}
        aria-label={`Remove ${show.name} from the list`}
        title="Remove from list"
        className="absolute right-2 top-2 rounded p-1 text-neutral-400 opacity-0 hover:bg-neutral-100 hover:text-neutral-700 focus-visible:opacity-100 group-hover:opacity-100 dark:hover:bg-neutral-800 dark:hover:text-neutral-200"
      >
        <X size={14} />
      </button>
    </li>
  );
}
