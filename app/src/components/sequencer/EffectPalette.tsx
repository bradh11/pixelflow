import {
  Activity,
  ArrowUpFromLine,
  AudioLines,
  BarChart3,
  Bomb,
  Blend,
  Blinds,
  CircleDot,
  CloudLightning,
  CloudSnow,
  Fan,
  FerrisWheel,
  Flame,
  Flower2,
  Grid3x3,
  HeartPulse,
  Image as ImageIcon,
  Lightbulb,
  LightbulbOff,
  type LucideIcon,
  MicVocal,
  Orbit,
  PersonStanding,
  Rainbow,
  Ribbon,
  Shapes,
  Sparkle,
  Sparkles,
  Sprout,
  SwatchBook,
  Sunrise,
  Tornado,
  Type,
  Waves,
  Waypoints,
  Zap,
  MoveRight,
  Smile,
  Snowflake,
} from "lucide-react";
import { type PointerEvent as ReactPointerEvent, useEffect, useRef } from "react";
import { create } from "zustand";
import type { EffectKind } from "../../api/sequence";
import { useSequencer } from "../../state/sequencer";

export const EFFECT_ICONS: Record<EffectKind, LucideIcon> = {
  on: Lightbulb,
  off: LightbulbOff,
  colorWash: Rainbow,
  fade: Sunrise,
  chase: MoveRight,
  bars: BarChart3,
  wave: Waves,
  twinkle: Sparkles,
  shimmer: Sparkle,
  strobe: Zap,
  spiral: Tornado,
  fire: Flame,
  meteors: Snowflake,
  ripple: CircleDot,
  shape: Shapes,
  fan: Fan,
  morph: ArrowUpFromLine,
  circles: Orbit,
  pinwheel: FerrisWheel,
  snowflakes: CloudSnow,
  plasma: Blend,
  butterfly: Flower2,
  garlands: Ribbon,
  lines: Waypoints,
  life: Grid3x3,
  tendril: Sprout,
  text: Type,
  faces: Smile,
  vuMeter: AudioLines,
  impact: Bomb,
  wipe: Blinds,
  lightning: CloudLightning,
  pulse: HeartPulse,
  sing: MicVocal,
  colorShift: SwatchBook,
  dancer: PersonStanding,
  picture: ImageIcon,
};

/** Drags this far (screen pixels) before a press on the palette becomes a drag. */
const DRAG_PX = 4;

/** An effect being dragged from the palette, and where the timeline can take it. */
interface PaletteDrag {
  kind: EffectKind | null;
  x: number;
  y: number;
  /** Alt (Option) is held: no snapping. */
  alt: boolean;
  /** Set by the timeline: places the effect if (x, y) is over a row, and says whether it did. */
  drop: ((kind: EffectKind, x: number, y: number, alt: boolean) => boolean) | null;
  /** Set by the timeline: adds the effect at the playhead on the chosen row (keyboard). */
  addAtPlayhead: ((kind: EffectKind) => void) | null;
}

export const usePaletteDrag = create<PaletteDrag>(() => ({ kind: null, x: 0, y: 0, alt: false, drop: null, addAtPlayhead: null }));

/** The effect kinds, to drag onto a row of the timeline (or press Enter to add at the playhead). */
export function EffectPalette({ compact = false }: { compact?: boolean }) {
  const catalog = useSequencer((s) => s.catalog);
  const dragging = usePaletteDrag((s) => s.kind);
  const press = useRef<{ kind: EffectKind; x: number; y: number; moved: boolean } | null>(null);

  // Escape lets go of an effect being dragged without adding it.
  useEffect(() => {
    if (!dragging) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      press.current = null;
      usePaletteDrag.setState({ kind: null });
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [dragging]);

  const onPointerDown = (kind: EffectKind, e: ReactPointerEvent<HTMLButtonElement>) => {
    if (e.button !== 0) return;
    press.current = { kind, x: e.clientX, y: e.clientY, moved: false };
    e.currentTarget.setPointerCapture?.(e.pointerId);
  };
  const onPointerMove = (e: ReactPointerEvent<HTMLButtonElement>) => {
    const p = press.current;
    if (!p) return;
    if (!p.moved && Math.hypot(e.clientX - p.x, e.clientY - p.y) < DRAG_PX) return;
    p.moved = true;
    usePaletteDrag.setState({ kind: p.kind, x: e.clientX, y: e.clientY, alt: e.altKey });
  };
  const onPointerUp = (e: ReactPointerEvent<HTMLButtonElement>) => {
    const p = press.current;
    press.current = null;
    if (!p?.moved) return;
    const { drop } = usePaletteDrag.getState();
    usePaletteDrag.setState({ kind: null });
    drop?.(p.kind, e.clientX, e.clientY, e.altKey);
  };
  const onPointerCancel = () => {
    press.current = null;
    usePaletteDrag.setState({ kind: null });
  };

  return (
    <aside
      aria-label="Effects"
      data-compact={compact || undefined}
      className={`flex shrink-0 flex-col border-r border-neutral-200 dark:border-neutral-800 ${compact ? "w-12" : "w-40"}`}
    >
      {compact ? (
        <h2 className="sr-only">Effects</h2>
      ) : (
        <>
          <h2 className="px-3 pt-3 pb-1 text-xs font-semibold tracking-wide text-neutral-500 uppercase">Effects</h2>
          <p className="px-3 pb-2 text-xs text-neutral-500">Drag onto a row (Escape cancels), or press Enter to add at the playhead.</p>
        </>
      )}
      <ul className="flex-1 overflow-auto px-1.5 pb-2">
        {catalog.map((info) => {
          const Icon = EFFECT_ICONS[info.kind] ?? Activity;
          return (
            <li key={info.kind}>
              <button
                type="button"
                data-tip={compact ? `${info.label}: ${info.description} Drag onto a row, or press Enter to add at the playhead.` : info.description}
                aria-label={`${info.label} effect`}
                className={`flex w-full cursor-grab touch-none items-center gap-2 rounded-md py-1.5 text-left text-sm select-none hover:bg-neutral-200/70 dark:hover:bg-neutral-800 ${
                  compact ? "justify-center px-0" : "px-2"
                } ${
                  dragging === info.kind ? "bg-accent-50 dark:bg-accent-600/15" : ""
                }`}
                onPointerDown={(e) => onPointerDown(info.kind, e)}
                onPointerMove={onPointerMove}
                onPointerUp={onPointerUp}
                onPointerCancel={onPointerCancel}
                onKeyDown={(e) => {
                  if (e.key !== "Enter" && e.key !== " ") return;
                  e.preventDefault();
                  usePaletteDrag.getState().addAtPlayhead?.(info.kind);
                }}
              >
                <Icon size={15} className="shrink-0 text-accent-600 dark:text-accent-400" />
                {!compact && info.label}
              </button>
            </li>
          );
        })}
      </ul>
      {dragging && <DragGhost />}
    </aside>
  );
}

/** The effect's name following the pointer while it's dragged. */
function DragGhost() {
  const { kind, x, y } = usePaletteDrag();
  const label = useSequencer((s) => s.catalog.find((c) => c.kind === kind)?.label ?? "");
  return (
    <div
      aria-hidden
      className="pointer-events-none fixed z-50 rounded bg-accent-600 px-2 py-0.5 text-xs font-medium text-white shadow-lg"
      style={{ left: x + 12, top: y + 8 }}
    >
      {label}
    </div>
  );
}
