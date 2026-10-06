import { AudioLines, Cable, Film, FlaskConical, History, LayoutGrid, Network, PanelLeftClose, PanelLeftOpen } from "lucide-react";
import { type ReactNode, useState } from "react";
import { SIDEBAR_RAIL_BELOW, useWindowWidth } from "../lib/useWidth";
import { useSequencer } from "../state/sequencer";
import { type Screen, useApp } from "../state/store";
import { IconButton } from "./ui";

/** The screens, in the order a show is made: draw it, find the controllers, wire, test, sequence, play. */
export const NAV: { screen: Screen; label: string; icon: ReactNode }[] = [
  { screen: "layout", label: "Layout", icon: <LayoutGrid size={18} aria-hidden /> },
  { screen: "devices", label: "Devices", icon: <Network size={18} aria-hidden /> },
  { screen: "wiring", label: "Wiring", icon: <Cable size={18} aria-hidden /> },
  { screen: "test", label: "Test", icon: <FlaskConical size={18} aria-hidden /> },
  { screen: "sequence", label: "Sequence", icon: <AudioLines size={18} aria-hidden /> },
  { screen: "play", label: "Play", icon: <Film size={18} aria-hidden /> },
  { screen: "history", label: "History", icon: <History size={18} aria-hidden /> },
];

const PREF_KEY = "pixelflow.sidebar";
type Pref = "rail" | "full" | null;

function loadPref(): Pref {
  try {
    const saved = localStorage.getItem(PREF_KEY);
    return saved === "rail" || saved === "full" ? saved : null;
  } catch {
    return null;
  }
}

function savePref(pref: Pref) {
  try {
    if (pref) localStorage.setItem(PREF_KEY, pref);
    else localStorage.removeItem(PREF_KEY);
  } catch {
    // Storage unavailable: the choice holds until the app closes.
  }
}

/**
 * Whether the sidebar shows only icons: in a narrow window, unless the user chose. A choice that
 * matches what the window width would give anyway is forgotten, so the sidebar goes back to
 * following the window.
 */
export function useSidebarRail(): { rail: boolean; toggle: () => void } {
  const width = useWindowWidth();
  const [pref, setPref] = useState<Pref>(loadPref);
  const auto = width < SIDEBAR_RAIL_BELOW;
  const rail = pref === null ? auto : pref === "rail";
  const toggle = () => {
    const next = !rail;
    const chosen: Pref = next === auto ? null : next ? "rail" : "full";
    savePref(chosen);
    setPref(chosen);
  };
  return { rail, toggle };
}

export function Sidebar({ footer }: { footer?: (rail: boolean) => ReactNode }) {
  const screen = useApp((s) => s.screen);
  const setScreen = useApp((s) => s.setScreen);
  const toRecover = useSequencer((s) => s.recoveries.length > 0);
  const { rail, toggle } = useSidebarRail();
  return (
    <nav
      aria-label="Screens"
      data-collapsed={rail}
      className={`flex shrink-0 flex-col gap-1 overflow-y-auto border-r border-neutral-200 p-2 dark:border-neutral-800 ${rail ? "w-14" : "w-44"}`}
    >
      {NAV.map((item) => (
        <button
          key={item.screen}
          type="button"
          data-screen={item.screen}
          aria-current={screen === item.screen ? "page" : undefined}
          data-tip={rail ? item.label : undefined}
          onClick={() => setScreen(item.screen)}
          className={`relative flex items-center gap-2 rounded-md py-2 text-sm ${rail ? "justify-center px-0" : "px-3"} ${
            screen === item.screen
              ? "bg-accent-50 font-medium text-accent-600 dark:bg-accent-600/15 dark:text-accent-400"
              : "text-neutral-600 hover:bg-neutral-200/70 dark:text-neutral-300 dark:hover:bg-neutral-800"
          }`}
        >
          {item.icon}
          <span className={rail ? "sr-only" : undefined}>{item.label}</span>
          {item.screen === "sequence" && toRecover && (
            <span
              className={`h-2 w-2 rounded-full bg-amber-500 ${rail ? "absolute top-1.5 right-2" : "ml-auto"}`}
              title="Unsaved work to recover"
              aria-hidden
            />
          )}
        </button>
      ))}
      <div className="mt-auto flex flex-col gap-2 pt-2">
        {footer?.(rail)}
        <IconButton
          label={rail ? "Show names" : "Show only icons"}
          hint={rail ? "Show the screen names" : "Make the sidebar narrow: icons only"}
          onClick={toggle}
          className={`rounded-md p-2 text-neutral-500 hover:bg-neutral-200/70 dark:hover:bg-neutral-800 ${rail ? "self-center" : "self-start"}`}
        >
          {rail ? <PanelLeftOpen size={16} aria-hidden /> : <PanelLeftClose size={16} aria-hidden />}
        </IconButton>
      </div>
    </nav>
  );
}
