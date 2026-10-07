import { AlertTriangle, CheckCircle2 } from "lucide-react";
import type { ControllerCheck, Destination, Device, PlayerStatus } from "../../api/types";
import { describeWarning } from "../../lib/fppHealth";
import { Dot, Section } from "./Section";

/**
 * Problems the FPP reports, in plain words with what to do, and whether each controller it sends
 * to answers this computer (the Test screen's quick look: a connection opened and closed).
 */
export function FppHealth({
  address,
  status,
  error,
  destinations,
  reach,
  devices,
}: {
  address: string;
  status: PlayerStatus | null;
  error: string | null;
  destinations: Destination[];
  /** Each output target's answer, by address; missing while checking. */
  reach: Record<string, ControllerCheck>;
  devices: Device[];
}) {
  const problems = (status?.warnings ?? []).map((w) => describeWarning(w, destinations, devices));
  const targets = destinations.filter((d, i) => destinations.findIndex((o) => o.address === d.address) === i);
  const allClear = !error && status !== null && problems.length === 0 && targets.every((d) => reach[d.address]?.answering !== false);
  return (
    <Section title="Health">
      <ul className="flex flex-col gap-2">
        {error && (
          <li className="flex items-start gap-2">
            <AlertTriangle size={16} className="mt-0.5 shrink-0 text-amber-600 dark:text-amber-400" aria-hidden />
            <div>
              <p className="font-medium">This FPP at {address} isn't answering.</p>
              <p className="text-neutral-600 dark:text-neutral-300">Check it's powered on and on the same network as this computer.</p>
              <p className="text-xs text-neutral-500">{error}</p>
            </div>
          </li>
        )}
        {problems.map((p) => (
          <li key={p.title} className="flex items-start gap-2">
            <AlertTriangle size={16} className="mt-0.5 shrink-0 text-amber-600 dark:text-amber-400" aria-hidden />
            <div>
              <p className="font-medium">{p.title}</p>
              <p className="text-neutral-600 dark:text-neutral-300">{p.advice}</p>
              {p.fppSays && <p className="text-xs text-neutral-500">FPP says: {p.fppSays}</p>}
            </div>
          </li>
        ))}
        {allClear && (
          <li className="flex items-center gap-2 text-neutral-700 dark:text-neutral-300">
            <CheckCircle2 size={16} className="shrink-0 text-emerald-600" aria-hidden /> No problems reported.
          </li>
        )}
      </ul>
      {targets.length > 0 && (
        <ul aria-label="Output targets" className="flex flex-col divide-y divide-neutral-100 border-t border-neutral-100 dark:divide-neutral-800 dark:border-neutral-800">
          {targets.map((d) => {
            const check = reach[d.address];
            return (
              <li key={d.address} className="flex items-center justify-between gap-2 py-1.5">
                <span className="min-w-0 truncate">
                  {d.description || d.address} <span className="text-neutral-500 tabular-nums">{d.description ? d.address : ""}</span>
                </span>
                <span className="flex shrink-0 items-center gap-1.5 text-xs">
                  <Dot tone={!check ? "idle" : check.answering ? "ok" : "bad"} />
                  {!check ? "Checking…" : check.answering ? "Answering" : "Not answering"}
                </span>
              </li>
            );
          })}
        </ul>
      )}
    </Section>
  );
}
