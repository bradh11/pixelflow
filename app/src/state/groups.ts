import type { Group, Show } from "../api/types";
import { newGroupEdits } from "../lib/groupEdits";
import { plural } from "../lib/format";
import { useLayoutEditor } from "./layoutEditor";
import { useApp } from "./store";
import { toast } from "./toast";

/** The group whose members are exactly these props (in any order), if there is one. */
export function sameGroup(show: Show, ids: string[]): Group | undefined {
  if (ids.length === 0) return undefined;
  const want = new Set(ids);
  return show.groups.find((g) => g.members.length === want.size && g.members.every((m) => typeof m === "string" && want.has(m)));
}

/**
 * Makes a group of the selected props, in the order they were picked (⌘G, or Group selected), and
 * opens it in the Groups list. One undo step. Resolves with the new group's id, or null.
 */
export async function groupSelected(): Promise<string | null> {
  const ids = useLayoutEditor.getState().selected;
  if (ids.length === 0) return null;
  const show = useApp.getState().snapshot?.show;
  const existing = show && sameGroup(show, ids);
  if (existing) {
    useLayoutEditor.getState().setSidePanel({ open: true, tab: "groups", group: existing.id });
    toast(`These props are already ${existing.name}`, undefined, "info");
    return existing.id;
  }
  let made: { id: string; name: string; count: number } | null = null;
  const ok = await useApp.getState().apply((show) => {
    const members = ids.filter((id) => show.props.some((p) => p.id === id));
    const { edits, id } = newGroupEdits(show, members);
    const first = edits[0];
    made = { id, name: first.type === "addGroup" ? first.group.name : "", count: members.length };
    return edits;
  });
  const group = made as { id: string; name: string; count: number } | null;
  if (!ok || !group) return null;
  useLayoutEditor.getState().setSidePanel({ open: true, tab: "groups", group: group.id });
  toast(`Made ${group.name} from ${plural(group.count, "prop")}`);
  return group.id;
}
