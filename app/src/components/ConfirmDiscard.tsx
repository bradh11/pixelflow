import { useEffect, useRef, useState } from "react";
import { unsavedWork, useCloseGuard } from "../state/closeGuard";
import { useSequencer } from "../state/sequencer";
import { useApp } from "../state/store";
import { Button } from "./ui";

/** Asks what to do with unsaved work (the show, the open sequence, or both) before the window closes. */
export function ConfirmClose() {
  const asking = useCloseGuard((s) => s.asking);
  const resolve = useCloseGuard((s) => s.resolve);
  const saveRef = useRef<HTMLButtonElement>(null);
  const [busy, setBusy] = useState(false);

  const choose = async (choice: "save" | "discard" | "cancel") => {
    if (busy) return;
    setBusy(true);
    try {
      await resolve(choice);
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    if (!asking) return;
    saveRef.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      void choose("cancel");
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [asking, busy]); // eslint-disable-line react-hooks/exhaustive-deps

  if (!asking) return null;
  const work = unsavedWork();
  const what = [work.show && `the show “${work.show}”`, work.sequence && `the sequence “${work.sequence}”`].filter(Boolean).join(" and ");
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="confirm-close-title"
        aria-describedby="confirm-close-body"
        className="w-full max-w-sm rounded-xl border border-neutral-200 bg-white p-5 shadow-2xl dark:border-neutral-800 dark:bg-neutral-900"
      >
        <h2 id="confirm-close-title" className="text-base font-semibold">
          Save your changes before closing?
        </h2>
        <p id="confirm-close-body" className="mt-2 text-sm text-neutral-500 dark:text-neutral-400">
          {what ? `There are unsaved changes to ${what}. ` : ""}They will be lost if you don&apos;t save them.
        </p>
        <div className="mt-5 flex justify-end gap-2">
          <Button disabled={busy} onClick={() => void choose("cancel")}>
            Cancel
          </Button>
          <Button variant="danger" disabled={busy} onClick={() => void choose("discard")}>
            Don&apos;t save
          </Button>
          <Button ref={saveRef} variant="primary" disabled={busy} onClick={() => void choose("save")}>
            Save
          </Button>
        </div>
      </div>
    </div>
  );
}

/** Asks, once, what to do with unsaved changes (the show's, the open sequence's, or both) before
 * something leaves the show: New, Open, a recent show, Import, the demo, or Close show. */
export function ConfirmDiscard() {
  const pending = useApp((s) => s.pendingReplace);
  // Watched so the question follows a save made while it's up.
  useApp((s) => s.snapshot?.dirty);
  useSequencer((s) => s.dirty);
  const resolve = useApp((s) => s.resolvePendingReplace);
  const saveRef = useRef<HTMLButtonElement>(null);
  const [busy, setBusy] = useState(false);

  const choose = async (choice: "save" | "discard" | "cancel") => {
    if (busy) return;
    setBusy(true);
    try {
      await resolve(choice);
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    if (!pending) return;
    saveRef.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") void choose("cancel");
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [pending, busy]);

  if (!pending) return null;
  const work = unsavedWork();
  const names = [work.show, work.sequence].filter((n): n is string => n !== null);
  const both = names.length === 2;
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="confirm-discard-title"
        className="w-full max-w-sm rounded-xl border border-neutral-200 bg-white p-5 shadow-2xl dark:border-neutral-800 dark:bg-neutral-900"
      >
        <h2 id="confirm-discard-title" className="text-base font-semibold">
          Save changes to {names.length > 0 ? names.join(" and ") : "this show"}?
        </h2>
        <p className="mt-2 text-sm text-neutral-500 dark:text-neutral-400">
          {both
            ? "The show and its open sequence both have unsaved changes. They will be lost if you don't save them."
            : work.sequence
              ? "The open sequence closes with the show. Your changes will be lost if you don't save them."
              : "Your changes will be lost if you don't save them."}
        </p>
        <div className="mt-5 flex justify-end gap-2">
          <Button disabled={busy} onClick={() => void choose("cancel")}>Cancel</Button>
          <Button variant="danger" disabled={busy} onClick={() => void choose("discard")}>
            Don&apos;t save
          </Button>
          <Button ref={saveRef} variant="primary" disabled={busy} onClick={() => void choose("save")}>
            {both ? "Save all" : "Save"}
          </Button>
        </div>
      </div>
    </div>
  );
}

/** Asks what to do with the open sequence's unsaved changes before something replaces it: New,
 * Open, Recover, or an xLights sequence import (from the Sequence screen or the command palette). */
export function ConfirmReplaceSequence() {
  const pending = useSequencer((s) => s.replacing !== null);
  const name = useSequencer((s) => s.doc?.name ?? "this sequence");
  const resolve = useSequencer((s) => s.resolveReplacing);
  const saveRef = useRef<HTMLButtonElement>(null);
  const [busy, setBusy] = useState(false);

  const choose = async (choice: "save" | "discard" | "cancel") => {
    if (busy) return;
    setBusy(true);
    try {
      await resolve(choice);
    } finally {
      setBusy(false);
    }
  };

  // Save has the focus; Escape is Cancel.
  useEffect(() => {
    if (!pending) return;
    saveRef.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      void choose("cancel");
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [pending, busy]); // eslint-disable-line react-hooks/exhaustive-deps

  if (!pending) return null;
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Unsaved changes"
        aria-describedby="confirm-replace-sequence-body"
        className="w-full max-w-sm rounded-xl border border-neutral-200 bg-white p-5 shadow-2xl dark:border-neutral-800 dark:bg-neutral-900"
      >
        <h2 className="text-base font-semibold">Save changes to {name}?</h2>
        <p id="confirm-replace-sequence-body" className="mt-2 text-sm text-neutral-500 dark:text-neutral-400">
          Your changes will be lost if you don&apos;t save them.
        </p>
        <div className="mt-5 flex justify-end gap-2">
          <Button disabled={busy} onClick={() => void choose("cancel")}>
            Cancel
          </Button>
          <Button variant="danger" disabled={busy} onClick={() => void choose("discard")}>
            Don&apos;t save
          </Button>
          <Button ref={saveRef} variant="primary" disabled={busy} onClick={() => void choose("save")}>
            Save
          </Button>
        </div>
      </div>
    </div>
  );
}
