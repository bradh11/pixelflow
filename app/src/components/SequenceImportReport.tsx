import { AlertTriangle, CheckCircle2 } from "lucide-react";
import { useEffect, useRef } from "react";
import type { LookVerdict } from "../api/types";
import { plural, thousands } from "../lib/format";
import { LOOK_HEADING, LOOK_VERDICTS, lookHeadline, looksByVerdict } from "../lib/importLook";
import { useApp } from "../state/store";
import { Button } from "./ui";

/** Each verdict's color: in the bar under the headline, and beside its heading. */
const TONE: Record<LookVerdict, string> = {
  matches: "bg-green-600",
  close: "bg-amber-500",
  different: "bg-red-500",
  noEquivalent: "bg-neutral-400 dark:bg-neutral-500",
};

/** What an xLights sequence import brought in: how much of it comes across looking the same, and
 * what a viewer would notice of the rest. */
export function SequenceImportReport() {
  const report = useApp((s) => s.sequenceImportReport);
  const dismiss = useApp((s) => s.dismissSequenceImportReport);
  const doneRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!report) return;
    doneRef.current?.focus();
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && dismiss();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [report, dismiss]);

  if (!report) return null;
  const { name, summary, looks, notes } = report;
  const headline = lookHeadline(summary.lookMs);
  const total = LOOK_VERDICTS.reduce((sum, v) => sum + summary.lookMs[v], 0);
  const sections = looksByVerdict(looks);
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4">
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="sequence-import-report-title"
        className="flex max-h-[85vh] w-full max-w-xl flex-col rounded-xl border border-neutral-200 bg-white shadow-2xl dark:border-neutral-800 dark:bg-neutral-900"
      >
        <div className="border-b border-neutral-200 p-5 dark:border-neutral-800">
          <h2 id="sequence-import-report-title" className="flex items-center gap-2 text-lg font-semibold">
            <CheckCircle2 size={18} className="text-green-600" /> Imported {name}
          </h2>
          <p className="mt-1 text-sm text-neutral-500">
            {plural(summary.rows, "row")} · {plural(summary.effects, "effect")}
            {summary.skipped > 0 && ` (${thousands(summary.skipped)} not imported)`} · {plural(summary.timingTracks, "timing track")}{" "}
            · {thousands(summary.marks)} marks
            {summary.marksSkipped > 0 && ` (${thousands(summary.marksSkipped)} not imported)`}
          </p>
          {headline && (
            <>
              <p className="mt-3 text-sm font-medium">{headline}</p>
              <div aria-hidden className="mt-2 flex h-1.5 overflow-hidden rounded-full bg-neutral-200 dark:bg-neutral-800">
                {LOOK_VERDICTS.map((verdict) => (
                  <div key={verdict} className={TONE[verdict]} style={{ width: `${(summary.lookMs[verdict] * 100) / total}%` }} />
                ))}
              </div>
            </>
          )}
        </div>
        <div className="flex-1 overflow-auto p-5 text-sm">
          {sections.length === 0 && notes.length === 0 && <p>Everything looks the way it does in xLights.</p>}
          {sections.map(({ verdict, groups }) => (
            <section key={verdict} className="mb-4" aria-label={LOOK_HEADING[verdict]}>
              <h3 className="mb-1.5 flex items-center gap-2 font-medium">
                <span aria-hidden className={`size-2 shrink-0 rounded-full ${TONE[verdict]}`} />
                {LOOK_HEADING[verdict]}
                <span className="font-normal text-neutral-500">
                  {plural(
                    groups.reduce((sum, g) => sum + g.effects, 0),
                    "effect",
                  )}
                </span>
              </h3>
              <ul className="flex flex-col gap-1.5 pl-4">
                {groups.map((group) => (
                  <li key={group.effect}>
                    <span className="font-medium">{group.effect}</span>{" "}
                    <span className="text-neutral-500">({thousands(group.effects)})</span>
                    {group.reasons.length > 0 && <span className="text-neutral-600 dark:text-neutral-300">: {group.reasons.join("; ")}</span>}
                  </li>
                ))}
              </ul>
            </section>
          ))}
          {notes.length > 0 && (
            <>
              <p className="mb-2 text-neutral-500">{sections.length > 0 ? "Also:" : "Worth knowing:"}</p>
              <ul className="flex flex-col gap-1.5 text-amber-700 dark:text-amber-400">
                {notes.map((note, i) => (
                  // Notes can repeat, so they're keyed by position (the list never reorders).
                  <li key={i} className="flex items-start gap-2">
                    <AlertTriangle size={14} className="mt-0.5 shrink-0" /> {note}
                  </li>
                ))}
              </ul>
            </>
          )}
          <p className="mt-4 text-neutral-500">
            It&apos;s open on the Sequence screen as an unsaved sequence. Save it to keep it.
          </p>
        </div>
        <div className="flex justify-end border-t border-neutral-200 p-4 dark:border-neutral-800">
          <Button ref={doneRef} variant="primary" onClick={dismiss}>
            Done
          </Button>
        </div>
      </div>
    </div>
  );
}
