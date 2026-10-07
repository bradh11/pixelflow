import { PanelLeftClose, PanelLeftOpen } from "lucide-react";
import { useRef, useState } from "react";
import { useLayoutEditor } from "../../state/layoutEditor";
import { useApp } from "../../state/store";
import { GroupsPanel } from "./GroupsPanel";
import { PropsList } from "./PropsList";

const WIDTH_KEY = "pixelflow.layoutSidePanelWidth";
const DEFAULT_WIDTH = 256;
export const MIN_WIDTH = 200;
export const MAX_WIDTH = 520;
const STEP = 16;

const clampWidth = (w: number) => Math.round(Math.max(MIN_WIDTH, Math.min(MAX_WIDTH, w)));

function storedWidth(): number {
  try {
    const saved = Number(localStorage.getItem(WIDTH_KEY));
    return saved > 0 ? clampWidth(saved) : DEFAULT_WIDTH;
  } catch {
    return DEFAULT_WIDTH;
  }
}

function saveWidth(width: number) {
  try {
    localStorage.setItem(WIDTH_KEY, String(width));
  } catch {
    // Storage unavailable: the width still applies until the screen closes.
  }
}

/** The props list and groups beside the canvas: resizable from its edge, and folds away. */
export function SidePanel() {
  const { open, tab } = useLayoutEditor((s) => s.sidePanel);
  const setSidePanel = useLayoutEditor((s) => s.setSidePanel);
  const props = useApp((s) => s.snapshot?.show.props.length ?? 0);
  const groups = useApp((s) => s.snapshot?.show.groups.length ?? 0);
  const [width, setWidth] = useState(storedWidth);
  const resizing = useRef<{ startX: number; from: number } | null>(null);

  if (!open) {
    return (
      <div className="flex w-9 shrink-0 flex-col items-center rounded-lg border border-neutral-200 bg-white py-1 dark:border-neutral-800 dark:bg-neutral-900">
        <button
          type="button"
          aria-label="Show the props and groups list"
          title="Show the props and groups list"
          className="rounded p-1.5 text-neutral-600 hover:bg-neutral-200/70 dark:text-neutral-300 dark:hover:bg-neutral-800"
          onClick={() => setSidePanel({ open: true })}
        >
          <PanelLeftOpen size={16} aria-hidden />
        </button>
      </div>
    );
  }

  const tabClass = (on: boolean) =>
    `flex-1 rounded-md px-2 py-1 text-sm ${on ? "bg-accent-50 font-medium text-accent-700 dark:bg-accent-600/15 dark:text-accent-300" : "text-neutral-600 hover:bg-neutral-100 dark:text-neutral-300 dark:hover:bg-neutral-800"}`;
  const resize = (next: number) => {
    const w = clampWidth(next);
    setWidth(w);
    return w;
  };
  return (
    <aside
      aria-label="Props and groups"
      style={{ width }}
      className="relative flex shrink-0 flex-col overflow-hidden rounded-lg border border-neutral-200 bg-white dark:border-neutral-800 dark:bg-neutral-900"
    >
      <div role="tablist" aria-label="Props or groups" className="flex items-center gap-1 border-b border-neutral-200 p-1 dark:border-neutral-800">
        <button type="button" role="tab" aria-selected={tab === "props"} className={tabClass(tab === "props")} onClick={() => setSidePanel({ tab: "props" })}>
          Props <span className="text-xs text-neutral-500 tabular-nums">{props}</span>
        </button>
        <button type="button" role="tab" aria-selected={tab === "groups"} className={tabClass(tab === "groups")} onClick={() => setSidePanel({ tab: "groups" })}>
          Groups <span className="text-xs text-neutral-500 tabular-nums">{groups}</span>
        </button>
        <button
          type="button"
          aria-label="Fold the list away"
          title="Fold the list away"
          className="rounded p-1.5 text-neutral-500 hover:bg-neutral-200/70 dark:hover:bg-neutral-800"
          onClick={() => setSidePanel({ open: false })}
        >
          <PanelLeftClose size={15} aria-hidden />
        </button>
      </div>
      <div role="tabpanel" aria-label={tab === "props" ? "Props" : "Groups"} className="flex min-h-0 flex-1 flex-col">
        {tab === "props" ? <PropsList /> : <GroupsPanel />}
      </div>
      <div
        role="separator"
        aria-orientation="vertical"
        aria-label="Width of the props list"
        aria-valuemin={MIN_WIDTH}
        aria-valuemax={MAX_WIDTH}
        aria-valuenow={width}
        tabIndex={0}
        title="Drag to resize (double-click to reset)"
        className="absolute top-0 right-0 bottom-0 w-1.5 cursor-col-resize touch-none outline-none hover:bg-accent-400/40 focus-visible:bg-accent-400/40"
        onPointerDown={(e) => {
          if (e.button !== 0) return;
          e.currentTarget.setPointerCapture?.(e.pointerId);
          resizing.current = { startX: e.clientX, from: width };
        }}
        onPointerMove={(e) => {
          const r = resizing.current;
          if (r) resize(r.from + e.clientX - r.startX);
        }}
        onPointerUp={(e) => {
          const r = resizing.current;
          resizing.current = null;
          if (r) saveWidth(resize(r.from + e.clientX - r.startX));
        }}
        onPointerCancel={() => {
          const r = resizing.current;
          resizing.current = null;
          if (r) setWidth(r.from);
        }}
        onDoubleClick={() => saveWidth(resize(DEFAULT_WIDTH))}
        onKeyDown={(e) => {
          if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
          // The arrows resize the list here, not nudge the selection.
          e.preventDefault();
          e.stopPropagation();
          saveWidth(resize(width + (e.key === "ArrowRight" ? STEP : -STEP)));
        }}
      />
    </aside>
  );
}
