import { AlertTriangle, Loader2, Play, Square, StepForward } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { errorMessage } from "../api/backend";
import type { FppSequence, PlayerStatus } from "../api/types";
import { clock, thousands } from "../lib/format";
import { useApp } from "../state/store";
import { Button } from "./ui";

/** How often the player status refreshes while the panel is open. */
const REFRESH_MS = 2000;

function describe(status: PlayerStatus): string {
  const what = status.sequence ?? status.playlist ?? "";
  switch (status.state) {
    case "idle":
      return "Idle";
    case "playing":
      return `Playing ${what}`;
    case "paused":
      return `Paused ${what}`;
    case "stopping":
      return `Stopping after ${what}`;
    case "other":
      return "Busy (testing or another mode)";
  }
}

/** An FPP's live status, its playback controls, and the sequences stored on it. Play and Stop
 * change what the FPP is doing, so they only ever run when clicked. */
export function FppPanel({ address }: { address: string }) {
  const backend = useApp((s) => s.backend);
  const [status, setStatus] = useState<PlayerStatus | null>(null);
  const [sequences, setSequences] = useState<FppSequence[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    if (!backend) return;
    try {
      setStatus(await backend.fppStatus(address));
      setError(null);
    } catch (e) {
      setError(errorMessage(e));
    }
  }, [backend, address]);

  useEffect(() => {
    void refresh();
    backend?.fppSequences(address).then(setSequences, () => setSequences([]));
    const timer = setInterval(() => void refresh(), REFRESH_MS);
    return () => clearInterval(timer);
  }, [backend, address, refresh]);

  const act = async (action: () => Promise<void>) => {
    if (busy) return;
    setBusy(true);
    try {
      await action();
      await refresh();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const active = status && (status.state === "playing" || status.state === "paused");

  return (
    <section aria-label="Player" className="flex flex-col gap-3 rounded-lg border border-neutral-200 p-4 dark:border-neutral-800">
      <div className="flex items-center justify-between gap-3">
        {status ? (
          <div className="min-w-0">
            <p className="flex items-center gap-2 font-medium">
              {status.state === "playing" && <Play size={14} className="shrink-0 text-emerald-600" />}
              <span className="truncate">{describe(status)}</span>
            </p>
            {active && status.secondsRemaining > 0 && (
              <p className="text-neutral-500 tabular-nums">{clock(status.secondsRemaining)} left</p>
            )}
            {status.nextPlaylist && (
              <p className="text-neutral-500">
                Next: {status.nextPlaylist}
                {status.nextStart ? `, ${status.nextStart}` : ""}
              </p>
            )}
          </div>
        ) : (
          !error && (
            <p className="flex items-center gap-2 text-neutral-500">
              <Loader2 size={16} className="animate-spin" /> Checking what this FPP is playing…
            </p>
          )
        )}
        {active && (
          <div className="flex shrink-0 gap-2">
            <Button onClick={() => act(() => backend!.fppStop(address, true))} disabled={busy}>
              <StepForward size={14} /> Stop after this
            </Button>
            <Button onClick={() => act(() => backend!.fppStop(address, false))} disabled={busy}>
              <Square size={14} /> Stop now
            </Button>
          </div>
        )}
      </div>
      {error && (
        <p role="alert" className="text-red-600 dark:text-red-400">
          {error}
        </p>
      )}
      {status && status.warnings.length > 0 && (
        <ul className="flex flex-col gap-1 text-amber-700 dark:text-amber-400">
          {status.warnings.map((w) => (
            <li key={w} className="flex items-start gap-2">
              <AlertTriangle size={14} className="mt-0.5 shrink-0" /> FPP reports: {w}
            </li>
          ))}
        </ul>
      )}
      {sequences.length > 0 && (
        <table className="w-full">
          <thead>
            <tr className="text-left text-xs tracking-wide text-neutral-500 uppercase">
              <th className="pb-1 font-medium">Sequence on this FPP</th>
              <th className="pb-1 text-right font-medium">Length</th>
              <th className="pb-1 text-right font-medium">Channels</th>
              <th className="pb-1" />
            </tr>
          </thead>
          <tbody>
            {sequences.map((s) => (
              <tr key={s.name} className="border-t border-neutral-200 dark:border-neutral-800">
                <td className="py-1">{s.name}</td>
                <td className="text-right tabular-nums">{s.frames ? clock((s.frames * s.stepMs) / 1000) : "—"}</td>
                <td className="text-right tabular-nums">{s.channels ? thousands(s.channels) : "—"}</td>
                <td className="py-1 pl-3 text-right">
                  <Button
                    aria-label={`Play ${s.name}`}
                    onClick={() => act(() => backend!.fppStart(address, `${s.name}.fseq`))}
                    disabled={busy}
                  >
                    <Play size={14} /> Play
                  </Button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  );
}
