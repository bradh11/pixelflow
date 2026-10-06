import { describe, expect, it } from "vitest";
import { demoShow } from "../api/demo";
import { MemoryBackend } from "../api/memory";
import type { Edit, Group, GroupMember, Show } from "../api/types";
import {
  addMembersEdits,
  groupPropIds,
  memberKey,
  memberLabel,
  moveMemberEdits,
  newGroupEdits,
  removeGroupEdits,
  removeMemberEdits,
  renameGroupEdits,
} from "./groupEdits";

function withGroup(members: (show: Show) => GroupMember[]): { show: Show; group: Group } {
  const show = demoShow();
  const group: Group = { id: "g1", name: "Arches", members: members(show) };
  show.groups = [group];
  return { show, group };
}

const updated = (edits: Edit[]) => {
  expect(edits).toHaveLength(1);
  const e = edits[0];
  if (e.type !== "updateGroup") throw new Error(e.type);
  return e.group;
};

describe("group edits", () => {
  it("makes a group from the selection, in the order picked, with the next free name", () => {
    const show = demoShow();
    show.groups = [{ id: "x", name: "Group 1", members: [] }];
    const [a, b] = [show.props[2].id, show.props[0].id];
    const { edits, id } = newGroupEdits(show, [a, b, a]);
    expect(edits).toEqual([{ type: "addGroup", group: { id, name: "Group 2", members: [a, b] } }]);
  });

  it("renames, refusing an empty name", () => {
    const { show } = withGroup(() => []);
    expect(updated(renameGroupEdits("g1", "  Front arches ")(show)).name).toBe("Front arches");
    expect(renameGroupEdits("g1", "  ")(show)).toEqual([]);
    expect(renameGroupEdits("g1", "Arches")(show)).toEqual([]);
    expect(renameGroupEdits("gone", "X")(show)).toEqual([]);
  });

  it("adds props and submodels once each, at the end", () => {
    const { show } = withGroup((s) => [s.props[0].id]);
    const arch = show.props[0];
    const half: GroupMember = { prop: arch.id, region: arch.regions[0].id };
    const group = updated(addMembersEdits("g1", [arch.id, show.props[1].id, half, half])(show));
    expect(group.members).toEqual([arch.id, show.props[1].id, half]);
    expect(addMembersEdits("g1", [arch.id])(show)).toEqual([]);
  });

  it("removes a member, and moves one to a new place", () => {
    const { show } = withGroup((s) => s.props.map((p) => p.id));
    const ids = show.props.map((p) => p.id);
    expect(updated(removeMemberEdits("g1", ids[1])(show)).members).toEqual([ids[0], ids[2], ids[3]]);
    expect(updated(moveMemberEdits("g1", ids[3], 0)(show)).members).toEqual([ids[3], ids[0], ids[1], ids[2]]);
    expect(updated(moveMemberEdits("g1", ids[0], 9)(show)).members).toEqual([ids[1], ids[2], ids[3], ids[0]]);
    expect(moveMemberEdits("g1", ids[0], 0)(show)).toEqual([]);
  });

  it("deletes a group", () => {
    expect(removeGroupEdits("g1")).toEqual([{ type: "removeGroup", id: "g1" }]);
  });

  it("are each one undo step the engine accepts", async () => {
    const backend = new MemoryBackend(demoShow());
    const show = backend.show;
    const { edits, id } = newGroupEdits(show, [show.props[0].id]);
    await backend.applyEdits(edits);
    await backend.applyEdits(addMembersEdits(id, [show.props[1].id])(backend.show));
    await backend.applyEdits(moveMemberEdits(id, show.props[1].id, 0)(backend.show));
    expect(backend.show.groups[0].members).toEqual([show.props[1].id, show.props[0].id]);
    await backend.undo();
    expect(backend.show.groups[0].members).toEqual([show.props[0].id, show.props[1].id]);
  });

  it("names members and finds the props to select", () => {
    const { show } = withGroup((s) => [s.props[1].id, { prop: s.props[0].id, region: s.props[0].regions[1].id }, "missing"]);
    const members = show.groups[0].members;
    expect(members.map((m) => memberLabel(show, m))).toEqual(["Mega Tree", "Garage Arch / Right half", "Missing prop"]);
    expect(groupPropIds(show, show.groups[0])).toEqual([show.props[1].id, show.props[0].id]);
    expect(memberKey(members[1])).toBe(`${show.props[0].id}/${show.props[0].regions[1].id}`);
  });
});
