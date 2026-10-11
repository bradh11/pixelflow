import { ArrowRight } from "lucide-react";
import { useLayoutEditor } from "../state/layoutEditor";
import { useApp } from "../state/store";
import { setMoreOpen } from "./ui";

/** The Layout section that holds a prop's color order (see PropertiesPanel). */
export const COLOR_ORDER_SECTION = "layout-prop";

/**
 * A pointer to where a prop's color order is set, for screens where people look for it (Wiring,
 * Test). With `propId` the link also selects that prop and opens the section.
 */
export function ColorOrderPointer({ propId, className = "" }: { propId?: string; className?: string }) {
  const open = () => {
    if (propId) {
      useLayoutEditor.getState().select([propId]);
      setMoreOpen(COLOR_ORDER_SECTION, true);
    }
    useApp.getState().setScreen("layout");
  };
  return (
    <p className={`text-xs text-neutral-500 dark:text-neutral-400 ${className}`}>
      Colors wrong, like red showing as green? A prop's color order is set on Layout: {propId ? "this prop's" : "select the prop, then"} More: color order and scale.{" "}
      <button type="button" onClick={open} className="inline-flex items-center gap-1 rounded font-medium text-accent-600 hover:underline dark:text-accent-400">
        {propId ? "Open it on Layout" : "Go to Layout"} <ArrowRight size={12} aria-hidden />
      </button>
    </p>
  );
}
