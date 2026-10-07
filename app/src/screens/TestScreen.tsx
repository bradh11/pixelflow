import { Play, RefreshCw, Square } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { errorMessage } from "../api/backend";
import type { ControllerCheck, OutputStatus, PatternKind, TargetSpec } from "../api/types";
import { Button, Card, EmptyState, Field, Input, PageHeader, Select } from "../components/ui";
import { thousands } from "../lib/format";
import { type TestDestination, describeUse, isDemoShow, sendingSummary, testDestinations } from "../lib/testTargets";
import { currentSetupKey, useSetup } from "../state/setup";
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

const NOT_ANSWERING = "Not answering — check it's powered on and on the same network as this computer";

/** Whether each listed controller answers: looked at again whenever `when` changes (the target, or Check again). */
function useReachability(addresses: string[], when: string): Map<string, ControllerCheck> | null {
  const backend = useApp((s) => s.backend);
  const key = addresses.join("\n");
  const [result, setResult] = useState<{ key: string; when: string; checks: Map<string, ControllerCheck> } | null>(null);
  useEffect(() => {
    if (!backend || !key) return;
    let cancelled = false;
    backend
      .checkControllers(key.split("\n"))
      .then((checks) => {
        if (!cancelled) setResult({ key, when, checks: new Map(checks.map((c) => [c.address, c])) });
      })
      .catch(() => {
        // Not knowing isn't a problem worth showing: the rows stay "Checking…" until Check again.
      });
    return () => {
      cancelled = true;
    };
  }, [backend, key, when]);
  return result && result.key === key && result.when === when ? result.checks : null;
}

/** The controllers the chosen target sends to, each with whether it answers. */
function SendsTo({ destinations, checks, onCheckAgain }: { destinations: TestDestination[]; checks: Map<string, ControllerCheck> | null; onCheckAgain: () => void }) {
  return (
    <section aria-label="Sends to" className="mt-3 border-t border-neutral-200 pt-2 dark:border-neutral-800">
      <div className="mb-1 flex items-center justify-between gap-2">
        <h2 className="text-xs font-medium tracking-wide text-neutral-500 uppercase">Sends to</h2>
        <Button variant="ghost" className="px-2! py-0.5! text-xs" onClick={onCheckAgain} title="Check again whether each controller answers">
          <RefreshCw size={12} aria-hidden /> Check again
        </Button>
      </div>
      <ul className="flex flex-col gap-1 text-sm">
        {destinations.map((d) => {
          const check = checks?.get(d.controller.address.trim());
          const state = !checks ? "checking" : check?.answering ? "answering" : "silent";
          return (
            <li key={d.controller.id} className="flex flex-wrap items-center gap-x-2 gap-y-0.5" data-reach={state}>
              <span
                aria-hidden
                className={`h-2 w-2 shrink-0 rounded-full ${state === "answering" ? "bg-green-500" : state === "silent" ? "bg-amber-500" : "bg-neutral-400"}`}
              />
              <span className="font-medium">{d.controller.name}</span>
              <span className="font-mono text-xs text-neutral-600 dark:text-neutral-400">{d.controller.address}</span>
              <span className="text-xs text-neutral-500 tabular-nums">{describeUse(d)}</span>
              <span
                className={`text-xs ${state === "answering" ? "text-green-700 dark:text-green-400" : state === "silent" ? "text-amber-700 dark:text-amber-400" : "text-neutral-500"}`}
              >
                {state === "answering" ? "Answering" : state === "silent" ? NOT_ANSWERING : "Checking…"}
              </span>
            </li>
          );
        })}
      </ul>
    </section>
  );
}

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
  /** Whether output runs, as of the latest status (read by a change's timer when it fires). */
  const runningNow = useRef(false);
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
  const targetSpec = (targets.find((t) => t.value === targetValue) ?? targets[0]).spec;
  const channelMap = snapshot?.channelMap;
  const specKey = JSON.stringify(targetSpec);
  const destinations = useMemo(
    () => (show && channelMap ? testDestinations(show, channelMap, JSON.parse(specKey) as TargetSpec) : []),
    [show, channelMap, specKey],
  );
  const [again, setAgain] = useState(0);
  const checks = useReachability(
    destinations.map((d) => d.controller.address.trim()).filter(Boolean),
    `${specKey} ${again}`,
  );
  useEffect(() => {
    if (!targetMissing || !show) return;
    setTargetValue("show");
    setRemoved(true);
  }, [targetMissing, show, setTargetValue]);

  if (!snapshot || !backend || !show) return null;
  const pattern = PATTERNS.find((p) => p.kind === kind)!;
  const nothingToSend = destinations.length === 0;
  const listed = destinations.map((d) => checks?.get(d.controller.address.trim()));
  const elsewhere = isDemoShow(show) || (listed.length > 0 && listed.every((c) => c?.onLocalNetwork === false));

  /** Drops a change still waiting to reach the lights. */
  const cancelLive = () => {
    clearTimeout(live.current);
    live.current = undefined;
  };
  const start = async (next: { kind?: PatternKind; color?: string; target?: string } = {}) => {
    cancelLive();
    const target = (targets.find((t) => t.value === (next.target ?? targetValue)) ?? targets[0]).spec;
    try {
      setStatus(await backend.startOutput({ kind: next.kind ?? kind, color: (next.color ?? color).replace("#", "") }, target));
      setError(null);
      useSetup.getState().markTested(currentSetupKey());
      setRemoved(false);
    } catch (e) {
      setError(errorMessage(e));
    }
  };
  const stop = async () => {
    // A change made just before Stop must not turn the lights back on afterwards.
    cancelLive();
    runningNow.current = false;
    try {
      setStatus(await backend.stopOutput());
      setError(null);
    } catch (e) {
      setError(errorMessage(e));
    }
  };
  const running = status?.running ?? false;
  runningNow.current = running;
  /** While a test runs, a change shows on the lights at once (no Restart needed). */
  const change = (next: { kind?: PatternKind; color?: string; target?: string }) => {
    if (!running) return;
    cancelLive();
    live.current = setTimeout(() => {
      live.current = undefined;
      // Checked when it fires, not when it was asked for: Stop may have come in between.
      if (runningNow.current) void start(next);
    }, LIVE_MS);
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
          <div className="mt-3 flex flex-wrap items-center justify-center gap-2">
            <Button variant="primary" onClick={() => useApp.getState().setScreen("devices")}>
              Find controllers
            </Button>
            <Button onClick={() => useApp.getState().setScreen("wiring")}>Go to Wiring</Button>
          </div>
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
                <Button
                  variant="primary"
                  onClick={() => void start()}
                  disabled={nothingToSend && !running}
                  title={
                    nothingToSend && !running
                      ? "Nothing on this target is wired to a controller, so there's nothing to send"
                      : running
                        ? "Start the test again from the beginning"
                        : "Start sending the test pattern"
                  }
                >
                  <Play size={16} /> {running ? "Restart" : "Start"}
                </Button>
                <Button onClick={stop} disabled={!running}>
                  <Square size={16} /> Stop
                </Button>
              </div>
            </div>
            {nothingToSend ? (
              <div role="status" className="mt-3 flex flex-wrap items-center gap-2 border-t border-neutral-200 pt-2 text-sm dark:border-neutral-800">
                <span className="text-amber-700 dark:text-amber-400">
                  Nothing on this target is wired to a controller yet, so there's nothing to send. Wire its props to a controller port first.
                </span>
                <Button className="px-2! py-0.5! text-xs" onClick={() => useApp.getState().setScreen("wiring")}>
                  Go to Wiring
                </Button>
              </div>
            ) : (
              <SendsTo destinations={destinations} checks={checks} onCheckAgain={() => setAgain((n) => n + 1)} />
            )}
            {elsewhere && (
              <p className="mt-2 flex flex-wrap items-center gap-2 text-xs text-neutral-600 dark:text-neutral-400">
                This show's controllers aren't on your network. Add your own on the Devices screen.
                <Button variant="ghost" className="px-2! py-0.5! text-xs" onClick={() => useApp.getState().setScreen("devices")}>
                  Go to Devices
                </Button>
              </p>
            )}
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
                <span className="min-w-0">
                  <span className="font-medium">Sending {sendingSummary(pattern.label, pattern.usesColor ? color : null, destinations)}</span>
                  <span className="text-neutral-500">
                    {" "}
                    · {status!.achievedFps.toFixed(1)} fps · {thousands(status!.frames)} frames
                  </span>
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
