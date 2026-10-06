import { PanelRightClose, PanelRightOpen } from "lucide-react";
import { type ReactNode, useEffect, useState } from "react";
import { useLayoutEditor } from "../../state/layoutEditor";
import { useApp } from "../../state/store";
import { IconButton } from "../ui";

const PREF_KEY = "pixelflow.layoutProperties";

/**
 * When the properties panel shows: "auto" while something is selected, "pinned" always, or
 * "hidden" never (until asked for again).
 */
export type PropertiesPref = "auto" | "pinned" | "hidden";

function loadPref(): PropertiesPref {
  try {
    const saved = localStorage.getItem(PREF_KEY);
    return saved === "pinned" || saved === "hidden" ? saved : "auto";
  } catch {
    return "auto";
  }
}

function savePref(pref: PropertiesPref) {
  try {
    if (pref === "auto") localStorage.removeItem(PREF_KEY);
    else localStorage.setItem(PREF_KEY, pref);
  } catch {
    // Storage unavailable: the choice holds until the screen closes.
  }
}

/**
 * The properties panel beside the canvas. It opens while a prop is selected and otherwise folds to
 * a thin strip, leaving the room to the canvas; the user can keep it open or put it away, and
 * that's remembered. `floating` (a narrow window): open, it shows over the canvas's right edge
 * instead of taking room from it.
 */
export function PropertiesDock({ floating, children }: { floating: boolean; children: ReactNode }) {
  const props = useApp((s) => s.snapshot?.show.props);
  const selected = useLayoutEditor((s) => s.selected);
  const hasSelection = selected.some((id) => props?.some((p) => p.id === id));
  const [pref, setPref] = useState(loadPref);
  const open = pref === "pinned" || (pref === "auto" && hasSelection);
  const choose = (next: PropertiesPref) => {
    savePref(next);
    setPref(next);
  };
  const show = () => choose(hasSelection ? "auto" : "pinned");
  const hide = () => choose(hasSelection ? "hidden" : "auto");

  useEffect(() => {
    if (!floating || !open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !e.defaultPrevented && pref === "pinned") choose("auto");
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  const rail = (
    <div className="relative flex w-9 shrink-0 flex-col items-center rounded-lg border border-neutral-200 bg-white py-1 dark:border-neutral-800 dark:bg-neutral-900">
      <IconButton
        label="Show properties"
        hint={hasSelection ? "Show the selected prop's properties" : "Keep the properties panel open (the photo's settings are under Photo above)"}
        aria-expanded={open}
        className="rounded p-1.5 text-neutral-600 hover:bg-neutral-200/70 dark:text-neutral-300 dark:hover:bg-neutral-800"
        onClick={open ? hide : show}
      >
        <PanelRightOpen size={16} aria-hidden />
      </IconButton>
      {hasSelection && !open && <span aria-hidden className="mt-1 h-1.5 w-1.5 rounded-full bg-accent-500" />}
    </div>
  );
  if (!open) return rail;
  const panel = (
    <div className="relative h-full">
      {children}
      <IconButton
        label="Hide properties"
        hint={hasSelection ? "Put the properties panel away (Show properties brings it back)" : "Put the properties panel away"}
        className="absolute top-1.5 right-1.5 rounded p-1 text-neutral-500 hover:bg-neutral-200/70 dark:hover:bg-neutral-800"
        onClick={hide}
      >
        <PanelRightClose size={15} aria-hidden />
      </IconButton>
    </div>
  );
  if (!floating) return <div className="w-72 shrink-0">{panel}</div>;
  return (
    <>
      {rail}
      <div data-floating className="absolute inset-y-0 right-12 z-20 w-72 rounded-lg shadow-2xl">
        {panel}
      </div>
    </>
  );
}
