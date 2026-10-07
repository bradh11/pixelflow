import { useSequencer } from "./sequencer";
import { useApp } from "./store";
import { toast } from "./toast";

/** Runs one save and says how it went: saved, cancelled (no error), or failed with the reason. */
async function attempt(save: () => Promise<boolean>): Promise<{ saved: true } | { saved: false; error: string | null }> {
  // Each save reports its own failure in the shared error banner; start clean so a cancelled save
  // isn't blamed for an older message.
  useApp.setState({ error: null });
  if (await save()) return { saved: true };
  return { saved: false, error: useApp.getState().error };
}

/**
 * ⌘S on the Sequence screen: saves the show when it has unsaved changes, then the open sequence
 * (the sequence's rows refer to the show's props, so the show goes first). A toast names what
 * was saved; anything that failed is said in the error banner, by name and reason. Nothing that
 * failed or was cancelled is called saved. Each keeps its own undo history. True when everything
 * asked for was saved.
 */
export async function saveSequenceAndShow(): Promise<boolean> {
  const saved: string[] = [];
  const failed: string[] = [];
  let ok = true;

  const app = useApp.getState();
  if (app.snapshot?.dirty) {
    const name = app.snapshot.show.name;
    const result = await attempt(() => app.save({ quiet: true }));
    if (result.saved) saved.push(useApp.getState().snapshot?.show.name ?? name);
    else {
      ok = false;
      if (result.error) failed.push(`${name} wasn't saved: ${result.error}`);
    }
  }

  const sequencer = useSequencer.getState();
  if (sequencer.doc) {
    const name = sequencer.doc.name;
    const result = await attempt(() => sequencer.save({ quiet: true }));
    if (result.saved) saved.push(useSequencer.getState().doc?.name ?? name);
    else {
      ok = false;
      if (result.error) failed.push(`${name} wasn't saved: ${result.error}`);
    }
  }

  // The show's banner would otherwise be cleared by the sequence's save, or the other way round.
  useApp.setState({ error: failed.length > 0 ? failed.join(" ") : null });
  if (saved.length > 0) toast(`Saved ${saved.join(" and ")}`);
  return ok;
}
