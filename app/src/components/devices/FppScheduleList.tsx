import { Loader2 } from "lucide-react";
import { useEffect, useState } from "react";
import { errorMessage } from "../../api/backend";
import type { ScheduleEntry } from "../../api/types";
import { scheduleRepeat, scheduleStop, scheduleWhen } from "../../lib/fppSchedule";
import { useApp } from "../../state/store";
import { Section } from "./Section";

const KIND_LABEL: Record<ScheduleEntry["kind"], string | null> = { playlist: null, sequence: "Sequence", command: "Command" };

/** The FPP's schedule, read-only (change it on FPP's own Scheduler page). Read once, and again
 * on Refresh; reading changes nothing. */
export function FppScheduleList({ address, turn }: { address: string; turn: number }) {
  const backend = useApp((s) => s.backend);
  const [entries, setEntries] = useState<ScheduleEntry[] | { error: string } | null>(null);
  useEffect(() => {
    if (!backend) return;
    let current = true;
    setEntries(null);
    backend.fppSchedule(address).then(
      (list) => current && setEntries(list),
      (e) => current && setEntries({ error: errorMessage(e) }),
    );
    return () => {
      current = false;
    };
  }, [backend, address, turn]);
  return (
    <Section title="Schedule">
      {entries === null && (
        <p className="flex items-center gap-2 text-neutral-500">
          <Loader2 size={14} className="animate-spin" aria-hidden /> Reading the schedule…
        </p>
      )}
      {entries !== null && !Array.isArray(entries) && <p className="text-neutral-500">Couldn't read the schedule: {entries.error}</p>}
      {Array.isArray(entries) && entries.length === 0 && <p className="text-neutral-500">Nothing is scheduled on this FPP.</p>}
      {Array.isArray(entries) && entries.length > 0 && (
        <ul aria-label="Schedule entries" className="flex flex-col divide-y divide-neutral-100 dark:divide-neutral-800">
          {entries.map((e, i) => {
            const repeat = scheduleRepeat(e.repeat);
            return (
              <li key={i} className={`flex flex-col py-1.5 ${e.enabled ? "" : "text-neutral-500"}`}>
                <span className="flex min-w-0 items-center gap-1.5">
                  <span className="truncate font-medium">{e.name || "(nothing chosen)"}</span>
                  {KIND_LABEL[e.kind] && <span className="shrink-0 text-xs text-neutral-500">{KIND_LABEL[e.kind]}</span>}
                  {!e.enabled && <span className="shrink-0 rounded bg-neutral-100 px-1.5 text-xs dark:bg-neutral-800">Off</span>}
                </span>
                <span className="text-xs text-neutral-600 dark:text-neutral-400">
                  {scheduleWhen(e)}
                  {e.kind !== "command" && ` · ${[repeat, scheduleStop(e.stopType)].filter(Boolean).join(", ")}`}
                </span>
              </li>
            );
          })}
        </ul>
      )}
      <p className="text-xs text-neutral-500">To change the schedule, use the Scheduler page on FPP's web page.</p>
    </Section>
  );
}
