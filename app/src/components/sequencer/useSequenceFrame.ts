import { useEffect, useRef, useState } from "react";
import { everyFrame, frameTarget } from "../../lib/avSync";
import { pipeline, playClock, usePreviewSync } from "../../state/previewSync";
import { useSequencer } from "../../state/sequencer";

/**
 * The open sequence's frame for a preview: the moment at the playhead while it's still (drawn by
 * the engine; scrubbing asks for one frame at a time and skips the moments it passed meanwhile),
 * and while it plays, the moment that will be heard when the frame reaches the screen (see
 * lib/avSync). Controllers get the engine's own frames, untouched. `durationMs`: the sequence's
 * length; `stamp`: anything that changes when the picture may have without the playhead moving.
 */
export function useSequenceFrame(durationMs: number, stamp: unknown): Uint8Array | null {
  const api = useSequencer((s) => s.api);
  const playheadMs = useSequencer((s) => s.playheadMs);
  const revision = useSequencer((s) => s.revision);
  const picturesArrived = useSequencer((s) => s.picturesArrived);
  const running = useSequencer((s) => s.status?.state === "playing");
  const [frame, setFrame] = useState<Uint8Array | null>(null);
  /** The moment to draw next, while a frame is on its way. */
  const still = useRef<{ wanted: number | null; busy: boolean; live: boolean }>({ wanted: null, busy: false, live: true });

  // Still: render the moment at the playhead (the latest moment wins).
  useEffect(() => {
    if (!api || running) return;
    const s = still.current;
    s.wanted = playheadMs;
    const next = () => {
      if (s.busy || s.wanted === null || !s.live) return;
      const ms = s.wanted;
      s.wanted = null;
      s.busy = true;
      api
        .sequenceDocFrame(ms)
        .then(
          (f) => s.live && useSequencer.getState().status?.state !== "playing" && setFrame(f.length ? f : null),
          () => s.live && setFrame(null),
        )
        .finally(() => {
          s.busy = false;
          next();
        });
    };
    next();
  }, [api, playheadMs, revision, running, stamp, picturesArrived]);
  useEffect(() => {
    const s = still.current;
    s.live = true;
    return () => {
      s.live = false;
    };
  }, []);

  // Playing: one frame asked for at a time, for the moment it will be seen.
  useEffect(() => {
    if (!api || !running) return;
    let pending = false;
    let live = true;
    const stop = everyFrame(() => {
      const reading = playClock.reading();
      if (pending || !reading) return;
      const sentAt = performance.now();
      const { offsetMs } = usePreviewSync.getState();
      const ms = frameTarget(reading, sentAt, pipeline.fetchMs ?? 0, pipeline.refreshMs, offsetMs, durationMs);
      pending = true;
      api.sequenceDocFrame(Math.round(ms)).then(
        (f) => {
          pending = false;
          pipeline.noteFetch(performance.now() - sentAt);
          if (live && useSequencer.getState().status?.state === "playing" && f.length) setFrame(f);
        },
        () => {
          pending = false;
        },
      );
    });
    return () => {
      live = false;
      stop();
    };
  }, [api, running, durationMs]);

  return frame;
}

/** How often playback is checked while a sequence plays. */
const POLL_MS = 50;

/**
 * Keeps the playhead in step with the engine while the open sequence plays: for the screens that
 * show its transport (Sequence and Play).
 */
export function useSequencePolling() {
  const active = useSequencer((s) => s.status !== null);
  const pollPlayback = useSequencer((s) => s.pollPlayback);
  useEffect(() => {
    if (!active) return;
    const timer = setInterval(() => void pollPlayback(), POLL_MS);
    return () => clearInterval(timer);
  }, [active, pollPlayback]);
}
