// What the Layout screen does to props, shared by its keys (useLayoutKeys) and its right-click
// menus, so both go through the same edits and undo the same way.

import type { Edit, Show } from "../api/types";
import { deleteProps } from "../components/layout/PropsList";
import { duplicateEdits, pasteEdits, removeEdits } from "../lib/layoutEdits";
import { groupSelected } from "./groups";
import { useLayoutEditor } from "./layoutEditor";
import { useApp } from "./store";
import type { MenuItem } from "./contextMenu";
import { useWiring } from "./wiring";

/** How far (layout units, right and down) each paste lands from the last. */
const PASTE_OFFSET = 0.5;

/** The ids that are props in the open show. */
function propIds(ids: string[]): string[] {
  const show = useApp.getState().snapshot?.show;
  return show ? ids.filter((id) => show.props.some((p) => p.id === id)) : [];
}

/** Adds props built from the latest show, then selects them (one undo step). */
function addAndSelect(build: (latest: Show) => { edits: Edit[]; ids: string[] }) {
  let made: string[] = [];
  const edits = (latest: Show) => {
    const built = build(latest);
    made = built.ids;
    return built.edits;
  };
  return useApp
    .getState()
    .apply(edits)
    .then((ok) => {
      if (ok && made.length > 0) useLayoutEditor.getState().select(made);
      return ok;
    });
}

/** Copies the props (or cuts them: they go, and paste back where they were). */
export function copyProps(ids: string[], cut = false): void {
  const show = useApp.getState().snapshot?.show;
  const chosen = propIds(ids);
  if (!show || chosen.length === 0) return;
  const props = structuredClone(show.props.filter((p) => chosen.includes(p.id)));
  useLayoutEditor.setState({ clipboard: { props, nextOffset: cut ? 0 : PASTE_OFFSET } });
  if (cut) void useApp.getState().apply(removeEdits(chosen)).then((ok) => ok && useLayoutEditor.getState().clear());
}

/** Pastes what was copied, a little further along each time, and selects it. */
export function pasteProps(): void {
  const clipboard = useLayoutEditor.getState().clipboard;
  if (!clipboard) return;
  useLayoutEditor.setState({ clipboard: { ...clipboard, nextOffset: clipboard.nextOffset + PASTE_OFFSET } });
  void addAndSelect((latest) => pasteEdits(latest, clipboard.props, clipboard.nextOffset));
}

export function duplicateProps(ids: string[]): void {
  const chosen = propIds(ids);
  if (chosen.length > 0) void addAndSelect((latest) => duplicateEdits(latest, chosen));
}

/** Deletes the props (asking first if the open sequence uses them), with Undo in a toast. */
export function deletePropsById(ids: string[]): void {
  const show = useApp.getState().snapshot?.show;
  const chosen = propIds(ids);
  if (!show || chosen.length === 0) return;
  void deleteProps(
    chosen,
    chosen.map((id) => show.props.find((p) => p.id === id)?.name ?? ""),
  );
}

/** Groups the props (selecting them first, as ⌘G groups the selection). */
export function groupProps(ids: string[]): void {
  const chosen = propIds(ids);
  if (chosen.length === 0) return;
  useLayoutEditor.getState().select(chosen);
  void groupSelected();
}

/** Opens the Wiring screen, with the props list there narrowed to this prop. */
export function wireProp(id: string): void {
  const name = useApp.getState().snapshot?.show.props.find((p) => p.id === id)?.name;
  if (!name) return;
  useWiring.getState().setQuery(name);
  useApp.getState().setScreen("wiring");
}

/** Types over the prop's name in the props list (bringing the list out if it's put away). */
export function renameProp(id: string): void {
  const editor = useLayoutEditor.getState();
  editor.select([id]);
  editor.setSidePanel({ open: true, tab: "props", group: null });
  editor.setRenaming(id);
}

/** Moves the 2D view to show the props. */
export function bringPropsToView(ids: string[]): void {
  const chosen = propIds(ids);
  if (chosen.length > 0) useLayoutEditor.setState({ reveal: chosen });
}

/**
 * The right-click menu for props (on the canvas or in the props list). `ids` are the props it acts
 * on: the selection, once the prop clicked is in it. With none, it offers Paste and Select all.
 */
export function propMenuItems(ids: string[], { canReveal = true } = {}): MenuItem[] {
  const chosen = propIds(ids);
  const canPaste = useLayoutEditor.getState().clipboard !== null;
  if (chosen.length === 0) {
    const all = useApp.getState().snapshot?.show.props.map((p) => p.id) ?? [];
    return [
      { label: "Paste", shortcut: "layout-paste", run: pasteProps, disabled: !canPaste },
      { label: "Select all", shortcut: "layout-select-all", run: () => useLayoutEditor.getState().select(all), disabled: all.length === 0 },
    ];
  }
  const one = chosen.length === 1 ? chosen[0] : null;
  return [
    { label: "Cut", shortcut: "layout-cut", run: () => copyProps(chosen, true) },
    { label: "Copy", shortcut: "layout-copy", run: () => copyProps(chosen) },
    { label: "Paste", shortcut: "layout-paste", run: pasteProps, disabled: !canPaste },
    { label: "Duplicate", shortcut: "layout-duplicate", run: () => duplicateProps(chosen) },
    { label: "Delete", shortcut: "layout-delete", run: () => deletePropsById(chosen), danger: true },
    { label: "Group", shortcut: "layout-group", run: () => groupProps(chosen), separated: true },
    { label: "Wire…", run: () => one && wireProp(one), disabled: !one },
    { label: "Rename", run: () => one && renameProp(one), disabled: !one },
    ...(canReveal ? [{ label: "Bring to view", run: () => bringPropsToView(chosen) }] : []),
  ];
}

/** The props a right-click on `id` acts on: the selection if it's in it, else just that prop (now selected). */
export function pickForMenu(id: string): string[] {
  const editor = useLayoutEditor.getState();
  if (editor.selected.includes(id)) return editor.selected;
  editor.select([id]);
  return [id];
}
