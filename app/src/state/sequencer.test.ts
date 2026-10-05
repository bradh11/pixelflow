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

describe("tap to time", () => {
  it("drops each mark where the music is, between playback polls too", async () => {
    const { seq } = await connected();
    await useSequencer.getState().edit([{ type: "addTimingTrack", track: { id: "taps", name: "Taps", kind: "custom", marks: [] } }]);
    useSequencer.getState().setActiveTrack("taps");
    const now = vi.spyOn(performance, "now").mockReturnValue(1000);
    await useSequencer.getState().play();
    expect(useSequencer.getState().status?.state).toBe("playing");
    const at = useSequencer.getState().playheadMs;
    // 300 ms after the player last said where it was.
    now.mockReturnValue(1300);
    useSequencer.getState().tap();
    now.mockReturnValue(1800);
    useSequencer.getState().tap();
    await vi.waitFor(() => expect(seq.doc!.timingTracks[2].marks).toHaveLength(2));
    expect(seq.doc!.timingTracks[2].marks).toEqual([
      { startMs: at + 300, endMs: at + 800, label: "" },
      { startMs: at + 800, endMs: at + 1300, label: "" },
    ]);
    await useSequencer.getState().stop();
    now.mockRestore();
  });

  it("starts a fresh run after a seek, a stop, or another track, instead of stretching the last mark", async () => {
    const { seq } = await connected();
    const tracks = () => seq.doc!.timingTracks;
    await useSequencer.getState().edit([
      { type: "addTimingTrack", track: { id: "taps", name: "Taps", kind: "custom", marks: [] } },
      { type: "addTimingTrack", track: { id: "other", name: "Other", kind: "custom", marks: [] } },
    ]);
    useSequencer.getState().setActiveTrack("taps");
    const tapAt = async (ms: number) => {
      useSequencer.getState().setPlayhead(ms);
      useSequencer.getState().tap();
      // Wait for the tap to land: an empty edit queued after it.
      await useSequencer.getState().edit(() => []);
    };
    await tapAt(10_000);
    // Scrubbed ahead to the chorus: the verse's last mark keeps its length.
    await useSequencer.getState().seek(60_000 - 1000);
    await tapAt(59_000);
    await vi.waitFor(() => expect(tracks()[2].marks).toHaveLength(2));
    expect(tracks()[2].marks[0]).toEqual({ startMs: 10_000, endMs: 10_500, label: "" });
    // Another track and back: no stretch either.
    useSequencer.getState().setActiveTrack("other");
    useSequencer.getState().setActiveTrack("taps");
    await tapAt(59_800);
    await vi.waitFor(() => expect(tracks()[2].marks).toHaveLength(3));
    expect(tracks()[2].marks[1]).toEqual({ startMs: 59_000, endMs: 59_500, label: "" });
    // Taps in one run still end the mark before.
    await tapAt(59_900);
    await vi.waitFor(() => expect(tracks()[2].marks[2]).toEqual({ startMs: 59_800, endMs: 59_900, label: "" }));
  });
});
