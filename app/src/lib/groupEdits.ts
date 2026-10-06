// Group edits for the Layout screen's Groups editor. Each change is one batch (one undo step),
// built from the show as it is when its turn comes (see `EditsFrom`), using the engine's
// addGroup, updateGroup, and removeGroup edits.

import type { Edit, Group, GroupMember, Show } from "../api/types";
import { memberProp, uniqueName } from "./shows";

/** One key per member: a prop's id, or "prop/submodel". */
export function memberKey(m: GroupMember): string {
  return typeof m === "string" ? m : `${m.prop}/${m.region}`;
}

/** "Mega Tree", or "Garage Arch / Left half" for a submodel. */
export function memberLabel(show: Show, m: GroupMember): string {
  const prop = show.props.find((p) => p.id === memberProp(m));
  if (!prop) return "Missing prop";
  if (typeof m === "string") return prop.name;
  return `${prop.name} / ${prop.regions.find((r) => r.id === m.region)?.name ?? "missing submodel"}`;
}

/** The props a group lights (a submodel's prop), each once, in member order: what selecting it selects. */
export function groupPropIds(show: Show, group: Group): string[] {
  const ids = new Set(group.members.map(memberProp));
  return [...ids].filter((id) => show.props.some((p) => p.id === id));
}

function unique(members: GroupMember[]): GroupMember[] {
  const seen = new Set<string>();
  return members.filter((m) => {
    const key = memberKey(m);
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

/** A new group of `members` (in that order), named "Group N". */
export function newGroupEdits(show: Show, members: GroupMember[]): { edits: Edit[]; id: string } {
  const id = crypto.randomUUID();
  const name = uniqueName("Group", show.groups.map((g) => g.name));
  return { edits: [{ type: "addGroup", group: { id, name, members: unique(members) } }], id };
}

/** The group `id` changed by `change`; nothing when it's gone or nothing changed. */
function updateGroupEdits(id: string, change: (group: Group) => Group | null): (show: Show) => Edit[] {
  return (show) => {
    const group = show.groups.find((g) => g.id === id);
    const next = group && change(group);
    if (!group || !next || JSON.stringify(next) === JSON.stringify(group)) return [];
    return [{ type: "updateGroup", group: next }];
  };
}

export function renameGroupEdits(id: string, name: string): (show: Show) => Edit[] {
  const trimmed = name.trim();
  return updateGroupEdits(id, (g) => (trimmed ? { ...g, name: trimmed } : null));
}

/** Adds the members not already in the group, at the end. */
export function addMembersEdits(id: string, members: GroupMember[]): (show: Show) => Edit[] {
  return updateGroupEdits(id, (g) => ({ ...g, members: unique([...g.members, ...members]) }));
}

export function removeMemberEdits(id: string, member: GroupMember): (show: Show) => Edit[] {
  const key = memberKey(member);
  return updateGroupEdits(id, (g) => ({ ...g, members: g.members.filter((m) => memberKey(m) !== key) }));
}

/** Moves a member to `index` (clamped): effects that run along the group follow this order. */
export function moveMemberEdits(id: string, member: GroupMember, index: number): (show: Show) => Edit[] {
  const key = memberKey(member);
  return updateGroupEdits(id, (g) => {
    const from = g.members.findIndex((m) => memberKey(m) === key);
    if (from < 0) return null;
    const members = [...g.members];
    const [moved] = members.splice(from, 1);
    members.splice(Math.max(0, Math.min(index, members.length)), 0, moved);
    return { ...g, members };
  });
}

export function removeGroupEdits(id: string): Edit[] {
  return [{ type: "removeGroup", id }];
}
