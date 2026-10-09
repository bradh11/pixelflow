import { CheckCircle2, Film } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { errorMessage } from "../../api/backend";
import type { Sequence } from "../../api/sequence";
import type { VideoChoices, VideoProgress, VideoRequest, VideoSummary } from "../../api/video";
import { clock, fileName, sizeText } from "../../lib/format";
import { formatTime, parseTime } from "../../lib/timelineMath";
import { useSequencer } from "../../state/sequencer";
import { ProgressBar } from "../ProgressBar";
import { Button, Input, Select } from "../ui";

/** The choices last used, remembered on this computer. */
const SETTINGS_KEY = "pixelflow.videoExport";

interface Settings {
  height: 720 | 1080;
  fps: 30 | 60;
  photo: boolean;
  pixelSize: number;
  ffmpeg: boolean;
}

const DEFAULTS: Settings = { height: 1080, fps: 30, photo: true, pixelSize: 1, ffmpeg: false };

const PIXEL_SIZES = [
  { value: 0.6, label: "Small" },
  { value: 1, label: "As in the preview" },
  { value: 1.5, label: "Large" },
  { value: 2, label: "Extra large" },
];

function loadSettings(): Settings {
  try {
    const saved = JSON.parse(localStorage.getItem(SETTINGS_KEY) ?? "{}") as Partial<Settings>;
    return {
      height: saved.height === 720 ? 720 : 1080,
      fps: saved.fps === 60 ? 60 : 30,
      photo: saved.photo !== false,
      pixelSize: PIXEL_SIZES.some((p) => p.value === saved.pixelSize) ? saved.pixelSize! : 1,
      ffmpeg: saved.ffmpeg === true,
    };
  } catch {
    return DEFAULTS;
  }
}

function saveSettings(settings: Settings) {
  try {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
  } catch {
    // Storage unavailable: the dialog starts from the defaults next time.
  }
}

/** The span of the selected effects, or null when none is selected. */
export function selectionSpan(doc: Sequence, selection: string[]): { startMs: number; endMs: number } | null {
  const chosen = new Set(selection);
  let span: { startMs: number; endMs: number } | null = null;
  for (const row of doc.rows) {
    for (const layer of row.layers) {
      for (const e of layer.effects) {
        if (!chosen.has(e.id)) continue;
        span = span ? { startMs: Math.min(span.startMs, e.startMs), endMs: Math.max(span.endMs, e.endMs) } : { startMs: e.startMs, endMs: e.endMs };
      }
    }
  }
  return span;
}

/** "1080p", or "1080p60" at 60 frames a second. */
export function qualityLabel(height: number, fps: number): string {
  return `${height}p${fps === 60 ? "60" : ""}`;
}

type Range = "all" | "selection" | "custom";

type Phase =
  | { kind: "choose"; error?: string; info?: string }
  | { kind: "exporting"; progress: VideoProgress | null; cancelling: boolean }
  | { kind: "done"; path: string; summary: VideoSummary };

/**
 * Export video: the sequence as an MP4 of the 2D preview with its music, to share or post. The
 * size, frame rate, stretch of the song, photo, dot size, and encoder are chosen here; then where
 * to save it, and its progress shows here with Cancel.
 */
export function ExportVideoDialog({ onClose }: { onClose: () => void }) {
  const api = useSequencer((s) => s.api);
  const doc = useSequencer((s) => s.doc);
  const path = useSequencer((s) => s.path);
  const selection = useSequencer((s) => s.selection);
  const playheadMs = useSequencer((s) => s.playheadMs);
  const [choices, setChoices] = useState<VideoChoices | null>(null);
  const [settings, setSettings] = useState<Settings>(loadSettings);
  const span = doc ? selectionSpan(doc, selection) : null;
  const [range, setRange] = useState<Range>(span ? "selection" : "all");
  const [from, setFrom] = useState(() => formatTime(span?.startMs ?? playheadMs, 100));
  const [to, setTo] = useState(() => formatTime(span?.endMs ?? doc?.durationMs ?? 0, 100));
  const [phase, setPhase] = useState<Phase>({ kind: "choose" });
  const box = useRef<HTMLDivElement>(null);
  const exporting = phase.kind === "exporting";
  const close = useRef(onClose);
  close.current = onClose;
  const busy = useRef(false);
  busy.current = exporting;

  useEffect(() => {
    if (!api) return;
    let live = true;
    api.videoExportChoices().then(
      (c) => live && setChoices(c),
      () => live && setChoices({ ffmpeg: null, photo: false }),
    );
    return () => {
      live = false;
    };
  }, [api]);

  // The dialog takes the focus (and gives it back when it closes); Escape closes it, unless
  // an export is running (Cancel stops that first).
  useEffect(() => {
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    box.current?.querySelector<HTMLElement>("select, input, button")?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || busy.current) return;
      e.preventDefault();
      close.current();
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
      if (opener?.isConnected) opener.focus();
    };
  }, []);

  if (!doc || !api) return null;
  const fromMs = parseTime(from);
  const toMs = parseTime(to);
  const customBad = range === "custom" && (fromMs === null || toMs === null || toMs <= fromMs || fromMs >= doc.durationMs);
  const startMs = range === "selection" && span ? span.startMs : range === "custom" ? (fromMs ?? 0) : 0;
  const endMs = range === "selection" && span ? span.endMs : range === "custom" ? Math.min(toMs ?? 0, doc.durationMs) : doc.durationMs;
  const ffmpeg = settings.ffmpeg && !!choices?.ffmpeg;
  const photo = settings.photo && !!choices?.photo;
  const update = (change: Partial<Settings>) => {
    const next = { ...settings, ...change };
    setSettings(next);
    saveSettings(next);
  };

  const start = async () => {
    if (customBad) return;
    // Export what's on screen: every edit made so far lands first.
    await useSequencer.getState().settled();
    const base = path ? fileName(path).replace(/\.pfseq\.json$|\.json$/i, "") : doc.name || "Sequence";
    let target: string | null;
    try {
      target = await api.pickVideoPath(`${base}.mp4`);
    } catch (e) {
      setPhase({ kind: "choose", error: errorMessage(e) });
      return;
    }
    if (!target) return;
    const request: VideoRequest = {
      width: settings.height === 1080 ? 1920 : 1280,
      height: settings.height,
      fps: settings.fps,
      startMs,
      endMs: range === "all" ? null : endMs,
      photo,
      pixelSize: settings.pixelSize,
      ffmpeg,
    };
    setPhase({ kind: "exporting", progress: null, cancelling: false });
    try {
      const summary = await api.exportVideo(target, request, (progress) =>
        setPhase((p) => (p.kind === "exporting" ? { ...p, progress } : p)),
      );
      setPhase({ kind: "done", path: target, summary });
    } catch (e) {
      const message = errorMessage(e);
      setPhase(message === "The export was cancelled." ? { kind: "choose", info: "Export cancelled. No file was written." } : { kind: "choose", error: message });
    }
  };
  const cancel = () => {
    setPhase((p) => (p.kind === "exporting" ? { ...p, cancelling: true } : p));
    void api.cancelVideoExport();
  };

  const label = "text-neutral-600 dark:text-neutral-400";
  return (
    <div className="fixed inset-0 z-40 flex items-center justify-center bg-black/40">
      <div
        ref={box}
        role="dialog"
        aria-modal="true"
        aria-label="Export video"
        className="flex max-h-[calc(100vh-2rem)] w-[30rem] max-w-[calc(100vw-2rem)] flex-col overflow-auto rounded-lg border border-neutral-200 bg-white p-5 text-sm shadow-xl dark:border-neutral-800 dark:bg-neutral-900"
      >
        <h2 className="flex items-center gap-2 text-lg font-semibold">
          <Film size={18} aria-hidden /> Export video
        </h2>
        {phase.kind === "done" ? (
          <Done path={phase.path} summary={phase.summary} onClose={onClose} />
        ) : (
          <>
            <p className="mt-1 text-neutral-500">The preview of {doc.name || "this sequence"} with its music, as an MP4 to share or post.</p>
            <fieldset disabled={exporting} className="mt-4 grid grid-cols-2 gap-3">
              <label className="flex flex-col gap-1">
                <span className={label}>Size</span>
                <Select value={settings.height} onChange={(e) => update({ height: Number(e.target.value) === 720 ? 720 : 1080 })}>
                  <option value={1080}>1080p (1920 × 1080)</option>
                  <option value={720}>720p (1280 × 720)</option>
                </Select>
              </label>
              <label className="flex flex-col gap-1">
                <span className={label}>Frame rate</span>
                <Select value={settings.fps} onChange={(e) => update({ fps: Number(e.target.value) === 60 ? 60 : 30 })}>
                  <option value={30}>30 frames a second</option>
                  <option value={60}>60 frames a second (smoother)</option>
                </Select>
              </label>
              <label className="col-span-2 flex flex-col gap-1">
                <span className={label}>What to export</span>
                <Select value={range} onChange={(e) => setRange(e.target.value as Range)}>
                  <option value="all">The whole sequence ({clock(doc.durationMs / 1000)})</option>
                  {span && (
                    <option value="selection">
                      The selected effects ({formatTime(span.startMs, 100)} – {formatTime(span.endMs, 100)})
                    </option>
                  )}
                  <option value="custom">Part of it…</option>
                </Select>
              </label>
              {range === "custom" && (
                <div className="col-span-2 flex items-end gap-2">
                  <label className="flex flex-1 flex-col gap-1">
                    <span className={label}>From</span>
                    <Input value={from} onChange={(e) => setFrom(e.target.value)} aria-invalid={fromMs === null} />
                  </label>
                  <label className="flex flex-1 flex-col gap-1">
                    <span className={label}>To</span>
                    <Input value={to} onChange={(e) => setTo(e.target.value)} aria-invalid={toMs === null} />
                  </label>
                </div>
              )}
              {customBad && (
                <p className="col-span-2 text-xs text-red-600 dark:text-red-400" role="alert">
                  Enter times like 1:05 or 65.5, with the end after the start and the start inside the sequence.
                </p>
              )}
              <label className="flex flex-col gap-1">
                <span className={label}>Pixel size</span>
                <Select value={settings.pixelSize} onChange={(e) => update({ pixelSize: Number(e.target.value) })}>
                  {PIXEL_SIZES.map((p) => (
                    <option key={p.value} value={p.value}>
                      {p.label}
                    </option>
                  ))}
                </Select>
              </label>
              <label className="flex flex-col gap-1">
                <span className={label}>Encoder</span>
                <Select value={ffmpeg ? "ffmpeg" : "builtIn"} onChange={(e) => update({ ffmpeg: e.target.value === "ffmpeg" })}>
                  <option value="builtIn">Built in</option>
                  {choices?.ffmpeg && <option value="ffmpeg">ffmpeg (higher quality)</option>}
                </Select>
              </label>
              <label
                className={`col-span-2 flex items-center gap-2 ${choices?.photo ? "" : "text-neutral-400 dark:text-neutral-500"}`}
                title={choices && !choices.photo ? "Add a photo of the house on the Layout screen to show it here" : undefined}
              >
                <input type="checkbox" checked={photo} disabled={!choices?.photo} onChange={(e) => update({ photo: e.target.checked })} />
                Show the house photo behind the lights
              </label>
            </fieldset>
            {phase.kind === "choose" && phase.error && (
              <p className="mt-3 text-red-600 dark:text-red-400" role="alert">
                {phase.error}
              </p>
            )}
            {phase.kind === "choose" && phase.info && (
              <p className="mt-3 text-neutral-600 dark:text-neutral-300" role="status">
                {phase.info}
              </p>
            )}
            {phase.kind === "exporting" ? (
              <div className="mt-5 flex items-end gap-3">
                <ProgressBar
                  className="flex-1"
                  label={phase.cancelling ? "Cancelling" : (phase.progress?.label ?? "Starting")}
                  fraction={phase.cancelling ? null : (phase.progress?.fraction ?? null)}
                />
                <Button variant="ghost" onClick={cancel} disabled={phase.cancelling}>
                  Cancel
                </Button>
              </div>
            ) : (
              <div className="mt-5 flex justify-end gap-2">
                <Button variant="ghost" onClick={onClose}>
                  Close
                </Button>
                <Button variant="primary" onClick={() => void start()} disabled={customBad || choices === null}>
                  Export…
                </Button>
              </div>
            )}
          </>
        )}
      </div>
    </div>
  );
}

function Done({ path, summary, onClose }: { path: string; summary: VideoSummary; onClose: () => void }) {
  const took = Math.max(1, Math.round(summary.elapsedMs / 1000));
  return (
    <>
      <p className="mt-3 flex items-start gap-2" role="status">
        <CheckCircle2 size={16} className="mt-0.5 shrink-0 text-emerald-600" aria-hidden />
        <span>
          Saved {fileName(path)} ({clock(summary.durationMs / 1000)}, {qualityLabel(summary.height, summary.fps)}).
        </span>
      </p>
      <p className="mt-1 text-neutral-500">
        {sizeText(summary.bytes)} · {summary.video}
        {summary.sound ? ` · ${summary.sound}` : " · no sound"} · took {clock(took)}
      </p>
      {summary.notes.length > 0 && (
        <ul className="mt-2 list-disc pl-5 text-neutral-600 dark:text-neutral-300">
          {summary.notes.map((note) => (
            <li key={note}>{note}</li>
          ))}
        </ul>
      )}
      <div className="mt-5 flex justify-end">
        <Button variant="primary" onClick={onClose}>
          Done
        </Button>
      </div>
    </>
  );
}
