import { CalendarClock, Loader2, Pause, Play, Square, StepForward } from "lucide-react";
import { useState } from "react";
import { errorMessage } from "../../api/backend";
import type { PlayerStatus } from "../../api/types";
import { clock } from "../../lib/format";
import { useApp } from "../../state/store";
import { Button } from "../ui";
import { Section } from "./Section";

/** What the FPP is playing, how far along it is, Stop, and what's scheduled next. Stop changes
 * what the FPP is doing, so it only ever runs when clicked. */
export function NowPlaying({
  address,
  status,
  error,
  onChanged,
}: {
  address: string;
  status: PlayerStatus | null;
  error: string | null;
  onChanged: () => void;
}) {
  const backend = useApp((s) => s.backend);
  const [busy, setBusy] = useState(false);
  const [stopError, setStopError] = useState<string | null>(null);
  const active = status && (status.state === "playing" || status.state === "paused" || status.state === "stopping");

  const stop = async (gracefully: boolean) => {
    if (!backend || busy) return;
    setBusy(true);
    setStopError(null);
    try {
      await backend.fppStop(address, gracefully);
    } catch (e) {
      setStopError(errorMessage(e));
    } finally {
      setBusy(false);
      onChanged();
    }
  };

  const total = status ? status.secondsElapsed + status.secondsRemaining : 0;
  const percent = total > 0 && status ? Math.min(100, (status.secondsElapsed / total) * 100) : 0;
  const title = status?.sequence ?? status?.playlist ?? "";

  return (
    <Section
      title="Now playing"
      actions={
        active && (
          <>
            <Button onClick={() => stop(true)} disabled={busy || status.state === "stopping"} title="Let the current sequence finish, then stop">
              <StepForward size={14} aria-hidden /> Stop after this
            </Button>
            <Button onClick={() => stop(false)} disabled={busy}>
              <Square size={14} aria-hidden /> Stop now
            </Button>
          </>
        )
      }
    >
      {!status && !error && (
        <p className="flex items-center gap-2 text-neutral-500">
          <Loader2 size={14} className="animate-spin" aria-hidden /> Checking what this FPP is playing…
        </p>
      )}
      {!status && error && <p className="text-neutral-500">Can't tell what it's playing while it isn't answering.</p>}
      {status && active && (
        <div className="flex flex-col gap-1.5">
          <div className="flex min-w-0 items-baseline gap-2">
            {status.state === "paused" ? (
              <Pause size={14} className="shrink-0 self-center text-neutral-500" aria-hidden />
            ) : (
              <Play size={14} className="shrink-0 self-center text-emerald-600" aria-hidden />
            )}
            <p className="truncate text-base font-medium">{title}</p>
            {status.playlist && status.playlist !== title && <p className="truncate text-neutral-500">from {status.playlist}</p>}
            {status.state !== "playing" && (
              <span className="shrink-0 rounded bg-neutral-100 px-1.5 py-0.5 text-xs dark:bg-neutral-800">
                {status.state === "paused" ? "Paused" : "Stopping after this sequence"}
              </span>
            )}
          </div>
          {total > 0 && (
            <div className="flex items-center gap-2 text-xs text-neutral-500 tabular-nums">
              <span aria-label="Played">{clock(status.secondsElapsed)}</span>
              <div
                role="progressbar"
                aria-label="How far along"
                aria-valuemin={0}
                aria-valuemax={total}
                aria-valuenow={status.secondsElapsed}
                aria-valuetext={`${clock(status.secondsRemaining)} left`}
                className="h-1.5 flex-1 overflow-hidden rounded-full bg-neutral-200 dark:bg-neutral-800"
              >
                <div className="h-full rounded-full bg-accent-600 transition-[width] duration-1000 ease-linear" style={{ width: `${percent}%` }} />
              </div>
              <span>{clock(status.secondsRemaining)} left</span>
            </div>
          )}
        </div>
      )}
      {status && status.state === "idle" && <p className="text-neutral-600 dark:text-neutral-300">Nothing playing.</p>}
      {status && status.state === "other" && <p className="text-neutral-600 dark:text-neutral-300">Busy testing, or in another mode.</p>}
      {status?.nextPlaylist && (
        <p className="flex items-center gap-1.5 text-neutral-600 dark:text-neutral-300">
          <CalendarClock size={14} className="shrink-0 text-neutral-500" aria-hidden />
          <span className="truncate">
            Next: <span className="font-medium">{status.nextPlaylist}</span>
            {status.nextStart ? ` · ${status.nextStart.replace(/\s+/g, " ")}` : ""}
          </span>
        </p>
      )}
      {stopError && (
        <p role="alert" className="text-red-600 dark:text-red-400">
          {stopError}
        </p>
      )}
    </Section>
  );
}
