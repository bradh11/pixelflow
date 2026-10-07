import { PanelRightClose } from "lucide-react";
import { type ReactNode, useEffect, useRef } from "react";
import { create } from "zustand";
import { useLayoutEditor } from "../../state/layoutEditor";
import { useApp } from "../../state/store";
import { toast } from "../../state/toast";
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

/** The panel's remembered choice, and whether "it's hidden" has been said yet this time. */
export const usePropertiesPanel = create<{ pref: PropertiesPref; hinted: boolean; choose(pref: PropertiesPref): void }>((set) => ({
  pref: loadPref(),
  hinted: false,
  choose(pref) {
    savePref(pref);
    set({ pref });
  },
}));

/** Whether a prop is selected, whether the panel is open, and how to open or put it away. */
export function usePropertiesOpen(): { open: boolean; hasSelection: boolean; toggle: () => void } {
  const props = useApp((s) => s.snapshot?.show.props);
  const selected = useLayoutEditor((s) => s.selected);
  const pref = usePropertiesPanel((s) => s.pref);
  const hasSelection = selected.some((id) => props?.some((p) => p.id === id));
  const open = pref === "pinned" || (pref === "auto" && hasSelection);
  const { choose } = usePropertiesPanel.getState();
  const toggle = () => (open ? choose(hasSelection ? "hidden" : "auto") : choose(hasSelection ? "auto" : "pinned"));
  return { open, hasSelection, toggle };
}

/**
 * The properties panel beside the canvas. It opens while a prop is selected and otherwise takes
 * no room; the tool bar's Properties button keeps it open or puts it away, and that's remembered.
 * Put away, picking a prop says so once, with a way back. `floating` (a narrow window): open, it
 * shows over the canvas's right edge instead of taking room from it, and Escape or a click
 * outside puts it away.
 */
export function PropertiesDock({ floating, children }: { floating: boolean; children: ReactNode }) {
  const { open, hasSelection, toggle } = usePropertiesOpen();
  const pref = usePropertiesPanel((s) => s.pref);
  const hinted = usePropertiesPanel((s) => s.hinted);
  const box = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (pref !== "hidden" || !hasSelection || hinted) return;
    usePropertiesPanel.setState({ hinted: true });
    toast("The properties panel is hidden.", { label: "Show it", run: () => usePropertiesPanel.getState().choose("auto") }, "info");
  }, [pref, hasSelection, hinted]);

  useEffect(() => {
    if (!floating || !open) return;
    const putAway = () => {
      const { pref: now, choose } = usePropertiesPanel.getState();
      // Kept open by hand with nothing selected: it goes back to opening with a selection.
      if (now === "pinned") choose("auto");
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !e.defaultPrevented) putAway();
    };
    const onDown = (e: PointerEvent) => {
      const t = e.target as Element | null;
      if (box.current?.contains(t) || t?.closest?.("[role=toolbar]")) return;
      putAway();
    };
    window.addEventListener("keydown", onKey);
    window.addEventListener("pointerdown", onDown);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("pointerdown", onDown);
    };
  }, [floating, open]);

  if (!open) return null;
  const panel = (
    <div className="relative h-full">
      {children}
      <IconButton
        label="Hide properties"
        hint={hasSelection ? "Put the properties panel away (Properties above brings it back)" : "Put the properties panel away"}
        className="absolute top-1.5 right-1.5 rounded p-1 text-neutral-500 hover:bg-neutral-200/70 dark:hover:bg-neutral-800"
        onClick={toggle}
      >
        <PanelRightClose size={15} aria-hidden />
      </IconButton>
    </div>
  );
  if (!floating) return <div className="w-72 shrink-0">{panel}</div>;
  return (
    <div ref={box} data-floating className="absolute inset-y-0 right-0 z-20 w-72 rounded-lg shadow-2xl">
      {panel}
    </div>
  );
}
