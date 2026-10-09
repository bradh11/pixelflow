import { useEffect } from "react";
import { eventTime } from "../../lib/avSync";
import { nextMark } from "../../lib/tapTiming";
import { formatTime } from "../../lib/timelineMath";
import { TAP_SPEEDS, useLyricTools } from "../../state/lyricTools";
import { useSequencer } from "../../state/sequencer";

/** Keys that tap: Space, or J (under the right hand's first finger). */
const isTapKey = (e: KeyboardEvent) => e.key === " " || e.key === "j" || e.key === "J";

/**
 * Tap timing's panel over the timeline: first the speed and Start, then while it runs the next
 * mark to tap and the keys. It takes the keyboard while it's open: Space or J taps (held down, it
 * sets the end too), Escape finishes (or, before starting, closes it).
 */
export function TapTimingHud({ top }: { top: number }) {
  const tap = useLyricTools((s) => s.tap);
  const track = useSequencer((s) => s.doc?.timingTracks.find((t) => t.id === tap?.track));
  const running = tap?.session != null;

  useEffect(() => {
    const tools = useLyricTools.getState;
    const onDown = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      if (target && ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName)) return;
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        void (tools().tap?.session ? tools().finishTap() : tools().cancelTap());
        return;
      }
      if (!tools().tap?.session || !isTapKey(e) || e.metaKey || e.ctrlKey || e.altKey) return;
      e.preventDefault();
      e.stopPropagation();
      if (!e.repeat) tools().press(eventTime(e));
    };
    const onUp = (e: KeyboardEvent) => {
      if (!tools().tap?.session || !isTapKey(e)) return;
      e.preventDefault();
      e.stopPropagation();
      tools().release(eventTime(e));
    };
    window.addEventListener("keydown", onDown, true);
    window.addEventListener("keyup", onUp, true);
    return () => {
      window.removeEventListener("keydown", onDown, true);
      window.removeEventListener("keyup", onUp, true);
    };
  }, []);

  // Leaving the timeline mid-tap changes nothing.
  useEffect(
    () => () => {
      if (useLyricTools.getState().tap) void useLyricTools.getState().cancelTap();
    },
    [],
  );

  if (!tap || !track) return null;
  const unit = track.kind === "words" ? "word" : "syllable";
  const next = tap.session ? nextMark(tap.session) : null;
  const total = tap.session?.queue.length ?? 0;
  return (
    <div
      role="dialog"
      aria-label="Tap timing"
      className="absolute right-3 z-20 flex w-[22rem] max-w-[calc(100%-1.5rem)] flex-col gap-1.5 rounded-lg border border-accent-500/60 bg-white/95 p-2.5 text-xs shadow-xl dark:bg-neutral-900/95"
      style={{ top: top + 8 }}
    >
      <div className="flex items-baseline gap-2">
        <span className="font-semibold text-accent-700 dark:text-accent-300">Tap timing</span>
        <span className="min-w-0 flex-1 truncate text-neutral-500" title={track.name}>
          {track.name} · from {formatTime(tap.fromMs, 100)}
        </span>
      </div>
      {running && tap.session ? (
        <>
          <p className="text-sm" aria-live="polite">
            {next !== null ? (
              <>
                Next: <span className="font-semibold">“{track.marks[next]?.label || "(no label)"}”</span>{" "}
                <span className="text-neutral-500">
                  {tap.session.done + 1} of {total}
                </span>
              </>
            ) : (
              <>Every {unit} is timed. Press Esc to keep it.</>
            )}
          </p>
          <p className="text-neutral-500">
            Space or J as each {unit} starts; hold it down to its end. Esc finishes (one undo step).
          </p>
        </>
      ) : (
        <p className="text-neutral-500">
          Plays from just before the first {unit} after the playhead. Press Space or J as each {unit} starts, holding it down to its end if you like; the {unit === "word" ? "syllables and mouth shapes follow" : "mouth shapes follow"}. Slower playback lowers the pitch.
        </p>
      )}
      <div className="flex items-center gap-1">
        <span className="mr-1 text-neutral-500">Speed</span>
        {TAP_SPEEDS.map((speed) => (
          <button
            key={speed}
            type="button"
            aria-pressed={tap.speed === speed}
            onClick={() => useLyricTools.getState().setTapSpeed(speed)}
            className={`rounded px-1.5 py-0.5 tabular-nums ${tap.speed === speed ? "bg-accent-600 text-white" : "hover:bg-neutral-200 dark:hover:bg-neutral-800"}`}
          >
            {Math.round(speed * 100)}%
          </button>
        ))}
        <span className="flex-1" />
        {running ? (
          <>
            <button type="button" onClick={() => void useLyricTools.getState().cancelTap()} className="rounded px-2 py-1 hover:bg-neutral-200 dark:hover:bg-neutral-800">
              Discard
            </button>
            <button type="button" onClick={() => void useLyricTools.getState().finishTap()} className="rounded bg-accent-600 px-2 py-1 font-medium text-white hover:bg-accent-500">
              Finish
            </button>
          </>
        ) : (
          <>
            <button type="button" onClick={() => void useLyricTools.getState().cancelTap()} className="rounded px-2 py-1 hover:bg-neutral-200 dark:hover:bg-neutral-800">
              Cancel
            </button>
            <button type="button" onClick={() => void useLyricTools.getState().beginTap()} className="rounded bg-accent-600 px-2 py-1 font-medium text-white hover:bg-accent-500">
              Start
            </button>
          </>
        )}
      </div>
    </div>
  );
}
