import { AlertTriangle, CheckCircle2 } from "lucide-react";
import { useEffect, useRef } from "react";
import { plural, thousands } from "../lib/format";
import { useApp } from "../state/store";
import { Button } from "./ui";

/** What an xLights import brought in, and anything that wasn't imported exactly. */
export function ImportReport() {
  const report = useApp((s) => s.importReport);
  const dismiss = useApp((s) => s.dismissImportReport);
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
        aria-labelledby="import-report-title"
        className="flex max-h-[85vh] w-full max-w-xl flex-col rounded-xl border border-neutral-200 bg-white shadow-2xl dark:border-neutral-800 dark:bg-neutral-900"
      >
        <div className="border-b border-neutral-200 p-5 dark:border-neutral-800">
          <h2 id="import-report-title" className="flex items-center gap-2 text-lg font-semibold">
            <CheckCircle2 size={18} className="text-green-600" /> Imported {name}
          </h2>
          <p className="mt-1 text-sm text-neutral-500">
            {plural(summary.props, "prop")} · {thousands(summary.pixels)} pixels · {plural(summary.controllers, "controller")}{" "}
            · {plural(summary.wired, "prop")} wired · {plural(summary.groups, "group")}
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
          <p className="mt-4 text-neutral-500">
            The show isn't saved yet. Save it to keep it (it's then in your recent shows), and use Play to run a sequence on it.
          </p>
        </div>
        <div className="flex justify-end gap-2 border-t border-neutral-200 p-4 dark:border-neutral-800">
          <Button onClick={dismiss}>Later</Button>
          <Button
            ref={doneRef}
            variant="primary"
            onClick={() => {
              dismiss();
              void useApp.getState().saveAs();
            }}
          >
            Save show…
          </Button>
        </div>
      </div>
    </div>
  );
}
