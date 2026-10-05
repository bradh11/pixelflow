import { AlertTriangle, CheckCircle2 } from "lucide-react";
import { useEffect, useRef } from "react";
import { plural, thousands } from "../lib/format";
import { useApp } from "../state/store";
import { Button } from "./ui";

/** What an xLights sequence import brought in, and anything that wasn't imported exactly. */
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
  const { name, summary, notes } = report;
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
            {plural(summary.rows, "row")} · {plural(summary.effects, "effect")} · {plural(summary.timingTracks, "timing track")}{" "}
            · {thousands(summary.marks)} marks
            {summary.marksSkipped > 0 && ` (${thousands(summary.marksSkipped)} not imported)`}
          </p>
          <p className="mt-1 text-sm text-neutral-500">
            {thousands(summary.exact)} exact · {thousands(summary.approximate)} approximated ·{" "}
            {plural(summary.placeholders, "placeholder")}
            {summary.skipped > 0 && ` · ${thousands(summary.skipped)} not imported`}
          </p>
        </div>
        <div className="flex-1 overflow-auto p-5 text-sm">
          {notes.length === 0 ? (
            <p>Everything came in exactly as it is in xLights.</p>
          ) : (
            <>
              <p className="mb-2 text-neutral-500">These weren't imported exactly:</p>
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
          {/* TODO: link to the Sequence screen once the timeline UI lands. */}
          <p className="mt-4 text-neutral-500">
            It&apos;s open as an unsaved sequence. Open the Sequence screen to see and save it.
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
