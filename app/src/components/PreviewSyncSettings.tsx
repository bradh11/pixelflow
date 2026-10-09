import { Play, Square } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { type ClockReading, SYNC_RANGE_MS, type TapResult, calibrateTaps, eventTime, everyFrame, musicAt, nextReading, readingFrom, tapLag } from "../lib/avSync";
import { usePreviewSync } from "../state/previewSync";
import { useApp } from "../state/store";
import { Button } from "./ui";

/** Clicks a second: 120 a minute. */
const INTERVAL_MS = 500;
/** How long the square stays lit on each click. */
const FLASH_MS = 90;
/** How often the click's position is asked for. */
const ASK_MS = 50;

/** The offset in words: which way it moves the picture. */
export function describeOffset(ms: number): string {
  if (ms === 0) return "0 ms (as the engine counts it)";
  return ms > 0 ? `+${ms} ms (picture earlier)` : `${ms} ms (picture later)`;
}

/**
 * Preview sync for the Settings screen: a click plays through the music's sound output while a
 * square flashes in time, moved by the offset; the slider (or tapping along to the click) sets the
 * offset until the two land together. It applies to the app's preview and the timeline's playhead
 * only, and is kept on this computer.
 */
export function PreviewSyncSettings() {
  const backend = useApp((s) => s.backend);
  const offsetMs = usePreviewSync((s) => s.offsetMs);
  const setOffset = usePreviewSync((s) => s.setOffset);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [tapping, setTapping] = useState(false);
  const [taps, setTaps] = useState<number[]>([]);
  const flash = useRef<HTMLDivElement>(null);
  const clock = useRef<ClockReading | null>(null);

  // While the click plays: follow where it's heard, and flash the square on each click.
  useEffect(() => {
    if (!running || !backend) return;
    let live = true;
    let asking = false;
    const ask = setInterval(() => {
      if (asking) return;
      asking = true;
      const sentAt = performance.now();
      backend.syncClickPosition().then(
        (ms) => {
          asking = false;
          const gotAt = performance.now();
          if (!live) return;
          clock.current = ms === null ? null : nextReading(clock.current, readingFrom(ms, 1, true, sentAt, gotAt));
        },
        () => {
          asking = false;
        },
      );
    }, ASK_MS);
    const stopFlashing = everyFrame(() => {
      const box = flash.current;
      const reading = clock.current;
      if (!box || !reading) return;
      // What's drawn now shows on the next refresh, about 16 ms on.
      const shown = musicAt(reading, performance.now() + 16) + usePreviewSync.getState().offsetMs;
      const into = ((shown % INTERVAL_MS) + INTERVAL_MS) % INTERVAL_MS;
      box.dataset.lit = String(into < FLASH_MS);
    });
    return () => {
      live = false;
      clearInterval(ask);
      stopFlashing();
      clock.current = null;
      void backend.syncClickStop();
    };
  }, [running, backend]);

  // Tap along: Space or T on each click, timed by when the key went down.
  useEffect(() => {
    if (!tapping) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== " " && e.key.toLowerCase() !== "t") return;
      e.preventDefault();
      e.stopPropagation();
      const reading = clock.current;
      if (e.repeat || !reading) return;
      const at = musicAt(reading, eventTime(e));
      setTaps((t) => [...t.slice(-23), at]);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [tapping]);

  const start = async () => {
    if (!backend) return;
    setError(null);
    try {
      await backend.syncClickStart(INTERVAL_MS);
      setRunning(true);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };
  const stop = () => {
    setRunning(false);
    setTapping(false);
  };

  const result: TapResult | null = calibrateTaps(taps, INTERVAL_MS);

  return (
    <div className="flex flex-col gap-3 text-sm">
      <p className="text-neutral-600 dark:text-neutral-400">
        The music reaches your ears a little after PixelFlow sends it out, and the preview takes a moment to draw. If words or beats look a touch late or early in the
        preview, play the click, then move the slider until the square flashes exactly on the click. This only moves the preview and the timeline's playhead on this
        computer; the lights, Send to FPP, and exports keep their timing.
      </p>
      <div className="flex flex-wrap items-center gap-3">
        {running ? (
          <Button onClick={stop}>
            <Square size={14} aria-hidden /> Stop the click
          </Button>
        ) : (
          <Button onClick={() => void start()} disabled={!backend}>
            <Play size={14} aria-hidden /> Play the click
          </Button>
        )}
        <div
          ref={flash}
          role="img"
          aria-label="Flashes on each click"
          data-lit="false"
          className="size-8 rounded-md border border-neutral-300 bg-neutral-200 data-[lit=true]:border-accent-500 data-[lit=true]:bg-accent-500 dark:border-neutral-700 dark:bg-neutral-800"
        />
        {running && (
          <Button
            variant={tapping ? "primary" : "secondary"}
            aria-pressed={tapping}
            onClick={() => {
              setTaps([]);
              setTapping(!tapping);
            }}
          >
            {tapping ? "Stop tapping" : "Tap along"}
          </Button>
        )}
      </div>
      {error && <p className="text-red-600 dark:text-red-400">{error}</p>}
      {tapping && (
        <div className="rounded-md bg-neutral-100 px-3 py-2 dark:bg-neutral-800/60" aria-live="polite">
          {result ? (
            <div className="flex flex-wrap items-center gap-2">
              <span>
                Your taps came {Math.abs(Math.round(result.lagMs))} ms {result.lagMs >= 0 ? "after" : "before"} the clicks (give or take {Math.round(result.spreadMs)} ms
                {result.left > 0 ? `, ${result.left} left out` : ""}). Suggested: {describeOffset(result.offsetMs)}.
              </span>
              <Button variant="primary" onClick={() => setOffset(result.offsetMs)}>
                Use {result.offsetMs > 0 ? "+" : ""}
                {result.offsetMs} ms
              </Button>
            </div>
          ) : (
            <span>
              Press Space or T on every click you hear ({taps.length} so far). After a few taps PixelFlow suggests an offset.
              {taps.length > 0 && ` Last tap ${Math.round(tapLag(taps[taps.length - 1], INTERVAL_MS))} ms from its click.`}
            </span>
          )}
        </div>
      )}
      <label className="flex flex-col gap-1">
        <span className="text-neutral-600 dark:text-neutral-400">
          Preview offset: <span className="font-medium text-neutral-900 tabular-nums dark:text-neutral-100">{describeOffset(offsetMs)}</span>
        </span>
        <div className="flex items-center gap-2">
          <span className="text-xs text-neutral-500">Later</span>
          <input
            type="range"
            aria-label="Preview offset"
            min={-SYNC_RANGE_MS}
            max={SYNC_RANGE_MS}
            step={5}
            value={offsetMs}
            onChange={(e) => setOffset(Number(e.target.value))}
            className="min-w-0 flex-1 accent-violet-600"
          />
          <span className="text-xs text-neutral-500">Earlier</span>
          <Button variant="ghost" onClick={() => setOffset(0)} disabled={offsetMs === 0}>
            Reset
          </Button>
        </div>
      </label>
      <p className="text-xs text-neutral-500">
        If the lights in the preview look late against the music, move right; if they look early, move left. Bluetooth speakers and headphones usually need the most.
      </p>
    </div>
  );
}
