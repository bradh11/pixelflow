import { Lightbulb, Pause, Play, Repeat, RotateCcw, Square } from "lucide-react";
import { type ReactNode, useRef } from "react";
import { clock } from "../../lib/format";
import { SHOW_ON_LIGHTS, SHOW_ON_LIGHTS_HINT } from "../../lib/labels";
import { hintFor } from "../../lib/shortcuts";

/** A button on the Play screen's bar: `label` names it (and is its tooltip, or `hint` when that says more). */
export function BarButton({
  label,
  hint,
  shortcut,
  pressed,
  expanded,
  disabled,
  primary = false,
  onClick,
  children,
}: {
  label: string;
  hint?: string;
  /** The key that does the same, shown in the tooltip. */
  shortcut?: string;
  pressed?: boolean;
  expanded?: boolean;
  disabled?: boolean;
  primary?: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  const look = primary
    ? "bg-accent-600 text-white hover:bg-accent-500"
    : pressed
      ? "bg-accent-50 text-accent-600 dark:bg-accent-600/15 dark:text-accent-400"
      : "text-neutral-700 hover:bg-neutral-200/70 dark:text-neutral-200 dark:hover:bg-neutral-800";
  return (
    <button
      type="button"
      aria-label={label}
      aria-pressed={pressed}
      aria-expanded={expanded}
      data-tip={hint ?? label}
      data-tip-key={shortcut}
      disabled={disabled}
      onClick={onClick}
      className={`inline-flex h-8 shrink-0 items-center gap-1.5 rounded-md px-2 text-sm disabled:opacity-40 disabled:hover:bg-transparent ${look}`}
    >
      {children}
    </button>
  );
}

/** A switch on the bar that applies to what's playing (looping, the real lights). */
export interface BarSwitch {
  on: boolean;
  set(on: boolean): void;
}

/** How far the seek bar's keys move: arrows a second, Page Up and Down ten. */
const KEY_STEPS: Record<string, number> = { ArrowRight: 1000, ArrowUp: 1000, ArrowLeft: -1000, ArrowDown: -1000, PageUp: 10000, PageDown: -10000 };

/**
 * The Play screen's transport: a seek bar with the time over one row of buttons (play or pause,
 * back to the start, stop, and the switches that apply), the name of what's playing, and the
 * screen's own buttons (`children`) at the end.
 */
export function TransportBar({
  name,
  note = null,
  playing,
  stopLabel = "Stop",
  stopDisabled,
  positionMs,
  durationMs,
  stepMs,
  onToggle,
  onRestart,
  onStop,
  onScrub,
  onScrubEnd,
  loop,
  lights,
  status,
  children,
}: {
  /** What's playing, or would. */
  name: string;
  /** A few words after the name ("finished"). */
  note?: string | null;
  playing: boolean;
  /** What Stop does now, when it isn't stopping. */
  stopLabel?: string;
  stopDisabled: boolean;
  positionMs: number;
  durationMs: number;
  /** How far the seek bar moves at a time while dragged (a frame). */
  stepMs: number;
  onToggle: () => void;
  onRestart: () => void;
  onStop: () => void;
  /** The seek bar was moved to `ms` (dragged, or by key)… */
  onScrub: (ms: number) => void;
  /** …and let go. */
  onScrubEnd: () => void;
  loop?: BarSwitch;
  lights?: BarSwitch;
  /** What the controllers are doing, after the name. */
  status?: ReactNode;
  children?: ReactNode;
}) {
  // Key presses build on each other, not on what was last drawn.
  const scrubbed = useRef<number | null>(null);
  const scrub = (ms: number) => {
    scrubbed.current = ms;
    onScrub(ms);
  };
  const end = () => {
    if (scrubbed.current === null) return;
    scrubbed.current = null;
    onScrubEnd();
  };
  return (
    <section aria-label="Transport" className="@container flex flex-col gap-1 px-3 pt-2 pb-2">
      <div className="flex items-center gap-3">
        {/* Always here, so the keyboard can reach it; when stopped, moving it plays from there. */}
        <input
          type="range"
          aria-label="Position"
          className="min-w-0 flex-1 accent-violet-600"
          min={0}
          max={durationMs}
          step={stepMs}
          disabled={durationMs === 0}
          value={positionMs}
          aria-valuetext={clock(positionMs / 1000)}
          onChange={(e) => scrub(Number(e.target.value))}
          onKeyDown={(e) => {
            // Dragging moves frame by frame; the keyboard jumps a second at a time.
            const here = scrubbed.current ?? positionMs;
            const next = e.key === "Home" ? 0 : e.key === "End" ? durationMs : e.key in KEY_STEPS ? here + KEY_STEPS[e.key] : null;
            if (next === null) return;
            e.preventDefault();
            scrub(Math.min(durationMs, Math.max(0, next)));
          }}
          onPointerUp={end}
          onKeyUp={end}
          onBlur={end}
        />
        <p className="shrink-0 text-xs text-neutral-500 tabular-nums dark:text-neutral-400">
          {clock(positionMs / 1000)} / {clock(durationMs / 1000)}
        </p>
      </div>
      <div className="flex items-center gap-1">
        <BarButton primary label={playing ? "Pause" : "Play"} shortcut={hintFor("play-toggle")} onClick={onToggle}>
          {playing ? <Pause size={16} aria-hidden /> : <Play size={16} aria-hidden />}
        </BarButton>
        <BarButton label="Restart" hint="Back to the start" onClick={onRestart}>
          <RotateCcw size={16} aria-hidden />
        </BarButton>
        <BarButton label={stopLabel} disabled={stopDisabled} onClick={onStop}>
          <Square size={15} aria-hidden />
        </BarButton>
        {(loop || lights) && <span aria-hidden className="mx-1 h-5 w-px shrink-0 bg-neutral-200 dark:bg-neutral-800" />}
        {loop && (
          <BarButton label="Loop playback" shortcut={hintFor("play-loop")} pressed={loop.on} onClick={() => loop.set(!loop.on)}>
            <Repeat size={16} aria-hidden />
          </BarButton>
        )}
        {lights && (
          <BarButton
            label={SHOW_ON_LIGHTS}
            hint={SHOW_ON_LIGHTS_HINT}
            pressed={lights.on}
            onClick={() => lights.set(!lights.on)}
          >
            <Lightbulb size={16} aria-hidden /> <span className="hidden @min-[760px]:inline">{SHOW_ON_LIGHTS}</span>
          </BarButton>
        )}
        <h2 className="ml-2 min-w-0 truncate text-sm font-semibold" title={name}>
          {name}
        </h2>
        {note && <span className="shrink-0 text-xs text-neutral-500 dark:text-neutral-400">{note}</span>}
        {status}
        <span className="ml-auto flex shrink-0 items-center gap-1 pl-2">{children}</span>
      </div>
    </section>
  );
}
