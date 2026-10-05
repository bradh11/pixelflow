// Sending finished gestures (drags and arrow-key moves) to the engine. Each is one batch (one
// undo step) built from the show as it is when its turn comes, and stays drawn on the canvas
// until the engine's pixel positions include it, so props never jump back while it's on its way.

import { gestureEdits } from "../lib/layoutEdits";
import { type Gesture, isNoop, tidy } from "../lib/layoutMath";
import { type PendingGesture, useLayoutEditor } from "./layoutEditor";
import { useApp } from "./store";

let nextKey = 1;

function send(entry: PendingGesture): Promise<boolean> {
  return useApp
    .getState()
    .edit((show) => gestureEdits(show, entry.ids, entry.gesture))
    .then((revision) => {
      useLayoutEditor.setState((s) => ({
        pending:
          revision === null
            ? s.pending.filter((p) => p.key !== entry.key)
            : s.pending.map((p) => (p.key === entry.key ? { ...p, revision } : p)),
      }));
      return revision !== null;
    });
}

/** Sends a finished move, turn, or resize of the props `ids`. False when refused or a no-op. */
export function commitGesture(ids: string[], gesture: Gesture): Promise<boolean> {
  if (ids.length === 0 || isNoop(gesture)) return Promise.resolve(false);
  const entry: PendingGesture = { key: nextKey++, ids, gesture, revision: null };
  useLayoutEditor.setState((s) => ({ pending: [...s.pending, entry] }));
  return send(entry);
}

const sameIds = (a: string[], b: string[]) => a.length === b.length && a.every((id, i) => id === b[i]);

/** Adds an arrow-key step to the move being built up (sending the previous one if the selection changed). */
export function addNudge(ids: string[], dx: number, dy: number) {
  const current = useLayoutEditor.getState().nudge;
  if (current && !sameIds(current.ids, ids)) void flushNudge();
  const base = useLayoutEditor.getState().nudge;
  useLayoutEditor.setState({ nudge: { ids, dx: tidy((base?.dx ?? 0) + dx), dy: tidy((base?.dy ?? 0) + dy) } });
}

/** Sends the arrow-key move built up so far as one move (one undo step). */
export function flushNudge(): Promise<boolean> {
  const nudge = useLayoutEditor.getState().nudge;
  if (!nudge) return Promise.resolve(false);
  const gesture: Gesture = { kind: "move", dx: nudge.dx, dy: nudge.dy };
  if (isNoop(gesture)) {
    useLayoutEditor.setState({ nudge: null });
    return Promise.resolve(false);
  }
  const entry: PendingGesture = { key: nextKey++, ids: nudge.ids, gesture, revision: null };
  // In one step, so the canvas never draws the props without the move.
  useLayoutEditor.setState((s) => ({ nudge: null, pending: [...s.pending, entry] }));
  return send(entry);
}

/** The gestures still to draw over pixel positions from show revision `revision`. */
export function unsettled(pending: PendingGesture[], revision: number): PendingGesture[] {
  return pending.filter((p) => p.revision === null || p.revision > revision);
}

/** Forgets gestures the engine's positions (from show revision `revision`) already include. */
export function settlePending(revision: number) {
  const { pending } = useLayoutEditor.getState();
  const left = unsettled(pending, revision);
  if (left.length !== pending.length) useLayoutEditor.setState({ pending: left });
}
