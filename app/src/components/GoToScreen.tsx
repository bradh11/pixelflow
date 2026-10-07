import { ArrowRight } from "lucide-react";
import type { ReactNode } from "react";
import { type Screen, useApp } from "../state/store";

/** A link-like button to another screen, for empty states that say where to go next. */
export function GoToScreen({ screen, children }: { screen: Screen; children: ReactNode }) {
  return (
    <button
      type="button"
      onClick={() => useApp.getState().setScreen(screen)}
      className="inline-flex items-center gap-1 rounded font-medium text-accent-600 hover:underline dark:text-accent-400"
    >
      {children} <ArrowRight size={13} aria-hidden />
    </button>
  );
}
