import { Play, Square } from "lucide-react";
import { useEffect, useState } from "react";
import { errorMessage } from "../api/backend";
import type { OutputStatus, PatternKind, TargetSpec } from "../api/types";
import { Button, Card, EmptyState, Field, PageHeader, Select } from "../components/ui";
import { thousands } from "../lib/format";
import { useApp } from "../state/store";

const PATTERNS: { kind: PatternKind; label: string; usesColor: boolean }[] = [
  { kind: "chase", label: "Chase", usesColor: true },
  { kind: "solid", label: "Solid color", usesColor: true },
  { kind: "cycle", label: "Red / green / blue / white", usesColor: false },
  { kind: "ramp", label: "Brightness ramp", usesColor: true },
  { kind: "alternate", label: "Alternate pixels", usesColor: true },
  { kind: "walk", label: "Pixel walk", usesColor: true },
  { kind: "identify", label: "Identify (blink)", usesColor: false },
];

const STATE_STYLE: Record<string, string> = {
  ok: "text-green-600 dark:text-green-400",
  degraded: "text-amber-600 dark:text-amber-400",
  unresolved: "text-red-600 dark:text-red-400",
};

/** Sends live test patterns to the real controllers. */
export function TestScreen() {
  const snapshot = useApp((s) => s.snapshot);
  const backend = useApp((s) => s.backend);
  const targetValue = useApp((s) => s.testTarget);
  const setTargetValue = useApp((s) => s.setTestTarget);
  const [kind, setKind] = useState<PatternKind>("chase");
  const [color, setColor] = useState("#ffffff");
  const [status, setStatus] = useState<OutputStatus | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!backend) return;
    let cancelled = false;
    const poll = async () => {
      try {
        const next = await backend.outputStatus();
        if (!cancelled) setStatus(next);
      } catch {
        // Polling errors are transient; the next poll retries.
      }
    };
    void poll();
    const timer = setInterval(poll, 500);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [backend]);

  if (!snapshot || !backend) return null;
  const { show } = snapshot;

  const targets: { value: string; label: string; spec: TargetSpec }[] = [
    { value: "show", label: "Whole show", spec: { type: "show" } },
    ...show.props.map((p) => ({ value: `prop:${p.id}`, label: `Prop: ${p.name}`, spec: { type: "prop", id: p.id } as TargetSpec })),
    ...show.groups.map((g) => ({ value: `group:${g.id}`, label: `Group: ${g.name}`, spec: { type: "group", id: g.id } as TargetSpec })),
    ...show.controllers.flatMap((c) => [
      { value: `controller:${c.id}`, label: `Controller: ${c.name}`, spec: { type: "controller", id: c.id } as TargetSpec },
      ...c.ports.map((p) => ({
        value: `port:${c.id}:${p.number}`,
        label: `${c.name} · port ${p.number}`,
        spec: { type: "port", controller: c.id, port: p.number } as TargetSpec,
      })),
    ]),
  ];
  const pattern = PATTERNS.find((p) => p.kind === kind)!;

  const start = async () => {
    const target = targets.find((t) => t.value === targetValue)?.spec;
    if (!target) {
      setError("The chosen target no longer exists. Choose another target.");
      return;
    }
    try {
      setStatus(await backend.startOutput({ kind, color: color.replace("#", "") }, target));
      setError(null);
    } catch (e) {
      setError(errorMessage(e));
    }
  };
  const stop = async () => {
    try {
      setStatus(await backend.stopOutput());
      setError(null);
    } catch (e) {
      setError(errorMessage(e));
    }
  };
  const running = status?.running ?? false;

  return (
    <div className="mx-auto max-w-4xl">
      <PageHeader
        title="Test"
        description="Send a test pattern to your controllers to check wiring and pixel order. Output stops with a blackout."
      />
      {show.controllers.length === 0 ? (
        <EmptyState title="No controllers to test">Add a controller and wire props to it on the Wiring screen.</EmptyState>
      ) : (
        <>
          <Card className="mb-4">
            <div className="grid grid-cols-1 gap-3 md:grid-cols-4">
              <Field label="Target">
                <Select value={targetValue} onChange={(e) => setTargetValue(e.target.value)}>
                  {targets.map((t) => (
                    <option key={t.value} value={t.value}>
                      {t.label}
                    </option>
                  ))}
                </Select>
              </Field>
              <Field label="Pattern">
                <Select value={kind} onChange={(e) => setKind(e.target.value as PatternKind)}>
                  {PATTERNS.map((p) => (
                    <option key={p.kind} value={p.kind}>
                      {p.label}
                    </option>
                  ))}
                </Select>
              </Field>
              <Field label="Color">
                <input
                  type="color"
                  aria-label="Color"
                  value={color}
                  disabled={!pattern.usesColor}
                  onChange={(e) => setColor(e.target.value)}
                  className="h-9 w-full cursor-pointer rounded-md border border-neutral-300 bg-transparent disabled:opacity-40 dark:border-neutral-700"
                />
              </Field>
              <div className="flex items-end gap-2">
                <Button variant="primary" onClick={start}>
                  <Play size={16} /> {running ? "Restart" : "Start"}
                </Button>
                <Button onClick={stop} disabled={!running}>
                  <Square size={16} /> Stop
                </Button>
              </div>
            </div>
            {error && (
              <p role="alert" className="mt-3 text-sm text-red-600 dark:text-red-400">
                {error}
              </p>
            )}
          </Card>
          <Card>
            <div className="mb-3 flex items-center gap-3 text-sm">
              <span className={`h-2.5 w-2.5 rounded-full ${running ? "bg-green-500" : "bg-neutral-400"}`} />
              {running ? (
                <span>
                  Sending · {status!.achievedFps.toFixed(1)} fps · {thousands(status!.frames)} frames
                </span>
              ) : (
                <span className="text-neutral-500">{status?.stopReason ?? "Output stopped"}</span>
              )}
            </div>
            {running && (
              <table className="w-full text-sm">
                <thead>
                  <tr className="text-left text-xs tracking-wide text-neutral-500 uppercase">
                    <th className="pb-1 font-medium">Controller</th>
                    <th className="pb-1 font-medium">State</th>
                    <th className="pb-1 text-right font-medium">Packets</th>
                    <th className="pb-1 text-right font-medium">Errors</th>
                  </tr>
                </thead>
                <tbody>
                  {status!.controllers.map((c) => (
                    <tr key={c.id} className="border-t border-neutral-200 dark:border-neutral-800">
                      <td className="py-1.5">{c.name}</td>
                      <td className={STATE_STYLE[c.state]} title={c.lastError ?? undefined}>
                        {c.state}
                        {c.lastError && <span className="ml-2 text-xs text-neutral-500">{c.lastError}</span>}
                      </td>
                      <td className="text-right tabular-nums">{thousands(c.packetsSent)}</td>
                      <td className="text-right tabular-nums">{thousands(c.sendErrors)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </Card>
        </>
      )}
    </div>
  );
}
