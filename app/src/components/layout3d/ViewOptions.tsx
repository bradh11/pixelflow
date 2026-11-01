import { SlidersHorizontal } from "lucide-react";
import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useShallow } from "zustand/react/shallow";
import { gridWords } from "../../lib/layout3d";
import { BOOST_CHOICES, DOT_CHOICES, UNLIT_CHOICES, type Unlit, VIEW_DEFAULTS } from "../../lib/viewOptions";
import { useView3d } from "../../state/view3d";

/** What each choice of Unlit pixels does, under the choices. */
const UNLIT_HINTS: Record<Unlit, string> = {
  dim: "Shown dim, so every prop can be seen.",
  hidden: "Only lit pixels show, as at night.",
  auto: "Hidden while the show plays; dim while it's paused or stopped, and on the Layout screen.",
};

/** The panel keeps this far from the window's edges, and this far from its button (CSS pixels). */
const EDGE = 8;
const GAP = 4;
const PANEL_WIDTH = 272;

/** A row of choices, one of them picked: radio buttons, so the arrow keys move between them. */
function Choices<T extends string | number>({ legend, name, choices, value, onChange }: { legend: string; name: string; choices: { value: T; label: string }[]; value: T; onChange: (value: T) => void }) {
  return (
    <fieldset className="flex flex-col gap-1">
      <legend className="mb-1 text-xs text-neutral-500 dark:text-neutral-400">{legend}</legend>
      <div className="flex rounded-md border border-neutral-200 p-0.5 dark:border-neutral-700">
        {choices.map((c) => (
          <label key={c.value} className="flex-1">
            <input type="radio" name={name} className="peer sr-only" checked={c.value === value} onChange={() => onChange(c.value)} />
            <span className="block cursor-pointer rounded px-1.5 py-1 text-center text-xs text-neutral-600 peer-checked:bg-accent-500 peer-checked:text-white peer-focus-visible:ring-2 peer-focus-visible:ring-accent-400 hover:bg-neutral-100 peer-checked:hover:bg-accent-500 dark:text-neutral-300 dark:hover:bg-neutral-800">
              {c.label}
            </span>
          </label>
        ))}
      </div>
    </fieldset>
  );
}

/** A switch for one option, with what it does. */
function Toggle({ label, hint, checked, onChange }: { label: string; hint?: string; checked: boolean; onChange: (on: boolean) => void }) {
  return (
    <label className="flex items-start gap-2 text-xs">
      <input type="checkbox" className="mt-0.5 accent-accent-500" checked={checked} onChange={(e) => onChange(e.target.checked)} />
      <span className="flex flex-col">
        <span>{label}</span>
        {hint && <span className="text-neutral-500 dark:text-neutral-400">{hint}</span>}
      </span>
    </label>
  );
}

/**
 * How this viewer likes to look at the lights, beside Glow: one View button opening its options.
 * With `grid` (the 3D view), the grid on the ground too. Every preview, flat and 3D, follows
 * them, and they're remembered on this computer, never in the show.
 *
 * `tone`: "scene" for a button over a view (always dark, like the 3D view's tool bar), "panel"
 * for one on a tool bar of the app's own. `iconOnly`: no word on the button, for a narrow place.
 * The options open in a panel of their own over everything, so a small preview doesn't cut them off.
 */
export function ViewOptionsControl({ grid = false, tone = "panel", iconOnly = false }: { grid?: boolean; tone?: "scene" | "panel"; iconOnly?: boolean }) {
  const options = useView3d(
    useShallow((s) => ({ grid: s.grid, names: s.names, unlit: s.unlit, dotSize: s.dotSize, boost: s.boost, floor: s.floor, set: s.setViewOptions })),
  );
  const [open, setOpen] = useState(false);
  const [place, setPlace] = useState<{ left: number; top: number; maxHeight: number } | null>(null);
  const box = useRef<HTMLDivElement>(null);
  const button = useRef<HTMLButtonElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const id = useId();

  // Under the button, or over it where there's more room there; inside the window either way.
  useLayoutEffect(() => {
    if (!open) return;
    const fit = () => {
      const at = button.current?.getBoundingClientRect();
      if (!at) return;
      const height = panel.current?.offsetHeight ?? 0;
      const [below, above] = [window.innerHeight - at.bottom - GAP - EDGE, at.top - GAP - EDGE];
      const down = height <= below || below >= above;
      const room = Math.max(120, down ? below : above);
      const left = Math.max(EDGE, Math.min(at.left, window.innerWidth - PANEL_WIDTH - EDGE));
      setPlace({ left, top: down ? at.bottom + GAP : at.top - GAP - Math.min(height, room), maxHeight: room });
    };
    fit();
    window.addEventListener("resize", fit);
    return () => window.removeEventListener("resize", fit);
  }, [open]);

  useEffect(() => {
    if (!open) return;
    panel.current?.querySelector<HTMLElement>("input")?.focus();
    // Ahead of the screen's own keys, so Escape only closes the panel.
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      setOpen(false);
      button.current?.focus();
    };
    const onDown = (e: PointerEvent) => {
      const target = e.target as Node;
      if (!box.current?.contains(target) && !panel.current?.contains(target)) setOpen(false);
    };
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("pointerdown", onDown);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("pointerdown", onDown);
    };
  }, [open]);

  const changed = options.names || options.unlit !== VIEW_DEFAULTS.unlit || options.dotSize !== VIEW_DEFAULTS.dotSize || options.boost !== VIEW_DEFAULTS.boost;
  const look =
    tone === "scene"
      ? `rounded px-2 py-1 text-xs hover:bg-white/10 aria-expanded:bg-white/15 ${changed ? "text-accent-400" : "text-neutral-200 aria-expanded:text-white"}`
      : `rounded-md px-1.5 py-1 text-xs hover:bg-neutral-200/70 dark:hover:bg-neutral-800 ${changed ? "text-accent-600 dark:text-accent-400" : "text-neutral-600 dark:text-neutral-300"}`;
  const { floor } = options;

  return (
    <div ref={box} className="relative inline-flex">
      <button
        ref={button}
        type="button"
        aria-haspopup="dialog"
        aria-expanded={open}
        title={grid ? "View options: the grid, prop names, pixels that are off, dot size, and boost" : "View options: prop names, pixels that are off, dot size, and boost"}
        onClick={() => setOpen(!open)}
        className={`inline-flex items-center gap-1 ${look}`}
      >
        <SlidersHorizontal size={tone === "scene" ? 12 : 14} aria-hidden />
        <span className={iconOnly ? "sr-only" : undefined}>View</span>
      </button>
      {open &&
        createPortal(
          <div
            ref={panel}
            role="dialog"
            aria-label="View"
            // Tabbing on from the last option (or back from the first) leaves the panel: it closes.
            onBlur={(e) => {
              const to = e.relatedTarget as Node | null;
              if (to && !panel.current?.contains(to) && !box.current?.contains(to)) setOpen(false);
            }}
            style={{ left: place?.left ?? 0, top: place?.top ?? 0, width: PANEL_WIDTH, maxHeight: place?.maxHeight, visibility: place ? "visible" : "hidden" }}
            className="fixed z-50 flex flex-col gap-3 overflow-auto rounded-lg border border-neutral-200 bg-white p-3 text-neutral-800 shadow-xl dark:border-neutral-800 dark:bg-neutral-900 dark:text-neutral-100"
          >
            {grid && (
              <Toggle
                label="Grid on the ground"
                hint={floor ? `Lines every ${gridWords(floor.minor)}, heavier every ${gridWords(floor.major)}.` : undefined}
                checked={options.grid}
                onChange={(on) => options.set({ grid: on })}
              />
            )}
            <Toggle label="Prop names" checked={options.names} onChange={(on) => options.set({ names: on })} />
            <div className="flex flex-col gap-1">
              <Choices legend="Pixels that are off" name={`${id}-unlit`} choices={UNLIT_CHOICES} value={options.unlit} onChange={(unlit) => options.set({ unlit })} />
              <p className="text-xs text-neutral-500 dark:text-neutral-400">{UNLIT_HINTS[options.unlit]}</p>
            </div>
            <Choices legend="Dot size" name={`${id}-dots`} choices={DOT_CHOICES} value={options.dotSize} onChange={(dotSize) => options.set({ dotSize })} />
            <div className="flex flex-col gap-1">
              <Choices legend="Boost dim" name={`${id}-boost`} choices={BOOST_CHOICES} value={options.boost} onChange={(boost) => options.set({ boost })} />
              <p className="text-xs text-neutral-500 dark:text-neutral-400">Lifts dim levels so fades show on a screen. Your lights still get the show's own colors.</p>
            </div>
          </div>,
          document.body,
        )}
    </div>
  );
}
