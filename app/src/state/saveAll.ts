import { useSequencer } from "./sequencer";
import { useApp } from "./store";
import { toast } from "./toast";

/**
 * ⌘S on the Sequence screen: saves the open sequence, and the show too when it has unsaved
 * changes, then says in one toast what was saved ("Saved Christmas Medley and Demo House").
 * Each keeps its own undo history. True when everything asked for was saved.
 */
export async function saveSequenceAndShow(): Promise<boolean> {
  const sequencer = useSequencer.getState();
  const saved: string[] = [];
  let ok = true;
  if (sequencer.doc) {
    const name = sequencer.doc.name;
    if (await sequencer.save({ quiet: true })) saved.push(useSequencer.getState().doc?.name ?? name);
    else ok = false;
  }
  const app = useApp.getState();
  if (app.snapshot?.dirty) {
    if (await app.save({ quiet: true })) saved.push(useApp.getState().snapshot?.show.name ?? "the show");
    else ok = false;
  }
  if (saved.length > 0) toast(`Saved ${saved.join(" and ")}`);
  return ok;
}
