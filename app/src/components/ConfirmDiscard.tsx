import { useEffect, useRef } from "react";
import { useApp } from "../state/store";
import { Button } from "./ui";

/** Asks what to do with unsaved changes before New or Open replaces the show. */
export function ConfirmDiscard() {
  const pending = useApp((s) => s.pendingReplace);
  const name = useApp((s) => s.snapshot?.show.name ?? "this show");
  const resolve = useApp((s) => s.resolvePendingReplace);
  const saveRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!pending) return;
    saveRef.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") void resolve("cancel");
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [pending, resolve]);

  if (!pending) return null;
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="confirm-discard-title"
        className="w-full max-w-sm rounded-xl border border-neutral-200 bg-white p-5 shadow-2xl dark:border-neutral-800 dark:bg-neutral-900"
      >
        <h2 id="confirm-discard-title" className="text-base font-semibold">
          Save changes to {name}?
        </h2>
        <p className="mt-2 text-sm text-neutral-500 dark:text-neutral-400">
          Your changes will be lost if you don&apos;t save them.
        </p>
        <div className="mt-5 flex justify-end gap-2">
          <Button onClick={() => void resolve("cancel")}>Cancel</Button>
          <Button variant="danger" onClick={() => void resolve("discard")}>
            Don&apos;t save
          </Button>
          <Button ref={saveRef} variant="primary" onClick={() => void resolve("save")}>
            Save
          </Button>
        </div>
      </div>
    </div>
  );
}
