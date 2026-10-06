import { CheckCircle2, ChevronDown, Circle, ListChecks, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { type SetupStep, nextStep, setupSteps } from "../lib/setupSteps";
import { setupKey, useSetup } from "../state/setup";
import { recentFor, useSequencer } from "../state/sequencer";
import { useApp } from "../state/store";
import { IconButton } from "./ui";

/** The checklist's steps for the open show, ticked from the show and what's been done with it. */
function useSteps(): { key: string | null; steps: SetupStep[] } | null {
  const snapshot = useApp((s) => s.snapshot);
  const showId = useApp((s) => s.showId);
  const tested = useSetup((s) => s.tested);
  const sequenceOpen = useSequencer((s) => s.doc !== null);
  const recent = useSequencer((s) => s.recent);
  const key = setupKey(snapshot, showId);
  const show = snapshot?.show;
  const isTested = key !== null && tested.includes(key);
  const sequenced = sequenceOpen || (snapshot ? recentFor(recent, snapshot.path).mine.length > 0 : false) || (show?.sequences.length ?? 0) > 0;
  // Worked out only when the show (or what's been done with it) changes.
  const steps = useMemo(() => (show ? setupSteps(show, { tested: isTested, sequenced }) : null), [show, isTested, sequenced]);
  return steps ? { key, steps } : null;
}

function Steps({ steps, onGo }: { steps: SetupStep[]; onGo: () => void }) {
  const screen = useApp((s) => s.screen);
  const next = nextStep(steps);
  return (
    <ol className="flex flex-col">
      {steps.map((step) => {
        const isNext = step === next;
        return (
          <li key={step.id}>
            <button
              type="button"
              data-tip={step.detail}
              aria-current={step.screen === screen ? "page" : undefined}
              onClick={() => {
                useApp.getState().setScreen(step.screen);
                onGo();
              }}
              className={`flex w-full items-start gap-1.5 rounded px-1 py-1 text-left hover:bg-neutral-200/70 dark:hover:bg-neutral-800 ${
                step.done ? "text-neutral-500" : isNext ? "font-medium text-accent-700 dark:text-accent-300" : "text-neutral-700 dark:text-neutral-300"
              }`}
            >
              {step.done ? (
                <CheckCircle2 size={14} className="mt-px shrink-0 text-emerald-600 dark:text-emerald-400" aria-hidden />
              ) : (
                <Circle size={14} className="mt-px shrink-0" aria-hidden />
              )}
              <span className="min-w-0">
                {step.label}
                <span className="sr-only">{step.done ? ", done" : isNext ? ", next" : ""}</span>
                {isNext && <span className="block text-[11px] font-normal text-neutral-500">{step.detail}</span>}
              </span>
            </button>
          </li>
        );
      })}
    </ol>
  );
}

/** The next step as one button: what to do, and how far along it is. */
function NextStep({ step, onGo }: { step: SetupStep; onGo: () => void }) {
  return (
    <button
      type="button"
      aria-label={`Next: ${step.label}, ${step.detail}`}
      onClick={() => {
        useApp.getState().setScreen(step.screen);
        onGo();
      }}
      className="mt-1 flex w-full items-start gap-1.5 rounded px-1 py-1 text-left font-medium text-accent-700 hover:bg-neutral-200/70 dark:text-accent-300 dark:hover:bg-neutral-800"
    >
      <Circle size={14} className="mt-px shrink-0" aria-hidden />
      <span className="min-w-0">
        {step.label}
        <span className="block text-[11px] font-normal text-neutral-500">{step.detail}</span>
      </span>
    </button>
  );
}

function Card({ steps, onAway, onGo, full = false }: { steps: SetupStep[]; onAway: () => void; onGo: () => void; full?: boolean }) {
  const done = steps.filter((s) => s.done).length;
  const next = nextStep(steps);
  const [open, setOpen] = useState(full);
  return (
    <section aria-label="Set up your show" className="rounded-lg border border-neutral-200 bg-white p-2 text-xs dark:border-neutral-800 dark:bg-neutral-900">
      <div className="flex items-center justify-between gap-1">
        <h2 className="font-semibold">Set up your show</h2>
        <IconButton
          label="Put the checklist away"
          hint="Put the checklist away (the command palette brings it back)"
          className="rounded p-0.5 text-neutral-500 hover:bg-neutral-200/70 dark:hover:bg-neutral-800"
          onClick={onAway}
        >
          <X size={14} aria-hidden />
        </IconButton>
      </div>
      <div
        role="progressbar"
        aria-label="Steps done"
        aria-valuemin={0}
        aria-valuemax={steps.length}
        aria-valuenow={done}
        aria-valuetext={`${done} of ${steps.length} done`}
        className="mt-1.5 h-1 overflow-hidden rounded-full bg-neutral-200 dark:bg-neutral-800"
      >
        <div className="h-full rounded-full bg-accent-500" style={{ width: `${(100 * done) / steps.length}%` }} />
      </div>
      <div className="mt-1 flex items-center justify-between text-[11px] text-neutral-500">
        <span>
          {done} of {steps.length} done
        </span>
        {!full && (
          <button
            type="button"
            aria-expanded={open}
            aria-label="Show all steps"
            data-tip={open ? "Show only the next step" : "Show all six steps"}
            onClick={() => setOpen(!open)}
            className="inline-flex items-center gap-0.5 rounded px-1 hover:bg-neutral-200/70 dark:hover:bg-neutral-800"
          >
            {open ? "Less" : "All steps"} <ChevronDown size={12} aria-hidden className={open ? "rotate-180" : ""} />
          </button>
        )}
      </div>
      {open ? <Steps steps={steps} onGo={onGo} /> : next && <NextStep step={next} onGo={onGo} />}
    </section>
  );
}

/**
 * "Set up your show" at the foot of the sidebar: the steps from an empty show to lights playing,
 * each a button to the screen where it's done. In the icons-only sidebar it's a button that shows
 * the checklist beside the sidebar. It can be put away (per show).
 */
export function SetupChecklist({ rail }: { rail: boolean }) {
  const found = useSteps();
  const dismissed = useSetup((s) => (found?.key ? s.dismissed.includes(found.key) : false));
  const [open, setOpen] = useState(false);
  const button = useRef<HTMLButtonElement>(null);
  const pop = useRef<HTMLDivElement>(null);
  const [place, setPlace] = useState<{ left: number; bottom: number } | null>(null);

  useEffect(() => {
    if (!open) return;
    // Beside the button, kept there as the window changes size.
    const placeIt = () => {
      const r = button.current?.getBoundingClientRect();
      if (r) setPlace({ left: Math.round(r.right + 8), bottom: Math.round(window.innerHeight - r.bottom) });
    };
    placeIt();
    pop.current?.querySelector<HTMLElement>("button")?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Tab") {
        // Tab goes round the checklist's buttons rather than out of it.
        const items = [...(pop.current?.querySelectorAll<HTMLElement>("button") ?? [])];
        const at = items.indexOf(document.activeElement as HTMLElement);
        if (items.length === 0) return;
        e.preventDefault();
        items[(at + (e.shiftKey ? -1 : 1) + items.length) % items.length].focus();
        return;
      }
      if (e.key !== "Escape") return;
      setOpen(false);
      button.current?.focus();
    };
    window.addEventListener("resize", placeIt);
    const onDown = (e: PointerEvent) => {
      const t = e.target as Node;
      if (!pop.current?.contains(t) && !button.current?.contains(t)) setOpen(false);
    };
    window.addEventListener("keydown", onKey);
    window.addEventListener("pointerdown", onDown);
    return () => {
      window.removeEventListener("resize", placeIt);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("pointerdown", onDown);
    };
  }, [open]);
  useEffect(() => {
    if (!rail) setOpen(false);
  }, [rail]);

  // Put away by hand, or (once every step is done) by itself.
  if (!found || dismissed || nextStep(found.steps) === null) return null;
  const { key, steps } = found;
  const away = () => {
    setOpen(false);
    useSetup.getState().setDismissed(key, true);
  };
  if (!rail) return <Card steps={steps} onAway={away} onGo={() => undefined} />;

  const done = steps.filter((s) => s.done).length;
  const left = steps.length - done;
  return (
    <>
      <IconButton
        ref={button}
        label={`Set up your show: ${done} of ${steps.length} done`}
        aria-expanded={open}
        aria-haspopup="dialog"
        onClick={() => setOpen(!open)}
        className="relative self-center rounded-md p-2 text-neutral-600 hover:bg-neutral-200/70 dark:text-neutral-300 dark:hover:bg-neutral-800"
      >
        <ListChecks size={18} aria-hidden />
        {left > 0 && (
          <span aria-hidden className="absolute -top-0.5 -right-0.5 min-w-4 rounded-full bg-accent-600 px-1 text-center text-[10px] leading-4 font-semibold text-white">
            {left}
          </span>
        )}
      </IconButton>
      {open &&
        createPortal(
          <div
            ref={pop}
            role="dialog"
            aria-label="Set up your show"
            className="fixed z-40 w-60 shadow-xl"
            style={{ left: place?.left ?? 64, bottom: place?.bottom ?? 48 }}
          >
            <Card steps={steps} onAway={away} onGo={() => setOpen(false)} full />
          </div>,
          document.body,
        )}
    </>
  );
}
