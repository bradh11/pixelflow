import { CheckCircle2, Download, Film } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import { errorMessage } from "../../api/backend";
import type { Sequence } from "../../api/sequence";
import type { H264Encoder, H264Progress, VideoChoices, VideoProgress, VideoRequest, VideoSummary } from "../../api/video";
import { clock, fileName, sizeText } from "../../lib/format";
import { formatTime, parseTime } from "../../lib/timelineMath";
import { BOOST_CHOICES, DOT_SCALES, hidesUnlit } from "../../lib/viewOptions";
import { useSequencer } from "../../state/sequencer";
import { useView3d } from "../../state/view3d";
import { EncoderConsent, downloadSize } from "../CiscoEncoder";
import { ProgressBar } from "../ProgressBar";
import { Button, Input, Select } from "../ui";

/** The choices last used, remembered on this computer. */
const SETTINGS_KEY = "pixelflow.videoExport";

interface Settings {
  height: 720 | 1080;
  fps: 30 | 60;
  photo: boolean;
  /** Against the preview's; null until one is chosen here: the video then has the previews' Dot size. */
  pixelSize: number | null;
  /** 0 (crisp dots) to 1; null until one is chosen here: the video then glows as the previews do. */
  glow: number | null;
  /** Pixels that are off; null until chosen here: then as the previews show them while the show plays. */
  unlit: "dim" | "hidden" | null;
  /** Boost dim's step; null until one is chosen here: then the previews'. */
  boost: number | null;
  ffmpeg: boolean;
}

const DEFAULTS: Settings = { height: 1080, fps: 30, photo: true, pixelSize: null, glow: null, unlit: null, boost: null, ffmpeg: false };

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
      pixelSize: PIXEL_SIZES.some((p) => p.value === saved.pixelSize) ? saved.pixelSize! : null,
      glow: typeof saved.glow === "number" && Number.isFinite(saved.glow) ? Math.min(1, Math.max(0, saved.glow)) : null,
      unlit: saved.unlit === "dim" || saved.unlit === "hidden" ? saved.unlit : null,
      boost: BOOST_CHOICES.some((b) => b.value === saved.boost) ? saved.boost! : null,
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
  /** Asking before Cisco's encoder is downloaded (the built-in encoder's first export). */
  | { kind: "consent" }
  /** Asking to turn Cisco's encoder back on (it was turned off in Settings). */
  | { kind: "turnOn" }
  | { kind: "downloading"; progress: H264Progress | null; cancelling: boolean }
  | { kind: "downloadFailed"; error: string }
  | { kind: "exporting"; progress: VideoProgress | null; cancelling: boolean }
  | { kind: "done"; path: string; summary: VideoSummary };

/** The steps that are about Cisco's encoder, in place of the choices. */
type EncoderPhase = Extract<Phase, { kind: "consent" | "turnOn" | "downloading" | "downloadFailed" }>;

function isEncoderPhase(phase: Phase): phase is EncoderPhase {
  return phase.kind === "consent" || phase.kind === "turnOn" || phase.kind === "downloading" || phase.kind === "downloadFailed";
}

/**
 * Export video: the sequence as an MP4 of the 2D preview with its music, to share or post. The
 * size, frame rate, stretch of the song, photo, dot size, and encoder are chosen here; then where
 * to save it, and its progress shows here with Cancel. The built-in encoder is Cisco's OpenH264:
 * the first export asks before downloading it, shows the download, then carries straight on.
 */
export function ExportVideoDialog({ onClose }: { onClose: () => void }) {
  const api = useSequencer((s) => s.api);
  const doc = useSequencer((s) => s.doc);
  const path = useSequencer((s) => s.path);
  const selection = useSequencer((s) => s.selection);
  const playheadMs = useSequencer((s) => s.playheadMs);
  const [choices, setChoices] = useState<VideoChoices | null>(null);
  const [h264, setH264] = useState<H264Encoder | null>(null);
  const [settings, setSettings] = useState<Settings>(loadSettings);
  // Until they're chosen here, the video starts with the glow, dot size, pixels that are off, and
  // boost the previews are shown with (as they look while the show plays: a video plays).
  const preview = useView3d(useShallow((s) => ({ glow: s.glow, dotSize: s.dotSize, unlit: s.unlit, boost: s.boost })));
  const glow = settings.glow ?? preview.glow;
  const pixelSize = settings.pixelSize ?? DOT_SCALES[preview.dotSize];
  const unlit = settings.unlit ?? (hidesUnlit(preview.unlit, true) ? "hidden" : "dim");
  const boost = settings.boost ?? preview.boost;
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
  busy.current = exporting || phase.kind === "downloading";
  // Escape on a step about the encoder goes back to the choices.
  const asking = useRef(false);
  asking.current = isEncoderPhase(phase);

  useEffect(() => {
    if (!api) return;
    let live = true;
    api.videoExportChoices().then(
      (c) => live && setChoices(c),
      () => live && setChoices({ ffmpeg: null, photo: false }),
    );
    api.h264Encoder().then(
      (e) => live && setH264(e),
      () => {},
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
      if (asking.current) setPhase({ kind: "choose" });
      else close.current();
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
      if (opener?.isConnected) opener.focus();
    };
  }, []);

  // A step about the encoder takes the focus (its main button), and the choices get it back.
  const encoderStep = isEncoderPhase(phase) ? phase.kind : null;
  useEffect(() => {
    if (encoderStep === "downloading") return;
    const button = encoderStep ? "[data-encoder-step] button[data-primary]" : "select, input, button";
    box.current?.querySelector<HTMLElement>(button)?.focus();
  }, [encoderStep]);

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
    if (!ffmpeg) {
      // The built-in encoder is Cisco's, downloaded only when the user agrees.
      let encoder: H264Encoder;
      try {
        encoder = await api.h264Encoder();
      } catch (e) {
        setPhase({ kind: "choose", error: errorMessage(e) });
        return;
      }
      setH264(encoder);
      if (encoder.state === "unsupported") {
        setPhase({ kind: "choose", error: encoder.problem ?? "The built-in encoder isn't available on this computer." });
        return;
      }
      if (encoder.state === "missing") {
        setPhase({ kind: "consent" });
        return;
      }
      if (encoder.state === "off") {
        setPhase({ kind: "turnOn" });
        return;
      }
    }
    await exportVideo();
  };

  /** Downloads Cisco's encoder (the user has just agreed), then exports. */
  const download = async () => {
    setPhase({ kind: "downloading", progress: null, cancelling: false });
    try {
      setH264(await api.downloadH264Encoder((progress) => setPhase((p) => (p.kind === "downloading" ? { ...p, progress } : p))));
    } catch (e) {
      const message = errorMessage(e);
      setPhase(message === "Stopped." ? { kind: "choose", info: "Download stopped. Nothing was kept." } : { kind: "downloadFailed", error: message });
      return;
    }
    await exportVideo();
  };

  /** Turns Cisco's encoder back on (the user has just said to), then exports. */
  const turnOn = async () => {
    try {
      setH264(await api.setH264EncoderEnabled(true));
    } catch (e) {
      setPhase({ kind: "choose", error: errorMessage(e) });
      return;
    }
    await exportVideo();
  };

  /** Asks where to save the video, then exports it there. */
  const exportVideo = async () => {
    setPhase({ kind: "choose" });
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
      pixelSize,
      glow,
      hideUnlit: unlit === "hidden",
      boost,
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
      // The encoder may have been found changed (and removed), or turned off meanwhile.
      if (!ffmpeg) api.h264Encoder().then(setH264, () => {});
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
        ) : isEncoderPhase(phase) && h264 ? (
          <EncoderStep
            phase={phase}
            encoder={h264}
            onDownload={() => void download()}
            onTurnOn={() => void turnOn()}
            onCancelDownload={() => {
              setPhase((p) => (p.kind === "downloading" ? { ...p, cancelling: true } : p));
              void api.cancelH264Download();
            }}
            onBack={() => setPhase({ kind: "choose" })}
          />
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
                <Select value={pixelSize} onChange={(e) => update({ pixelSize: Number(e.target.value) })}>
                  {PIXEL_SIZES.map((p) => (
                    <option key={p.value} value={p.value}>
                      {p.label}
                    </option>
                  ))}
                </Select>
              </label>
              <label className="flex flex-col gap-1" title="Dim shows every prop; hidden shows only lit pixels, as at night">
                <span className={label}>Pixels that are off</span>
                <Select value={unlit} onChange={(e) => update({ unlit: e.target.value as "dim" | "hidden" })}>
                  <option value="dim">Dim</option>
                  <option value="hidden">Hidden</option>
                </Select>
              </label>
              <label className="flex flex-col gap-1" title="Lifts dim levels so fades show on a screen">
                <span className={label}>Boost dim</span>
                <Select value={boost} onChange={(e) => update({ boost: Number(e.target.value) })}>
                  {BOOST_CHOICES.map((b) => (
                    <option key={b.value} value={b.value}>
                      {b.label}
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
              {!ffmpeg && h264 && h264.state !== "ready" && (
                <p className="col-span-2 text-xs text-neutral-500">
                  {h264.state === "missing"
                    ? `The built-in encoder is Cisco's OpenH264. The first export asks before downloading it (${downloadSize(h264.bytes)}).`
                    : h264.state === "off"
                      ? "Cisco's OpenH264 encoder is turned off in Settings. Exporting asks to turn it back on."
                      : h264.problem}
                </p>
              )}
              <label
                className={`col-span-2 flex items-center gap-2 ${choices?.photo ? "" : "text-neutral-400 dark:text-neutral-500"}`}
                title={choices && !choices.photo ? "Add a photo of the house on the Layout screen to show it here" : undefined}
              >
                <input type="checkbox" checked={photo} disabled={!choices?.photo} onChange={(e) => update({ photo: e.target.checked })} />
                Show the house photo behind the lights
              </label>
              <label className="col-span-2 flex flex-col gap-1" title="None for bare bulbs; more for lights behind diffusers">
                <span className={`${label} flex justify-between`}>
                  <span>Glow</span>
                  <span className="tabular-nums">{glow === 0 ? "None" : `${Math.round(glow * 100)}%`}</span>
                </span>
                <input
                  type="range"
                  min={0}
                  max={100}
                  step={5}
                  value={Math.round(glow * 100)}
                  aria-label="Glow"
                  aria-valuetext={glow === 0 ? "None" : `${Math.round(glow * 100)}%`}
                  onChange={(e) => update({ glow: Number(e.target.value) / 100 })}
                  className="accent-accent-500"
                />
                <span className="text-xs text-neutral-500">None for bare bulbs, more for lights behind diffusers.</span>
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

/**
 * Cisco's encoder, in place of the choices: asking before it's downloaded (what, from where, how
 * big, and why), its download, a download that failed, or asking to turn it back on.
 */
function EncoderStep({
  phase,
  encoder,
  onDownload,
  onTurnOn,
  onCancelDownload,
  onBack,
}: {
  phase: EncoderPhase;
  encoder: H264Encoder;
  onDownload: () => void;
  onTurnOn: () => void;
  onCancelDownload: () => void;
  onBack: () => void;
}) {
  const progress = phase.kind === "downloading" ? phase.progress : null;
  return (
    <div data-encoder-step className="mt-3 flex flex-col gap-3">
      <h3 className="font-semibold">{phase.kind === "turnOn" ? "Turn Cisco's video encoder back on?" : "Download Cisco's video encoder?"}</h3>
      {phase.kind === "turnOn" ? (
        <div className="flex flex-col gap-2 text-sm text-neutral-600 dark:text-neutral-300">
          <p>The built-in encoder is Cisco&apos;s OpenH264. It&apos;s downloaded, and turned off in Settings, under Video export.</p>
          <p className="text-xs text-neutral-500">{encoder.attribution}</p>
        </div>
      ) : (
        <EncoderConsent encoder={encoder} />
      )}
      {phase.kind === "downloadFailed" && (
        <p className="text-red-600 dark:text-red-400" role="alert">
          {phase.error}
        </p>
      )}
      {phase.kind === "downloading" ? (
        <div className="mt-2 flex items-end gap-3">
          <ProgressBar
            className="flex-1"
            label={phase.cancelling ? "Stopping the download" : "Downloading the encoder"}
            fraction={phase.cancelling || !progress || progress.total <= 0 ? null : progress.received / progress.total}
          />
          <Button variant="ghost" onClick={onCancelDownload} disabled={phase.cancelling}>
            Cancel
          </Button>
        </div>
      ) : (
        <div className="mt-2 flex justify-end gap-2">
          <Button variant="ghost" onClick={onBack}>
            Cancel
          </Button>
          {phase.kind === "turnOn" ? (
            <Button variant="primary" data-primary onClick={onTurnOn}>
              Turn on and export
            </Button>
          ) : (
            <Button variant="primary" data-primary onClick={onDownload}>
              <Download size={14} aria-hidden /> {phase.kind === "downloadFailed" ? "Try again" : "Download and export"}
            </Button>
          )}
        </div>
      )}
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
