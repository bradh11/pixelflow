// What a change is called, for the Undo and Redo tooltips: "Move Mega Tree", "Delete 3 effects".
// Worked out from the edits sent and the document as it was before them.

import type { Sequence, SequenceEdit } from "../api/sequence";
import type { Edit, Prop, Show } from "../api/types";

const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

/** "Arch 1", or "3 props" (`noun` in the plural). */
const nameOrCount = (names: string[], noun: string) => (names.length === 1 ? names[0] : `${names.length} ${noun}`);

type PropVerb = "Move" | "Turn" | "Resize" | "Rename" | "Change";

function propVerb(before: Prop, after: Prop): PropVerb {
  const others = (p: Prop) => ({ ...p, name: "", transform: null });
  if (!same(others(before), others(after))) return "Change";
  const t = before.transform;
  const u = after.transform;
  const moved = !same(t.position, u.position);
  const turned = !same(t.rotationDeg, u.rotationDeg);
  const resized = !same(t.scale, u.scale);
  const renamed = before.name !== after.name;
  if (renamed && !moved && !turned && !resized) return "Rename";
  if (renamed) return "Change";
  if (moved && !turned && !resized) return "Move";
  if (turned && !moved && !resized) return "Turn";
  if (resized && !moved && !turned) return "Resize";
  return "Change";
}

function describeProps(edits: Edit[], show: Show): string | null {
  const byId = new Map(show.props.map((p) => [p.id, p]));
  const added = edits.flatMap((e) => (e.type === "addProp" ? [e.prop.name] : []));
  const removed = edits.flatMap((e) => (e.type === "removeProp" ? [byId.get(e.id)?.name ?? "a prop"] : []));
  const updated = edits.flatMap((e) => (e.type === "updateProp" ? [[byId.get(e.prop.id), e.prop] as const] : []));
  const kinds = [added, removed, updated].filter((l) => l.length > 0).length;
  if (kinds === 0) return null;
  if (kinds > 1) return "Change props";
  if (added.length) return `Add ${nameOrCount(added, "props")}`;
  if (removed.length) return `Delete ${nameOrCount(removed, "props")}`;
  const verbs = new Set(updated.map(([before, after]) => (before ? propVerb(before, after) : "Change")));
  const verb = verbs.size === 1 ? [...verbs][0] : "Change";
  if (updated.length === 1) {
    const [before, after] = updated[0];
    return verb === "Rename" ? `Rename ${before?.name ?? "a prop"} to ${after.name}` : `${verb} ${after.name}`;
  }
  return `${verb} ${updated.length} props`;
}

function describeOthers(edit: Edit, show: Show): string {
  switch (edit.type) {
    case "renameShow":
      return `Rename the show to ${edit.name}`;
    case "setFrameRate":
      return "Change the frame rate";
    case "addGroup":
      return `Make group ${edit.group.name}`;
    case "updateGroup": {
      const before = show.groups.find((g) => g.id === edit.group.id);
      return before && before.name !== edit.group.name ? `Rename group ${before.name} to ${edit.group.name}` : `Change group ${edit.group.name}`;
    }
    case "removeGroup":
      return `Delete group ${show.groups.find((g) => g.id === edit.id)?.name ?? ""}`.trim();
    case "addController":
      return `Add ${edit.controller.name}`;
    case "updateController": {
      const before = show.controllers.find((c) => c.id === edit.controller.id);
      if (before && same({ ...before, ports: null }, { ...edit.controller, ports: null })) return `Change wiring on ${edit.controller.name}`;
      return `Edit ${before?.name ?? edit.controller.name}`;
    }
    case "removeController":
      return `Delete ${show.controllers.find((c) => c.id === edit.id)?.name ?? "a controller"}`;
    case "addSequence":
      return `Add ${edit.sequence.name} to the playlist`;
    case "updateSequence":
      return `Change ${edit.sequence.name}`;
    case "removeSequence":
      return `Take ${show.sequences.find((s) => s.id === edit.id)?.name ?? "a sequence"} off the playlist`;
    case "moveSequence":
      return `Move ${show.sequences.find((s) => s.id === edit.id)?.name ?? "a sequence"} in the playlist`;
    case "setBackground":
      return edit.background === null ? "Remove the photo" : show.background ? "Change the photo" : "Add the photo";
    case "setHouseModel":
      return edit.houseModel === null ? "Remove the house model" : "Change the house model";
    default:
      return "Change the show";
  }
}

/** What the edits do to `show`, in a few words. Props come first: deleting one unwires it too. */
export function describeShowEdits(edits: Edit[], show: Show): string {
  const props = describeProps(edits, show);
  if (props) return props;
  const others = [...new Set(edits.map((e) => describeOthers(e, show)))];
  return others.length === 1 ? others[0] : "Change the show";
}

function effectsOf(doc: Sequence) {
  const effects = new Map<string, { kind: string; startMs: number; endMs: number }>();
  for (const row of doc.rows) for (const layer of row.layers) for (const e of layer.effects) effects.set(e.id, { kind: e.params.kind, startMs: e.startMs, endMs: e.endMs });
  return effects;
}

/** What the edits do to the sequence, in a few words; `label` names an effect kind ("Color Wash"). */
export function describeSequenceEdits(edits: SequenceEdit[], doc: Sequence, label: (kind: string) => string): string {
  const effects = effectsOf(doc);
  const nameOf = (id: string) => {
    const e = effects.get(id);
    return e ? label(e.kind) : "effect";
  };
  const verbs = edits.map((edit): [string, string] => {
    switch (edit.type) {
      case "addEffect":
        return ["Add", label(edit.effect.params.kind)];
      case "removeEffect":
        return ["Delete", nameOf(edit.id)];
      case "updateEffect":
      case "setEffectParams":
        return ["Change", nameOf(edit.type === "updateEffect" ? edit.effect.id : edit.id)];
      case "moveEffect":
      case "setEffectTiming": {
        const before = effects.get(edit.id);
        const resized = before && edit.endMs - edit.startMs !== before.endMs - before.startMs;
        const moved = before && edit.type === "moveEffect" && before.startMs !== edit.startMs;
        return [resized && !moved ? "Resize" : "Move", nameOf(edit.id)];
      }
      default:
        return ["", ""];
    }
  });
  if (verbs.every(([verb]) => verb)) {
    const kinds = new Set(verbs.map(([verb]) => verb));
    const verb = kinds.size === 1 ? verbs[0][0] : "Change";
    return verbs.length === 1 ? `${verb} ${verbs[0][1]}` : `${verb} ${verbs.length} effects`;
  }
  const track = (id: string) => doc.timingTracks.find((t) => t.id === id)?.name ?? "a timing track";
  const one = edits[0];
  switch (one.type) {
    case "addRow":
      return edits.length > 1 ? `Add ${edits.length} rows` : "Add a row";
    case "removeRow":
      return edits.length > 1 ? `Delete ${edits.length} rows` : "Delete a row";
    case "moveRow":
      return "Move a row";
    case "addLayer":
      return "Add a layer";
    case "removeLayer":
      return "Delete a layer";
    case "addTimingTrack":
      return `Add timing track ${one.track.name}`;
    case "removeTimingTrack":
      return `Delete ${track(one.id)}`;
    case "renameTimingTrack":
      return `Rename ${track(one.id)} to ${one.name}`;
    case "moveTimingTrack":
      return `Move ${track(one.id)}`;
    case "updateTimingTrack":
      return `Change ${one.track.name}`;
    case "spreadLyrics":
      return `Add lyrics to ${track(one.track)}`;
    case "generateMarks":
      return `Make marks on ${track(one.track)}`;
    case "addMarks":
    case "setMark":
    case "removeMarks":
    case "splitMark":
    case "mergeMarks":
    case "labelMarks":
    case "breakIntoWords":
      return `Change marks on ${track(one.track)}`;
    case "copyMarks":
      return `Copy marks to ${track(one.to)}`;
    case "updateInfo":
      return "Change the sequence's details";
    default:
      return "Change the sequence";
  }
}
