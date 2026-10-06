import { Loader2 } from "lucide-react";
import { useEffect, useState } from "react";
import { useApp } from "../state/store";
import { Button } from "./ui";

/** "Name your show": asked before a new show's first save, so it isn't saved as "Untitled Show". */
export function NameShowDialog() {
  const offered = useApp((s) => s.naming);
  return offered === null ? null : <NameShowForm offered={offered} />;
}

function NameShowForm({ offered }: { offered: string }) {
  const resolve = useApp((s) => s.resolveNaming);
  const [name, setName] = useState(offered);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      resolve(null);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [resolve]);

  const ok = name.trim().length > 0;
  return (
    <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/40 p-4">
      <form
        role="dialog"
        aria-modal="true"
        aria-labelledby="name-show-title"
        onSubmit={(e) => {
          e.preventDefault();
          if (ok) resolve(name.trim());
        }}
        className="w-full max-w-sm rounded-xl border border-neutral-200 bg-white p-5 shadow-2xl dark:border-neutral-800 dark:bg-neutral-900"
      >
        <h2 id="name-show-title" className="text-base font-semibold">
          Name your show
        </h2>
        <p className="mt-1 text-sm text-neutral-500 dark:text-neutral-400">It&apos;s how you&apos;ll find it in your recent shows.</p>
        <input
          autoFocus
          aria-label="Show name"
          value={name}
          onFocus={(e) => e.currentTarget.select()}
          onChange={(e) => setName(e.target.value)}
          className="mt-3 w-full rounded-md border border-neutral-300 bg-white px-2 py-1.5 text-sm dark:border-neutral-700 dark:bg-neutral-950"
        />
        <div className="mt-5 flex justify-end gap-2">
          <Button onClick={() => resolve(null)}>Cancel</Button>
          <Button type="submit" variant="primary" disabled={!ok}>
            Save…
          </Button>
        </div>
      </form>
    </div>
  );
}

/** Says what's opening ("Opening the file dialog…", "Opening House…") the moment it starts. */
export function OpeningStatus() {
  const opening = useApp((s) => s.opening);
  return (
    <div aria-live="polite" className="pointer-events-none fixed inset-x-0 top-14 z-[55] flex justify-center px-4">
      {opening && (
        <div role="status" className="flex items-center gap-2 rounded-full border border-neutral-200 bg-white/95 px-4 py-2 text-sm shadow-lg dark:border-neutral-800 dark:bg-neutral-900/95">
          <Loader2 size={16} className="animate-spin text-accent-600 dark:text-accent-400" aria-hidden />
          {opening}
        </div>
      )}
    </div>
  );
}
