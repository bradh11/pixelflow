import { describe, expect, it, vi } from "vitest";
import { demoShow } from "../api/demo";
import { DEMO_SEQUENCE_PATH, demoSequence } from "../api/demoSequence";
import { MemoryBackend } from "../api/memory";
import { MemorySequencer } from "../api/memorySequencer";
import type { PlaybackStatus } from "../api/types";
import { useSequencer } from "./sequencer";
import { useApp } from "./store";

async function connected() {
  const show = demoShow();
  const backend = new MemoryBackend(show);
  const seq = new MemorySequencer(backend);
  seq.files.set(DEMO_SEQUENCE_PATH, demoSequence(show, 60_000));
  await seq.openSequenceDoc(DEMO_SEQUENCE_PATH);
  await useApp.getState().connect(backend);
  await useSequencer.getState().connect(seq);
  return { backend, seq, store: useSequencer.getState() };
}

describe("sequencer playback", () => {
  it("lets go of the player when the song ends, so seeking only moves the playhead", async () => {
    const { backend, store } = await connected();
    await store.play();
    await store.seek(59_990);
    await new Promise((resolve) => setTimeout(resolve, 30));
    await useSequencer.getState().pollPlayback();
    expect(useSequencer.getState().status).toBeNull();
    expect(useSequencer.getState().playheadMs).toBe(60_000);
    expect(backend.calls).toContain("stopPlayback");
    // A click on the ruler afterwards doesn't start the music again.
    await useSequencer.getState().seek(1000);
    expect(await backend.playbackStatus()).toBeNull();
    expect(useSequencer.getState().status).toBeNull();
    expect(useSequencer.getState().playheadMs).toBe(1000);
    // Play starts from there.
    await useSequencer.getState().play();
    expect(backend.calls).toContain("playAuthored@1000");
  });

  it("ignores a playback answer that a newer seek, pause, or stop overtook", async () => {
    const { backend, store } = await connected();
    await store.play();
    const stale = (await backend.playbackStatus())!;
    let answer: (status: PlaybackStatus) => void = () => undefined;
    vi.spyOn(backend, "playbackStatus").mockImplementationOnce(() => new Promise((resolve) => (answer = resolve)));
    const poll = useSequencer.getState().pollPlayback();
    await useSequencer.getState().seek(20_000);
    answer({ ...stale, positionMs: 5000, state: "playing" });
    await poll;
    expect(useSequencer.getState().playheadMs).toBe(20_000);

    // A late answer to a seek doesn't bring back a stopped player either.
    let seeked: (status: PlaybackStatus | null) => void = () => undefined;
    vi.spyOn(backend, "seekPlayback").mockImplementationOnce(() => new Promise((resolve) => (seeked = resolve)));
    const seek = useSequencer.getState().seek(30_000);
    await useSequencer.getState().stop();
    seeked({ ...stale, positionMs: 30_000, state: "playing" });
    await seek;
    expect(useSequencer.getState().status).toBeNull();
  });
});
