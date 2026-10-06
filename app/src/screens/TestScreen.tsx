import { Play, Square } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { errorMessage } from "../api/backend";
import type { OutputStatus, PatternKind, TargetSpec } from "../api/types";
import { Button, Card, EmptyState, Field, Input, PageHeader, Select } from "../components/ui";
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

/** Quick colors for checking each channel of a pixel. */
const PRESETS: { label: string; color: string }[] = [
  { label: "Red", color: "#ff0000" },
  { label: "Green", color: "#00ff00" },
  { label: "Blue", color: "#0000ff" },
  { label: "White", color: "#ffffff" },
];

/** How long after the last change a running test picks it up (a color drag sends many). */
const LIVE_MS = 150;
const HEX = /^#[0-9a-f]{6}$/i;

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
  const [removed, setRemoved] = useState(false);
  const [hex, setHex] = useState("#ffffff");
  const live = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(live.current), []);

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

  const show = snapshot?.show;
  const targets: { value: string; label: string; spec: TargetSpec }[] = [
    { value: "show", label: "Whole show", spec: { type: "show" } },
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
  ];
  const targetMissing = !targets.some((t) => t.value === targetValue);
  useEffect(() => {
    if (!targetMissing || !show) return;
    setTargetValue("show");
    setRemoved(true);
  }, [targetMissing, show, setTargetValue]);

  if (!snapshot || !backend || !show) return null;
  const pattern = PATTERNS.find((p) => p.kind === kind)!;

  const start = async (next: { kind?: PatternKind; color?: string; target?: string } = {}) => {
    const target = (targets.find((t) => t.value === (next.target ?? targetValue)) ?? targets[0]).spec;
    try {
      setStatus(await backend.startOutput({ kind: next.kind ?? kind, color: (next.color ?? color).replace("#", "") }, target));
      setError(null);
      setRemoved(false);
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
  /** While a test runs, a change shows on the lights at once (no Restart needed). */
  const change = (next: { kind?: PatternKind; color?: string; target?: string }) => {
    if (!running) return;
    clearTimeout(live.current);
    live.current = setTimeout(() => void start(next), LIVE_MS);
  };
  const pickColor = (value: string) => {
    setColor(value);
    setHex(value);
    change({ color: value });
  };

  return (
    <div className="mx-auto max-w-4xl">
      <PageHeader
        title="Test"
        description="Send a test pattern to your controllers to check wiring and pixel order. Output stops with a blackout."
      />
      {show.controllers.length === 0 ? (
        <EmptyState title="No controllers to test">
          <p>Add a controller and wire props to it first.</p>
          <Button variant="primary" className="mt-3" onClick={() => useApp.getState().setScreen("wiring")}>
            Go to Wiring
          </Button>
        </EmptyState>
      ) : (
        <>
          <Card className="mb-4">
            <div className="grid grid-cols-1 gap-3 md:grid-cols-[1fr_1fr_auto_auto]">
              <Field label="Target">
                <Select
                  value={targetValue}
                  onChange={(e) => {
                    setTargetValue(e.target.value);
                    setRemoved(false);
                    change({ target: e.target.value });
                  }}
                >
                  {targets.map((t) => (
                    <option key={t.value} value={t.value}>
                      {t.label}
                    </option>
                  ))}
                </Select>
              </Field>
              <Field label="Pattern">
                <Select
                  value={kind}
                  onChange={(e) => {
                    setKind(e.target.value as PatternKind);
                    change({ kind: e.target.value as PatternKind });
                  }}
                >
                  {PATTERNS.map((p) => (
                    <option key={p.kind} value={p.kind}>
                      {p.label}
                    </option>
                  ))}
                </Select>
              </Field>
              <div className="flex flex-col gap-1 text-sm">
                <span className="text-neutral-600 dark:text-neutral-400">Color</span>
                <div className="flex items-center gap-1.5">
                  <input
                    type="color"
                    aria-label="Color"
                    value={color}
                    disabled={!pattern.usesColor}
                    onChange={(e) => pickColor(e.target.value)}
                    className="h-8 w-9 shrink-0 cursor-pointer rounded-md border border-neutral-300 bg-transparent disabled:opacity-40 dark:border-neutral-700"
                  />
                  <Input
                    aria-label="Color as hex"
                    value={hex}
                    disabled={!pattern.usesColor}
                    spellCheck={false}
                    onChange={(e) => {
                      const typed = e.target.value.trim();
                      setHex(typed);
                      const full = typed.startsWith("#") ? typed : `#${typed}`;
                      if (HEX.test(full)) pickColor(full.toLowerCase());
                    }}
                    onBlur={() => setHex(color)}
                    className="w-20 min-w-0 font-mono text-xs uppercase disabled:opacity-40"
                  />
                  {PRESETS.map((p) => (
                    <button
                      key={p.label}
                      type="button"
                      aria-label={p.label}
                      title={p.label}
                      aria-pressed={color === p.color}
                      disabled={!pattern.usesColor}
                      onClick={() => pickColor(p.color)}
                      style={{ background: p.color }}
                      className="h-6 w-6 shrink-0 rounded-full border border-neutral-300 aria-pressed:ring-2 aria-pressed:ring-accent-500 disabled:opacity-40 dark:border-neutral-600"
                    />
                  ))}
                </div>
              </div>
              <div className="flex items-end gap-2">
                <Button variant="primary" onClick={() => void start()} title={running ? "Start the test again from the beginning" : "Start sending the test pattern"}>
                  <Play size={16} /> {running ? "Restart" : "Start"}
                </Button>
                <Button onClick={stop} disabled={!running}>
                  <Square size={16} /> Stop
                </Button>
              </div>
            </div>
            {running && <p className="mt-3 text-xs text-neutral-500">Changes show on the lights right away while the test runs.</p>}
            {removed && (
              <p role="status" className="mt-3 text-sm text-amber-600 dark:text-amber-400">
                The chosen target was removed; testing the whole show.
              </p>
            )}
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
