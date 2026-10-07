import { useEffect, useRef } from "react";
import { useConfirm } from "../state/confirm";
import { Button } from "./ui";

/** The open `confirmAction` question, if any: Cancel (the default, and Escape) or go ahead. */
export function ConfirmDialog() {
  const request = useConfirm((s) => s.request);
  const resolve = useConfirm((s) => s.resolve);
  const cancel = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (!request) return;
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    cancel.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      useConfirm.getState().resolve(false);
    };
    // Ahead of the screen's own keys, so Escape and Delete don't act behind the question.
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      if (opener?.isConnected) opener.focus();
    };
  }, [request]);
  if (!request) return null;
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div
        role="alertdialog"
        aria-modal="true"
        aria-label={request.title}
        aria-describedby="confirm-message"
        className="w-[28rem] max-w-[calc(100vw-2rem)] rounded-lg border border-neutral-200 bg-white p-5 shadow-xl dark:border-neutral-800 dark:bg-neutral-900"
        onKeyDown={(e) => e.stopPropagation()}
      >
        <h2 className="text-lg font-semibold">{request.title}</h2>
        <p id="confirm-message" className="mt-2 text-sm text-neutral-600 dark:text-neutral-300">
          {request.message}
        </p>
        <div className="mt-5 flex justify-end gap-2">
          <Button ref={cancel} onClick={() => resolve(false)}>
            Cancel
          </Button>
          <Button variant="danger" onClick={() => resolve(true)}>
            {request.confirm}
          </Button>
        </div>
      </div>
    </div>
  );
}
