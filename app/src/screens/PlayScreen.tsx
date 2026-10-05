import { AlertTriangle, Music, Pause, Play, RotateCcw, Square, Volume2 } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { errorMessage } from "../api/backend";
import type { PlaybackStatus, PlayerStatus, PreviewProp, SequenceEntry, Waveform } from "../api/types";
import { ChannelGrid } from "../components/ChannelGrid";
import { PreviewCanvas } from "../components/PreviewCanvas";
import { SequenceList } from "../components/SequenceList";
import { WaveformView } from "../components/WaveformView";
import { Button, EmptyState, PageHeader } from "../components/ui";
import { clock, fileName, thousands } from "../lib/format";
import { useApp } from "../state/store";

/** How often playback state and the preview refresh. */
const STATUS_MS = 250;
const FRAME_MS = 50;
const FPP_MS = 3000;

const STATE_STYLE: Record<string, string> = {
  ok: "text-green-600 dark:text-green-400",
  degraded: "text-amber-600 dark:text-amber-400",
  unresolved: "text-red-600 dark:text-red-400",
};

/** FPPs found by discovery that are playing right now (they override PixelFlow's output). */
function useBusyFpps(): {
  busy: { address: string; name: string; status: PlayerStatus }[];
  /** Checks the FPPs again now (after the user stops one). */
  recheck: () => Promise<void>;
} {
  const backend = useApp((s) => s.backend);
  const discovery = useApp((s) => s.discovery);
  const [busy, setBusy] = useState<{ address: string; name: string; status: PlayerStatus }[]>([]);
  const checkRef = useRef<() => Promise<void>>(async () => {});
  useEffect(() => {
    const fpps = discovery?.devices.filter((d) => d.kind === "fpp" && d.responding) ?? [];
    if (!backend || fpps.length === 0) {
      setBusy([]);
      return;
    }
    let cancelled = false;
    const check = async () => {
      const results = await Promise.all(
        fpps.map(async (d) => {
          try {
            return { address: d.address, name: d.name, status: await backend.fppStatus(d.address) };
          } catch {
            return null;
          }
        }),
      );
      if (!cancelled) setBusy(results.filter((r) => r !== null && r.status.state === "playing") as typeof busy);
    };
    checkRef.current = check;
    void check();
    const timer = setInterval(() => void check(), FPP_MS);
    return () => {
      cancelled = true;
      checkRef.current = async () => {};
      clearInterval(timer);
    };
  }, [backend, discovery]);
  const recheck = useCallback(() => checkRef.current(), []);
  return { busy, recheck };
}

/** Waveform slices to draw. */
const WAVEFORM_SLICES = 1200;

function describeOffset(ms: number): string {
  if (ms === 0) return "Lights are in sync with the music";
  return `Lights are ${Math.abs(ms)} ms ${ms > 0 ? "ahead of" : "behind"} the music`;
}

/** Shifts a sequence's lights against its music; every change is one undo step. */
function OffsetControl({ entry }: { entry: SequenceEntry }) {
  const apply = useApp((s) => s.apply);
  const set = (offsetMs: number) => apply([{ type: "updateSequence", sequence: { ...entry, offsetMs } }]);
  return (
    <div className="flex flex-wrap items-center gap-2 text-sm" role="group" aria-label="Music alignment">
      <span className="min-w-56 text-neutral-600 dark:text-neutral-300">{describeOffset(entry.offsetMs)}</span>
      {[-50, -10, 10, 50].map((delta) => (
        <Button key={delta} aria-label={`Lights ${delta > 0 ? "earlier" : "later"} by ${Math.abs(delta)} ms`} onClick={() => set(entry.offsetMs + delta)}>
          {delta > 0 ? `+${delta}` : delta}
        </Button>
      ))}
      {entry.offsetMs !== 0 && (
        <Button variant="ghost" onClick={() => set(0)}>
          Reset
        </Button>
      )}
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
    <div className="flex items-center gap-2 text-sm">
      <Music size={14} className="shrink-0 text-neutral-400" />
      {entry.audio ? (
        <>
          <span className="truncate" title={entry.audio}>
            {fileName(entry.audio)}
          </span>
          <Button variant="ghost" onClick={choose}>
            Change…
          </Button>
          <Button variant="ghost" onClick={() => apply([{ type: "updateSequence", sequence: { ...entry, audio: null } }])}>
            Remove
          </Button>
        </>
      ) : (
        <>
          <span className="text-neutral-500">No music</span>
          <Button onClick={choose}>Choose music…</Button>
        </>
      )}
    </div>
  );
}

/** Plays a rendered sequence (.fseq) on the controllers and shows it on the props. */
export function PlayScreen() {
  const backend = useApp((s) => s.backend);
  const snapshot = useApp((s) => s.snapshot);
  const setScreen = useApp((s) => s.setScreen);
  const [status, setStatus] = useState<PlaybackStatus | null>(null);
  const [props, setProps] = useState<PreviewProp[]>([]);
  const [frame, setFrame] = useState<Uint8Array | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [scrub, setScrub] = useState<number | null>(null);
  const { busy: busyFpps, recheck: recheckFpps } = useBusyFpps();
  const [stopping, setStopping] = useState<string | null>(null);
  // What the screen last showed, and when the user last did something: answers to status polls
  // that started before the user's last action are out of date and ignored.
  const [notice, setNotice] = useState<string | null>(null);
  const statusRef = useRef<PlaybackStatus | null>(null);
  const actions = useRef(0);
  statusRef.current = status;
  const known = snapshot?.show.controllers.some((c) => c.sequenceChannels) ?? false;
  // Controllers that get sequence data but have no strings in PixelFlow yet: shown as raw grids.
  const unwired = (snapshot?.show.controllers ?? []).filter(
    (c) => c.sequenceChannels && !c.ports.some((p) => p.slots.length > 0),
  );
  const [raw, setRaw] = useState<Uint8Array | null>(null);
  const sequences = snapshot?.show.sequences ?? [];
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const selected = sequences.find((s) => s.id === selectedId) ?? sequences[0] ?? null;
  const [waveform, setWaveform] = useState<Waveform | null>(null);
  const [playAll, setPlayAll] = useState(false);
  /** Status, when it belongs to the selected sequence (the transport and waveform show it). */
  const current = status && selected && status.sequence === selected.id ? status : null;

  const run = useCallback(
    async (action: () => Promise<PlaybackStatus | null | void>) => {
      actions.current++;
      try {
        const next = await action();
        if (next !== undefined) setStatus(next);
        setNotice(null);
        setError(null);
      } catch (e) {
        setError(errorMessage(e));
      } finally {
        actions.current++;
      }
    },
    [],
  );

  // Playback state, and the props' positions whenever the show changes.
  useEffect(() => {
    if (!backend) return;
    let cancelled = false;
    let polling = false;
    const poll = async () => {
      if (polling) return;
      polling = true;
      const startedAt = actions.current;
      try {
        const s = await backend.playbackStatus();
        if (cancelled || startedAt !== actions.current) return;
        if (s === null && statusRef.current !== null) {
          // Playback ended without the user stopping it: an edit to the show may be why.
          const reason = await backend.playbackStopReason().catch(() => null);
          if (cancelled || startedAt !== actions.current) return;
          setNotice(reason);
        }
        setStatus(s);
      } catch {
        // The next poll tries again.
      } finally {
        polling = false;
      }
    };
    void poll();
    const timer = setInterval(() => void poll(), STATUS_MS);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [backend]);

  useEffect(() => {
    if (!backend) return;
    void backend.previewProps().then(setProps, () => setProps([]));
  }, [backend, snapshot?.revision]);

  // The live preview, while something is loaded.
  const active = status !== null;
  useEffect(() => {
    if (!backend || !active) {
      setFrame(null);
      return;
    }
    let cancelled = false;
    let pending = false;
    const timer = setInterval(() => {
      if (pending) return;
      pending = true;
      backend.liveFrame().then(
        (f) => {
          pending = false;
          if (!cancelled) setFrame(f.length ? f : null);
        },
        () => {
          pending = false;
        },
      );
    }, FRAME_MS);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [backend, active]);

  const wantRaw = active && unwired.length > 0;
  useEffect(() => {
    if (!backend || !wantRaw) {
      setRaw(null);
      return;
    }
    let cancelled = false;
    let pending = false;
    const timer = setInterval(() => {
      if (pending) return;
      pending = true;
      backend.sequenceFrame().then(
        (f) => {
          pending = false;
          if (!cancelled) setRaw(f.length ? f : null);
        },
        () => {
          pending = false;
        },
      );
    }, FRAME_MS);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [backend, wantRaw]);

  // The selected sequence's music, drawn once per file.
  const audio = selected?.audio ?? null;
  useEffect(() => {
    if (!backend || !audio) {
      setWaveform(null);
      return;
    }
    let cancelled = false;
    backend.audioWaveform(audio, WAVEFORM_SLICES).then(
      (w) => !cancelled && setWaveform(w),
      () => !cancelled && setWaveform(null),
    );
    return () => {
      cancelled = true;
    };
  }, [backend, audio]);

  // Play all: when a sequence finishes, the next one starts (after the last, the first).
  const endedId = status?.state === "ended" && !status.error ? status.sequence : null;
  useEffect(() => {
    if (!playAll || !endedId || sequences.length === 0) return;
    const at = sequences.findIndex((s) => s.id === endedId);
    const next = sequences[(at + 1) % sequences.length];
    setSelectedId(next.id);
    void run(() => backend!.playSequence(next.id, 0));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [endedId, playAll]);

  const playSelected = (positionMs = 0) => {
    if (!selected) return;
    setSelectedId(selected.id);
    void run(() => backend!.playSequence(selected.id, positionMs));
  };

  const seekTo = (ms: number) => {
    if (current) void run(() => backend!.seekPlayback(ms));
    else playSelected(ms);
  };

  const scrubRef = useRef<number | null>(null);
  scrubRef.current = scrub;
  /** Jumps to where the slider was left (pointer release, key release, or leaving the slider). */
  const commitSeek = () => {
    const target = scrubRef.current;
    if (target === null) return;
    scrubRef.current = null;
    setScrub(null);
    void run(() => backend!.seekPlayback(target));
  };

  const stop = () => run(async () => {
    await backend!.stopPlayback();
    setStatus(null);
  });

  return (
    <div className="flex h-full flex-col">
      <PageHeader title="Play" description="Your show's sequences, played on your controllers with their music." />

      {busyFpps.map((fpp) => (
        <div
          key={fpp.address}
          role="alert"
          aria-label="FPP is playing"
          className="mb-4 flex items-center justify-between gap-3 rounded-lg border border-amber-300 bg-amber-50 p-3 text-sm text-amber-800 dark:border-amber-800 dark:bg-amber-950/40 dark:text-amber-300"
        >
          <p className="flex items-start gap-2">
            <AlertTriangle size={16} className="mt-0.5 shrink-0" />
            {fpp.name} is playing {fpp.status.sequence ?? fpp.status.playlist}. While it plays, it overrides PixelFlow on
            the same controllers.
          </p>
          <Button
            disabled={stopping === fpp.address}
            onClick={async () => {
              setStopping(fpp.address);
              try {
                await run(async () => backend!.fppStop(fpp.address, false));
                await recheckFpps();
              } finally {
                setStopping(null);
              }
            }}
          >
            {stopping === fpp.address ? "Stopping…" : "Stop FPP"}
          </Button>
        </div>
      ))}

      {error && (
        <p role="alert" className="mb-4 text-sm text-red-600 dark:text-red-400">
          {error}
        </p>
      )}
      {notice && !status && <p className="mb-4 text-sm text-amber-700 dark:text-amber-400">{notice}</p>}

      <div className="flex min-h-0 flex-1 gap-6">
        <SequenceList selected={selected?.id ?? null} playing={status?.sequence ?? null} onSelect={setSelectedId} />
        <div className="flex min-w-0 flex-1 flex-col">
      {!known && !status && (
        <EmptyState title="Add your controllers first">
          <p>
            PixelFlow doesn't know which channels go to which controller yet. On the Devices screen, open your FPP and add
            the controllers it sends to.
          </p>
          <div className="mt-3">
            <Button onClick={() => setScreen("devices")}>Go to Devices</Button>
          </div>
        </EmptyState>
      )}

      {selected && (
        <section aria-label="Transport" className="mb-4 flex flex-col gap-3 rounded-lg border border-neutral-200 p-4 dark:border-neutral-800">
          <div className="flex items-baseline justify-between gap-3">
            <h2 className="truncate text-lg font-semibold">{selected.name}</h2>
            <label className="flex shrink-0 items-center gap-2 text-sm text-neutral-600 dark:text-neutral-300">
              <input type="checkbox" checked={playAll} onChange={(e) => setPlayAll(e.target.checked)} />
              Play all in order
            </label>
          </div>
          <MusicRow entry={selected} />
          <div className="flex items-center gap-3">
            <Button
              variant="primary"
              aria-label={current?.state === "playing" ? "Pause" : "Play"}
              onClick={() =>
                !current || current.state === "ended"
                  ? playSelected(0)
                  : run(() => backend!.pausePlayback(current.state === "playing"))
              }
            >
              {current?.state === "playing" ? <Pause size={16} /> : <Play size={16} />}
            </Button>
            <Button aria-label="Restart" onClick={() => seekTo(0)}>
              <RotateCcw size={16} />
            </Button>
            <Button aria-label="Stop" onClick={stop} disabled={!status}>
              <Square size={16} />
            </Button>
            <div className="min-w-0 flex-1">
              <WaveformView
                waveform={waveform}
                positionMs={current ? (scrub ?? current.positionMs) : null}
                durationMs={current?.durationMs ?? waveform?.durationMs ?? 0}
                onSeek={seekTo}
              />
              {current && (
                <input
                  type="range"
                  aria-label="Position"
                  className="mt-1 w-full accent-violet-600"
                  min={0}
                  max={current.durationMs}
                  step={current.frameMs}
                  value={scrub ?? current.positionMs}
                  aria-valuetext={clock((scrub ?? current.positionMs) / 1000)}
                  onChange={(e) => setScrub(Number(e.target.value))}
                  onKeyDown={(e) => {
                    // Dragging moves frame by frame; the keyboard jumps a second at a time.
                    const keys: Record<string, number> = { ArrowRight: 1000, ArrowUp: 1000, ArrowLeft: -1000, ArrowDown: -1000, PageUp: 10000, PageDown: -10000 };
                    const here = scrubRef.current ?? current.positionMs;
                    const next =
                      e.key === "Home" ? 0 : e.key === "End" ? current.durationMs : e.key in keys ? here + keys[e.key] : null;
                    if (next === null) return;
                    e.preventDefault();
                    scrubRef.current = Math.min(current.durationMs, Math.max(0, next));
                    setScrub(scrubRef.current);
                  }}
                  onPointerUp={commitSeek}
                  onKeyUp={commitSeek}
                  onBlur={commitSeek}
                />
              )}
            </div>
            <p className="shrink-0 text-sm text-neutral-500 tabular-nums">
              {clock((current ? (scrub ?? current.positionMs) : 0) / 1000)} /{" "}
              {clock((current?.durationMs ?? waveform?.durationMs ?? 0) / 1000)}
            </p>
          </div>
          {selected.audio && <OffsetControl entry={selected} />}
          {current && (
            <label className="flex items-center gap-2 text-sm text-neutral-600 dark:text-neutral-300">
              <Volume2 size={14} />
              <input
                type="range"
                aria-label="Volume"
                className="w-40 accent-violet-600"
                min={0}
                max={100}
                value={Math.round(current.volume * 100)}
                onChange={(e) => void run(() => backend!.setPlaybackVolume(Number(e.target.value) / 100))}
              />
            </label>
          )}
          {current?.state === "ended" && !current.error && <p className="text-sm text-neutral-500">Finished. Press play to start again.</p>}
          {current?.error && <p className="text-sm text-red-600 dark:text-red-400">{current.error}</p>}
          {current?.notes.map((note) => (
            <p key={note} className="text-sm text-amber-700 dark:text-amber-400">
              {note}
            </p>
          ))}
          {current && current.controllers.length > 0 && (
            <ul className="flex flex-wrap gap-x-4 gap-y-1 text-sm">
              {current.controllers.map((c) => (
                <li key={c.id}>
                  {c.name}: <span className={STATE_STYLE[c.state]}>{c.state === "ok" ? "sending" : c.state}</span>
                  {c.lastError && <span className="text-neutral-500"> — {c.lastError}</span>}
                </li>
              ))}
            </ul>
          )}
        </section>
      )}

      {status && unwired.length > 0 && (
        <div className="mb-4 flex flex-col gap-3">
          {unwired.map((c) => (
            <ChannelGrid
              key={c.id}
              frame={raw}
              start={c.sequenceChannels!.start}
              count={c.sequenceChannels!.count}
              label={`${c.name}: ${thousands(Math.ceil(c.sequenceChannels!.count / 3))} pixels as received (import its strings to see them on your props)`}
            />
          ))}
        </div>
      )}

      {props.length > 0 && (status || known) && (
        <div className="min-h-64 flex-1">
          <PreviewCanvas props={props} frame={frame} />
        </div>
      )}
        </div>
      </div>
    </div>
  );
}
