import { ArrowLeft, Camera, Film, Play, Square, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { errorMessage } from "../api/backend";
import { MemoryBackend } from "../api/memory";
import type { CameraMapPlan, CameraMapTargetInfo, CodeBase, DecodedCapture, OutputStatus, TargetSpec } from "../api/types";
import { Button, Card, EmptyState, Field, PageHeader, Select } from "../components/ui";
import { type ApplyChoice, canFit, canMeasure, defaultChoice, describeAnomaly, groupAnomalies, placementEdits } from "../lib/cameraMap";
import { type CaptureRead, type FrameSource, type ReadProgress, openVideo, readCapture } from "../lib/captureFrames";
import { thousands } from "../lib/format";
import { toastWithUndo } from "../state/undoToast";
import { useApp } from "../state/store";

/** A capture read for a target, with the target and code it was recorded with. */
interface Capture extends CaptureRead {
  target: TargetSpec;
  pixels: number;
  base: CodeBase;
  name: string;
}

const STEP_LABEL: Record<ReadProgress["step"], string> = {
  sync: "Finding the start of the sequence",
  frames: "Reading the flashes",
  decode: "Finding and reading pixels",
};

/** The targets a capture can cover (as on the Test screen). */
function useTargets(): { value: string; label: string; spec: TargetSpec }[] {
  const show = useApp((s) => s.snapshot?.show);
  return useMemo(
    () => [
      { value: "show", label: "Whole show", spec: { type: "show" } as TargetSpec },
      ...(show?.props ?? []).map((p) => ({ value: `prop:${p.id}`, label: `Prop: ${p.name}`, spec: { type: "prop", id: p.id } as TargetSpec })),
      ...(show?.groups ?? []).map((g) => ({ value: `group:${g.id}`, label: `Group: ${g.name}`, spec: { type: "group", id: g.id } as TargetSpec })),
      ...(show?.controllers ?? []).flatMap((c) => [
        { value: `controller:${c.id}`, label: `Controller: ${c.name}`, spec: { type: "controller", id: c.id } as TargetSpec },
        ...c.ports.map((p) => ({
          value: `port:${c.id}:${p.number}`,
          label: `${c.name} · port ${p.number}`,
          spec: { type: "port", controller: c.id, port: p.number } as TargetSpec,
        })),
      ]),
    ],
    [show],
  );
}

/** Places pixels by flashing a code on them and reading a phone video of it. */
export function CameraMapScreen() {
  const snapshot = useApp((s) => s.snapshot);
  const backend = useApp((s) => s.backend);
  const targets = useTargets();
  const [targetValue, setTargetValue] = useState(() => useApp.getState().testTarget);
  const [base, setBase] = useState<CodeBase>("four");
  const [info, setInfo] = useState<CameraMapTargetInfo | null>(null);
  const [infoError, setInfoError] = useState<string | null>(null);
  const [status, setStatus] = useState<OutputStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [progress, setProgress] = useState<ReadProgress | null>(null);
  const reading = useRef<AbortController | null>(null);
  const [capture, setCapture] = useState<Capture | null>(null);
  const [anchors, setAnchors] = useState<number[]>([]);
  const [plan, setPlan] = useState<CameraMapPlan | null>(null);
  const [choices, setChoices] = useState<Record<string, ApplyChoice>>({});
  const fileInput = useRef<HTMLInputElement>(null);

  const target = (targets.find((t) => t.value === targetValue) ?? targets[0]).spec;
  const targetKey = JSON.stringify(target);

  useEffect(() => {
    if (!backend) return;
    let cancelled = false;
    backend.cameraMapTarget(JSON.parse(targetKey) as TargetSpec, base).then(
      (next) => !cancelled && (setInfo(next), setInfoError(null)),
      (e) => !cancelled && (setInfo(null), setInfoError(errorMessage(e))),
    );
    return () => {
      cancelled = true;
    };
  }, [backend, targetKey, base, snapshot?.revision]);

  useEffect(() => {
    if (!backend) return;
    let cancelled = false;
    const poll = () => backend.outputStatus().then((s) => !cancelled && setStatus(s), () => undefined);
    void poll();
    const timer = setInterval(poll, 500);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [backend]);

  // Lining up again whenever the anchors (or the layout) change.
  useEffect(() => {
    if (!backend || !capture || anchors.length === 1) return;
    let cancelled = false;
    backend.cameraMapPlan(capture.target, capture.pixels, capture.decoded, anchors).then(
      (next) => {
        if (cancelled) return;
        setPlan(next);
        setChoices((old) => {
          const show = useApp.getState().snapshot?.show;
          const out: Record<string, ApplyChoice> = {};
          next.props.forEach((p, i) => {
            const prop = show?.props.find((q) => q.id === p.prop);
            out[p.prop] = old[p.prop] ?? (prop ? defaultChoice(prop, p, next.plan.props[i]) : "skip");
          });
          return out;
        });
      },
      (e) => !cancelled && setError(errorMessage(e)),
    );
    return () => {
      cancelled = true;
    };
  }, [backend, capture, anchors, snapshot?.revision]);

  useEffect(() => () => reading.current?.abort(), []);

  if (!snapshot || !backend) return null;
  const show = snapshot.show;
  const flashing = !!status?.running && (status.pattern?.kind === "cameraMap" || status.pattern?.kind === "cameraMapBinary");
  const samples = backend instanceof MemoryBackend && backend.cameraMapSamples ? backend : null;

  const start = async () => {
    try {
      setStatus(await backend.startOutput({ kind: base === "four" ? "cameraMap" : "cameraMapBinary", color: "ffffff" }, target));
      setError(null);
    } catch (e) {
      setError(errorMessage(e));
    }
  };
  const stop = async () => {
    try {
      setStatus(await backend.stopOutput());
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const read = async (open: () => Promise<FrameSource>, name: string) => {
    if (!info) return;
    reading.current?.abort();
    const abort = new AbortController();
    reading.current = abort;
    setError(null);
    setProgress({ step: "sync", done: 0 });
    let source: FrameSource | null = null;
    try {
      source = await open();
      const result = await readCapture(source, backend, info.pixels, base, setProgress, abort.signal);
      setAnchors([]);
      setPlan(null);
      setChoices({});
      setCapture({ ...result, target, pixels: info.pixels, base, name });
    } catch (e) {
      if (!abort.signal.aborted) setError(errorMessage(e));
    } finally {
      source?.dispose();
      if (reading.current === abort) {
        reading.current = null;
        setProgress(null);
      }
    }
  };

  const place = async () => {
    if (!plan) return;
    const edits = placementEdits(show, plan, choices);
    if (edits.length === 0) return;
    const revision = await useApp.getState().edit(edits);
    toastWithUndo(`Placed ${edits.length === 1 ? "1 prop" : `${edits.length} props`} from the camera`, revision);
  };

  const names = plan?.props.map((p) => p.name) ?? [];
  const placing = plan ? plan.props.filter((p) => (choices[p.prop] ?? "skip") !== "skip").length : 0;

  return (
    <div className="mx-auto max-w-5xl">
      <PageHeader
        title="Camera mapping"
        description="Find where every pixel really is: the lights flash a code, you film them with your phone, and PixelFlow reads the video."
        actions={
          <Button variant="ghost" onClick={() => useApp.getState().setScreen("test")}>
            <ArrowLeft size={16} aria-hidden /> Test
          </Button>
        }
      />
      {show.props.length === 0 ? (
        <EmptyState title="No props to map">
          <p>Add props on the Layout screen and wire them to a controller first.</p>
        </EmptyState>
      ) : (
        <div className="flex flex-col gap-4">
          <Card>
            <h2 className="mb-3 text-sm font-semibold">1 · What to map</h2>
            <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
              <Field label="Target">
                <Select value={targetValue} onChange={(e) => setTargetValue(e.target.value)}>
                  {targets.map((t) => (
                    <option key={t.value} value={t.value}>
                      {t.label}
                    </option>
                  ))}
                </Select>
              </Field>
              <Field label="Code">
                <Select value={base} onChange={(e) => setBase(e.target.value as CodeBase)}>
                  <option value="four">Colour: red, green, blue (shorter)</option>
                  <option value="two">White only (single-colour lights)</option>
                </Select>
              </Field>
            </div>
            {info && (
              <p className="mt-2 text-sm text-neutral-600 dark:text-neutral-400">
                {thousands(info.pixels)} pixels on {info.props.length === 1 ? info.props[0].name : `${info.props.length} props`}. The sequence takes{" "}
                {Math.ceil(info.seconds)} seconds and repeats.
              </p>
            )}
            {infoError && <p className="mt-2 text-sm text-amber-700 dark:text-amber-400">{infoError}</p>}
          </Card>

          <Card>
            <h2 className="mb-2 text-sm font-semibold">2 · Film the lights</h2>
            <ol className="mb-3 list-decimal space-y-1 pl-5 text-sm">
              <li>Darken the area: switch off other lights so only the pixels show.</li>
              <li>Hold your phone steady (a tripod or a ledge is best), in landscape, with every pixel of the target in view. Don't zoom.</li>
              <li>Start recording, then press Start flashing.</li>
              <li>
                Record the whole sequence: at least {info ? Math.ceil(info.seconds * 1.3) : "—"} seconds. Then stop recording and press Stop.
              </li>
            </ol>
            <div className="flex flex-wrap items-center gap-2">
              <Button variant="primary" onClick={() => void start()} disabled={!info}>
                <Play size={16} aria-hidden /> {flashing ? "Start again" : "Start flashing"}
              </Button>
              <Button onClick={() => void stop()} disabled={!status?.running}>
                <Square size={16} aria-hidden /> Stop
              </Button>
              <span className="text-sm text-neutral-500" role="status">
                {flashing ? "Flashing the code on the lights…" : status?.running ? "Another test pattern is running." : "The lights aren't flashing."}
              </span>
            </div>
          </Card>

          <Card>
            <h2 className="mb-2 text-sm font-semibold">3 · Read the video</h2>
            <p className="mb-3 text-sm text-neutral-600 dark:text-neutral-400">
              Copy the video to this computer (AirDrop, a cable, or a cloud folder) and choose it. MP4 (H.264) works everywhere; iPhone HEVC videos work on a Mac.
            </p>
            <div className="flex flex-wrap items-center gap-2">
              <input
                ref={fileInput}
                type="file"
                accept="video/*"
                className="hidden"
                aria-label="Video file"
                onChange={(e) => {
                  const file = e.target.files?.[0];
                  e.target.value = "";
                  if (file) void read(() => openVideo(file), file.name);
                }}
              />
              <Button variant="primary" disabled={!info || !!progress} onClick={() => fileInput.current?.click()}>
                <Film size={16} aria-hidden /> Choose video…
              </Button>
              {samples && (
                <Button disabled={!info || !!progress} onClick={() => void read(async () => samples.sampleCapture(target, base), "Sample video")}>
                  <Camera size={16} aria-hidden /> Use a sample video
                </Button>
              )}
              {progress && (
                <>
                  <div className="flex min-w-48 flex-1 items-center gap-2 text-sm" role="status">
                    <span>{STEP_LABEL[progress.step]}…</span>
                    <progress className="h-2 flex-1" max={1} value={progress.done} aria-label="Reading the video" />
                  </div>
                  <Button variant="ghost" onClick={() => reading.current?.abort()}>
                    <X size={16} aria-hidden /> Cancel
                  </Button>
                </>
              )}
            </div>
            {error && (
              <p role="alert" className="mt-3 text-sm text-red-600 dark:text-red-400">
                {error}
              </p>
            )}
          </Card>

          {capture && (
            <Card>
              <h2 className="mb-1 text-sm font-semibold">4 · Check and place</h2>
              <p className="mb-3 text-sm text-neutral-600 dark:text-neutral-400">
                {capture.name}: found {thousands(capture.decoded.pixels.length)} of {thousands(capture.pixels)} pixels.{" "}
                {anchors.length === 0
                  ? "Lined up with the layout by every pixel. To line up by particular pixels instead, click two or three that are already in the right place in the layout."
                  : anchors.length === 1
                    ? "Click one or two more pixels to line up by."
                    : `Lined up by ${anchors.length} pixels.`}{" "}
                {anchors.length > 0 && (
                  <button type="button" className="text-accent-600 underline dark:text-accent-400" onClick={() => setAnchors([])}>
                    Line up by every pixel
                  </button>
                )}
              </p>
              <CaptureOverlay decoded={capture.decoded} picture={capture.picture} anchors={anchors} onPick={(index) => setAnchors((a) => toggleAnchor(a, index))} />
              <Legend />
              {plan && plan.plan.anomalies.length > 0 && (
                <section aria-label="Worth checking" className="mt-4">
                  <h3 className="mb-1 text-xs font-medium tracking-wide text-neutral-500 uppercase">Worth checking</h3>
                  <ul className="flex flex-col gap-1.5 text-sm">
                    {groupAnomalies(plan.plan.anomalies).map((a, i) => (
                      <li key={i} className="flex flex-wrap items-center gap-2">
                        <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-amber-500" aria-hidden />
                        <span className="min-w-0 flex-1">{describeAnomaly(a, names)}</span>
                        {a.kind === "colorOrder" && (
                          <Button
                            className="px-2! py-0.5! text-xs"
                            onClick={() =>
                              void useApp.getState().edit((s) => {
                                const prop = s.props.find((p) => p.id === plan.props[a.prop]?.prop);
                                return prop ? [{ type: "updateProp", prop: { ...prop, colorOrder: a.suggested as typeof prop.colorOrder } }] : [];
                              })
                            }
                          >
                            Use {a.suggested}
                          </Button>
                        )}
                      </li>
                    ))}
                  </ul>
                </section>
              )}
              {plan && (
                <table className="mt-4 w-full text-sm">
                  <thead>
                    <tr className="text-left text-xs tracking-wide text-neutral-500 uppercase">
                      <th className="pb-1 font-medium">Prop</th>
                      <th className="pb-1 text-right font-medium">Found</th>
                      <th className="pb-1 pl-4 font-medium">Place it</th>
                    </tr>
                  </thead>
                  <tbody>
                    {plan.props.map((p, i) => {
                      const prop = show.props.find((q) => q.id === p.prop);
                      const result = plan.plan.props[i];
                      if (!prop || !result) return null;
                      return (
                        <tr key={p.prop} className="border-t border-neutral-200 dark:border-neutral-800">
                          <td className="py-1.5">
                            {p.name}
                            {p.covered < p.nodes && <span className="ml-2 text-xs text-neutral-500">(only part of it was in this capture)</span>}
                          </td>
                          <td className="text-right tabular-nums">
                            {thousands(result.found)} / {thousands(result.nodes)}
                          </td>
                          <td className="pl-4">
                            <Select
                              aria-label={`Place ${p.name}`}
                              value={choices[p.prop] ?? "skip"}
                              onChange={(e) => setChoices((c) => ({ ...c, [p.prop]: e.target.value as ApplyChoice }))}
                            >
                              <option value="measured" disabled={!canMeasure(p, result)}>
                                Measured shape (exactly as filmed)
                              </option>
                              <option value="fit" disabled={!canFit(prop, result)}>
                                Keep its shape, move it into place
                              </option>
                              <option value="skip">Leave as it is</option>
                            </Select>
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              )}
              <div className="mt-4 flex items-center gap-2">
                <Button variant="primary" disabled={!plan || placing === 0} onClick={() => void place()}>
                  Place {placing === 1 ? "1 prop" : `${placing} props`}
                </Button>
                <span className="text-xs text-neutral-500">One step to undo.</span>
              </div>
            </Card>
          )}
        </div>
      )}
    </div>
  );
}

/** Adds a pixel to the anchors (at most three: a fourth replaces the oldest), or removes it. */
export function toggleAnchor(anchors: number[], index: number): number[] {
  if (anchors.includes(index)) return anchors.filter((a) => a !== index);
  return [...anchors, index].slice(-3);
}

function Legend() {
  const item = (color: string, label: string) => (
    <span className="flex items-center gap-1.5">
      <span className="h-2.5 w-2.5 rounded-full" style={{ background: color }} aria-hidden />
      {label}
    </span>
  );
  return (
    <div className="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-xs text-neutral-600 dark:text-neutral-400">
      {item(OVERLAY.found, "Read clearly")}
      {item(OVERLAY.unsure, "Read, less sure")}
      {item(OVERLAY.anchor, "Lining up by")}
      {item(OVERLAY.duplicate, "Seen twice (reflection)")}
      {item(OVERLAY.unreadable, "Couldn't read")}
    </div>
  );
}

const OVERLAY = { found: "#22c55e", unsure: "#f59e0b", anchor: "#3b82f6", duplicate: "#ef4444", unreadable: "#a3a3a3" };

/** The capture with every pixel lit, and what was found drawn over it. Click a pixel to line up by it. */
function CaptureOverlay({ decoded, picture, anchors, onPick }: { decoded: DecodedCapture; picture: Uint8ClampedArray; anchors: number[]; onPick: (index: number) => void }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const ctx = canvas.current?.getContext("2d");
    if (!ctx) return;
    const { width, height } = decoded;
    if (picture.length === width * height * 4) ctx.putImageData(new ImageData(new Uint8ClampedArray(picture), width, height), 0, 0);
    ctx.lineWidth = 1.5;
    const ring = (x: number, y: number, r: number, color: string) => {
      ctx.strokeStyle = color;
      ctx.beginPath();
      ctx.arc(x, y, r, 0, Math.PI * 2);
      ctx.stroke();
    };
    for (const u of decoded.unreadable) ring(u.x, u.y, 4, OVERLAY.unreadable);
    for (const d of decoded.duplicates) {
      ctx.strokeStyle = OVERLAY.duplicate;
      ctx.beginPath();
      ctx.moveTo(d.x - 4, d.y - 4);
      ctx.lineTo(d.x + 4, d.y + 4);
      ctx.moveTo(d.x + 4, d.y - 4);
      ctx.lineTo(d.x - 4, d.y + 4);
      ctx.stroke();
    }
    for (const p of decoded.pixels) ring(p.x, p.y, 3.5, p.confidence >= 0.5 ? OVERLAY.found : OVERLAY.unsure);
    ctx.font = "bold 12px system-ui";
    anchors.forEach((index, n) => {
      const p = decoded.pixels.find((f) => f.index === index);
      if (!p) return;
      ctx.lineWidth = 2.5;
      ring(p.x, p.y, 8, OVERLAY.anchor);
      ctx.fillStyle = OVERLAY.anchor;
      ctx.fillText(String(n + 1), p.x + 10, p.y - 8);
    });
  }, [decoded, picture, anchors]);
  return (
    <canvas
      ref={canvas}
      width={decoded.width}
      height={decoded.height}
      aria-label={`The video with ${decoded.pixels.length} pixels found`}
      className="w-full cursor-crosshair rounded-md border border-neutral-200 bg-black dark:border-neutral-800"
      onClick={(e) => {
        const box = e.currentTarget.getBoundingClientRect();
        if (!box.width) return;
        const x = ((e.clientX - box.left) * decoded.width) / box.width;
        const y = ((e.clientY - box.top) * decoded.height) / box.height;
        let best: { index: number; d: number } | null = null;
        for (const p of decoded.pixels) {
          const d = Math.hypot(p.x - x, p.y - y);
          if (d < 10 && (!best || d < best.d)) best = { index: p.index, d };
        }
        if (best) onPick(best.index);
      }}
    />
  );
}
