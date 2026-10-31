import type { PreviewProp, Prop, Show } from "../api/types";
import { PROPERTIES_PANEL_WIDTH, usePropertiesPanel } from "../components/layout/PropertiesDock";
import { frontView } from "../lib/geometry";
import { placedInView, visibleBox } from "../lib/layoutEdits";
import { type Box, boxOfPoints, fitView, inBox, unionBox } from "../lib/layoutMath";
import { type PropKind, newProp } from "../lib/shows";
import { canvasSize, useLayoutEditor } from "./layoutEditor";
import { useApp } from "./store";
import { toast } from "./toast";

/** Resolves after the next paint (a moment later where there's no painting, as in tests). */
const nextFrame = () =>
  new Promise<void>((resolve) => (typeof requestAnimationFrame === "function" ? requestAnimationFrame(() => resolve()) : setTimeout(resolve, 16)));

/** Between the canvas and the properties panel docked beside it, in CSS pixels. */
const DOCK_GAP = 12;

/**
 * `visible` less what the properties panel will hide once the new prop is selected (`zoom` pixels
 * a unit). It opens with a selection: over the canvas's right edge in a narrow window, or beside
 * the canvas, which then shows less on each side of the middle. Already open, it hides nothing more.
 */
function seenOnceSelected(visible: Box, zoom: number): Box {
  const { pref } = usePropertiesPanel.getState();
  if (pref === "hidden") return visible;
  const floating = document.querySelector<HTMLElement>("[data-layout-row]")?.dataset.properties === "floating";
  if (floating) return { ...visible, maxX: visible.maxX - PROPERTIES_PANEL_WIDTH / zoom };
  if (pref === "pinned" || useLayoutEditor.getState().selected.length > 0) return visible;
  const side = (PROPERTIES_PANEL_WIDTH + DOCK_GAP) / 2 / zoom;
  return { ...visible, minX: visible.minX + side, maxX: visible.maxX - side };
}

/**
 * Adds a prop of `kind` near the middle of what the layout canvas shows, clear of the props there
 * (see `placedInView`), selects it, brings it into view, and says so. One undo step. `preview` is
 * the props' pixels from the engine, where the screen has them. Resolves with the new prop, or null.
 */
export async function addPropInView(kind: PropKind | ((show: Show) => Prop), preview: PreviewProp[] = []): Promise<Prop | null> {
  // From another screen (the command palette): open Layout first, so its canvas can be measured.
  if (useApp.getState().screen !== "layout") {
    useApp.getState().setScreen("layout");
    await nextFrame();
    await nextFrame();
  }
  const size = canvasSize();
  const editor = useLayoutEditor.getState();
  const content = () => unionBox(preview.map((p) => boxOfPoints(p.points)));
  const view = size ? (editor.view ?? fitView(content(), size)) : null;
  const visible = view && size ? visibleBox(view, size) : null;
  const seen = view && visible ? seenOnceSelected(visible, view.zoom) : null;
  let added: Prop | null = null;
  const ok = await useApp.getState().apply((show) => {
    added = placedInView(typeof kind === "function" ? kind(show) : newProp(kind, show), show, preview, seen);
    return [{ type: "addProp", prop: added }];
  });
  const prop = added as Prop | null;
  if (!ok || !prop) return null;
  const st = useLayoutEditor.getState();
  st.select([prop.id]);
  if (st.tool !== "select") st.setTool("select");
  // Pan (or zoom out) so all of it shows, when it doesn't already.
  const box = boxOfPoints(frontView(prop));
  if (box && size && view && visible) {
    const inside = inBox(visible, { x: box.minX, y: box.minY }) && inBox(visible, { x: box.maxX, y: box.maxY });
    if (!inside) st.setView(fitView(unionBox([box, visible]), size));
  }
  toast(`Added ${prop.name} — drag it into place`);
  return prop;
}
