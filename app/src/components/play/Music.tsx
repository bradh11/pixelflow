import { Music, Volume2 } from "lucide-react";
import { useEffect, useRef } from "react";
import type { MissingFile, SequenceEntry, Waveform } from "../../api/types";
import { fileName, shownPath } from "../../lib/format";
import { useApp } from "../../state/store";
import { LIGHT_TIMING, LIGHT_TIMING_HINT, LIGHT_TIMING_SCOPE } from "../../lib/labels";
import { MissingFileNotice } from "../MissingFiles";
import { Button } from "../ui";
import { WaveformView } from "../WaveformView";
import { DrawerSection } from "./DrawerSection";

/** Most the lights may be moved against the music, either way (the engine allows no more). */
const MAX_OFFSET_MS = 10_000;

function describeOffset(ms: number): string {
  if (ms === 0) return "Lights are in sync with the music";
  return `Lights are ${Math.abs(ms)} ms ${ms > 0 ? "ahead of" : "behind"} the music`;
}

/** Shifts a sequence's lights against its music; every change is one undo step. */
function OffsetControl({ entry }: { entry: SequenceEntry }) {
  const apply = useApp((s) => s.apply);
  // Quick clicks build on each other, not on what was last drawn.
  const target = useRef(entry.offsetMs);
  const pending = useRef(0);
  useEffect(() => {
    if (pending.current === 0) target.current = entry.offsetMs;
  }, [entry.offsetMs]);
  const set = async (offsetMs: number) => {
    target.current = Math.max(-MAX_OFFSET_MS, Math.min(MAX_OFFSET_MS, offsetMs));
    const latest = useApp.getState().snapshot?.show.sequences.find((s) => s.id === entry.id) ?? entry;
    pending.current++;
    try {
      await apply([{ type: "updateSequence", sequence: { ...latest, offsetMs: target.current } }]);
    } finally {
      pending.current--;
      if (pending.current === 0) {
        target.current = useApp.getState().snapshot?.show.sequences.find((s) => s.id === entry.id)?.offsetMs ?? target.current;
      }
    }
  };
  return (
    <div className="flex flex-col gap-1.5 text-sm" role="group" aria-label={LIGHT_TIMING}>
      <span className="font-medium text-neutral-700 dark:text-neutral-200" title={`${LIGHT_TIMING_HINT} ${LIGHT_TIMING_SCOPE}`}>
        {LIGHT_TIMING}
      </span>
      <p className="text-xs text-neutral-500">{LIGHT_TIMING_HINT}</p>
      <div className="grid grid-cols-4 gap-1.5">
        {[-50, -10, 10, 50].map((delta) => (
          <Button
            key={delta}
            aria-label={`${delta > 0 ? "+" : "−"}${Math.abs(delta)} ms: lights ${delta > 0 ? "earlier" : "later"}`}
            title={`Lights ${Math.abs(delta)} ms ${delta > 0 ? "earlier" : "later"}`}
            disabled={delta > 0 ? entry.offsetMs >= MAX_OFFSET_MS : entry.offsetMs <= -MAX_OFFSET_MS}
            onClick={() => set(target.current + delta)}
            className="tabular-nums"
          >
            {delta > 0 ? `+${delta}` : `−${Math.abs(delta)}`} ms
          </Button>
        ))}
      </div>
      <div className="flex min-h-8 items-center justify-between gap-2">
        <span className="text-xs text-neutral-500">{describeOffset(entry.offsetMs)}</span>
        {entry.offsetMs !== 0 && (
          <Button variant="ghost" onClick={() => set(0)}>
            Reset
          </Button>
        )}
      </div>
    </div>
  );
}

/** The sequence's music file, with a way to choose or remove it. */
function MusicRow({ entry }: { entry: SequenceEntry }) {
  const apply = useApp((s) => s.apply);
  const backend = useApp((s) => s.backend);
  const choose = async () => {
    const audio = await backend?.pickAudioPath();
    if (audio) await apply([{ type: "updateSequence", sequence: { ...entry, audio } }]);
  };
  return (
    <div className="flex flex-col gap-1.5 text-sm">
      <div className="flex items-start gap-2">
        <span title="Music" className="mt-0.5 shrink-0">
          <Music size={14} className="text-neutral-400" aria-hidden />
        </span>
        {entry.audio ? (
          <span className="min-w-0 break-words" title={shownPath(entry.audio)}>
            {fileName(entry.audio)}
          </span>
        ) : (
          <span className="text-neutral-500">No music</span>
        )}
      </div>
      <div className="flex flex-wrap gap-1.5">
        {entry.audio ? (
          <>
            <Button onClick={choose}>Change…</Button>
            <Button variant="ghost" title="Play this sequence without music" onClick={() => apply([{ type: "updateSequence", sequence: { ...entry, audio: null } }])}>
              Remove music
            </Button>
          </>
        ) : (
          <Button onClick={choose}>Choose music…</Button>
        )}
      </div>
    </div>
  );
}

/**
 * A rendered sequence's music, in the playlist drawer: its file (choose, change, or remove it),
 * its waveform (click to jump there), the volume, and the lights' timing against it.
 */
export function MusicSection({
  entry,
  missing,
  waveform,
  loading,
  positionMs,
  durationMs,
  volume,
  onSeek,
  onVolume,
}: {
  entry: SequenceEntry;
  /** The music file, when it isn't where the show says. */
  missing: MissingFile | undefined;
  waveform: Waveform | null;
  loading: boolean;
  /** Where this sequence is playing; null when it isn't the one playing. */
  positionMs: number | null;
  durationMs: number;
  volume: number;
  onSeek: (ms: number) => void;
  onVolume: (volume: number) => void;
}) {
  return (
    <DrawerSection id="music" title="Music">
      <p className="truncate text-xs text-neutral-500" title={entry.name}>
        For {entry.name}
      </p>
      <MusicRow entry={entry} />
      {missing && <MissingFileNotice missing={missing} />}
      {entry.audio && (
        <>
          <WaveformView waveform={waveform} audio={entry.audio} loading={loading} positionMs={positionMs} durationMs={durationMs} offsetMs={entry.offsetMs} onSeek={onSeek} />
          <label className="flex items-center gap-2 text-sm text-neutral-600 dark:text-neutral-300">
            <Volume2 size={14} className="shrink-0" aria-hidden />
            <input
              type="range"
              aria-label="Volume"
              className="min-w-0 flex-1 accent-violet-600"
              min={0}
              max={100}
              value={Math.round(volume * 100)}
              onChange={(e) => onVolume(Number(e.target.value) / 100)}
            />
          </label>
          <OffsetControl entry={entry} />
        </>
      )}
    </DrawerSection>
  );
}
