import { describe, expect, it, vi } from "vitest";
import { MemoryBackend } from "../api/memory";
import { MemorySequencer } from "../api/memorySequencer";
import { useSequencer } from "./sequencer";
import { useApp } from "./store";

/** The app's own show and sequence, connected, each with one change to undo. */
async function connected() {
  const backend = new MemoryBackend();
  const sequencer = new MemorySequencer(backend);
  await useApp.getState().connect(backend);
  await sequencer.newSequenceDoc("Song", 10_000, null);
  await useSequencer.getState().connect(sequencer);
  await useApp.getState().apply([{ type: "renameShow", name: "Renamed" }]);
  await useSequencer.getState().edit([{ type: "updateInfo", name: "Renamed song", audio: null, durationMs: 10_000, frameMs: 25 }]);
  return { backend, sequencer };
}

// A show and sequence change applied together (an assistant proposal) is one undo step in the
// app: undoing it on either side also changes the other side, which then fetches itself again.
describe("undo of a change made to the show and sequence together", () => {
  it("refreshes the sequence after a show undo that also took back its sequence half", async () => {
    const { backend, sequencer } = await connected();
    const undo = backend.undo.bind(backend);
    backend.undo = async () => {
      // The engine took back the paired sequence step too.
      await sequencer.undoSequence();
      return { ...(await undo()), sequenceRevision: sequencer.revision };
    };
    await useApp.getState().undo();
    expect(useSequencer.getState().revision).toBe(sequencer.revision);
    expect(useSequencer.getState().doc!.name).toBe("Song");
  });

  it("leaves the sequence alone when the show undo didn't touch it", async () => {
    const { backend, sequencer } = await connected();
    const undo = backend.undo.bind(backend);
    backend.undo = async () => ({ ...(await undo()), sequenceRevision: sequencer.revision });
    const getDoc = vi.spyOn(sequencer, "getSequenceDoc");
    await useApp.getState().undo();
    expect(getDoc).not.toHaveBeenCalled();
  });

  it("refreshes the show after a sequence undo that also took back its show half", async () => {
    const { backend, sequencer } = await connected();
    const undo = sequencer.undoSequence.bind(sequencer);
    sequencer.undoSequence = async () => {
      // The engine took back the paired show step too.
      await backend.undo();
      return { ...(await undo()), showRevision: backend.revision };
    };
    await useSequencer.getState().undo();
    await vi.waitFor(() => expect(useApp.getState().snapshot!.show.name).toBe("Untitled Show"));
  });
});
