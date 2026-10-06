import type { PreviewProp, Prop } from "../api/types";
import { frontView } from "../lib/geometry";
import { placedInView, visibleBox } from "../lib/layoutEdits";
import { boxOfPoints, fitView, inBox, unionBox } from "../lib/layoutMath";
import { type PropKind, newProp } from "../lib/shows";
import { canvasSize, useLayoutEditor } from "./layoutEditor";
import { useApp } from "./store";
import { toast } from "./toast";

/** Resolves after the next paint (a moment later where there's no painting, as in tests). */
const nextFrame = () =>
  new Promise<void>((resolve) => (typeof requestAnimationFrame === "function" ? requestAnimationFrame(() => resolve()) : setTimeout(resolve, 16)));

/**
 * Adds a prop of `kind` in the middle of what the layout canvas shows (stepped aside from the last
 * one), selects it, brings it into view, and says so. One undo step. `preview` is the props'
 * pixels from the engine, where the screen has them. Resolves with the new prop, or null.
 */
export async function addPropInView(kind: PropKind, preview: PreviewProp[] = []): Promise<Prop | null> {
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
  let added: Prop | null = null;
  const ok = await useApp.getState().apply((show) => {
    added = placedInView(newProp(kind, show), show, preview, visible);
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
