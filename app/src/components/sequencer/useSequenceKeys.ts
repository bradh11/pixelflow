import { useEffect } from "react";
import type { SequenceEdit } from "../../api/sequence";
import { buildIndex, pasteEffects, stepTime } from "../../lib/timelineMath";
import { useSequencer } from "../../state/sequencer";
import { useApp } from "../../state/store";

/** Beat times from the sequence's beats track (or its first track), for Shift-steps. */
function beatTimes(): number[] {
  const doc = useSequencer.getState().doc;
  const track = doc?.timingTracks.find((t) => t.kind === "beats") ?? doc?.timingTracks[0];
  return track?.marks.map((m) => m.startMs) ?? [];
}

/**
 * The Sequence screen's keys (not while typing in a field): Space plays and pauses; arrows move
 * the selected effects, or the playhead, by a frame (with Shift, to the next beat); Home and End
 * jump; Delete removes; ⌘C, ⌘V, and ⌘D copy, paste at the playhead, and duplicate; ⌘A selects
 * everything; Escape clears the selection. Undo, redo, and save are global shortcuts.
 */
export function useSequenceKeys() {
  useEffect(() => {
    let nudges = 0;
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      if (target && (["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName) || target.isContentEditable)) return;
      if (target?.closest?.("[role=dialog]")) return;
      if (useApp.getState().paletteOpen) return;
      const s = useSequencer.getState();
      const doc = s.doc;
      if (!doc) return;
      const mod = e.metaKey || e.ctrlKey;
      const key = e.key;
      if (key === " " && !mod) {
        // A focused button would also take Space as a click.
        e.preventDefault();
        void (s.status?.state === "playing" ? s.pause() : s.play());
        return;
      }
      if ((key === "ArrowLeft" || key === "ArrowRight") && !mod) {
        e.preventDefault();
        const direction = key === "ArrowRight" ? 1 : -1;
        const grid = { frameMs: doc.frameMs, beats: beatTimes() };
        if (s.selection.length === 0) {
          void s.seek(stepTime(s.playheadMs, direction, grid, e.shiftKey));
          return;
        }
        const index = buildIndex(doc);
        const placed = s.selection.map((id) => index.byId.get(id)).filter((p) => p !== undefined);
        if (placed.length === 0) return;
        const first = Math.min(...placed.map((p) => p.effect.startMs));
        const last = Math.max(...placed.map((p) => p.effect.endMs));
        let delta = stepTime(first, direction, grid, e.shiftKey) - first;
        delta = Math.max(-first, Math.min(doc.durationMs - last, delta));
        if (delta === 0) return;
        // Holding the key down is one step to undo.
        if (!e.repeat) nudges++;
        const edits: SequenceEdit[] = placed.map((p) => ({ type: "setEffectTiming", id: p.effect.id, startMs: p.effect.startMs + delta, endMs: p.effect.endMs + delta }));
        void s.edit(edits, `nudge:${nudges}`);
        return;
      }
      if (key === "Home" || key === "End") {
        e.preventDefault();
        void s.seek(key === "Home" ? 0 : doc.durationMs);
        return;
      }
      if ((key === "Delete" || key === "Backspace") && s.selection.length > 0) {
        e.preventDefault();
        void s.edit(s.selection.map((id) => ({ type: "removeEffect" as const, id })));
        return;
      }
      if (key === "Escape" && s.selection.length > 0) {
        s.select([]);
        return;
      }
      if (!mod) return;
      const lower = key.toLowerCase();
      if (lower === "a") {
        e.preventDefault();
        s.select(doc.rows.flatMap((r) => r.layers.flatMap((l) => l.effects.map((x) => x.id))));
      } else if (lower === "c" && s.selection.length > 0) {
        e.preventDefault();
        s.copy();
      } else if (lower === "v" && s.clipboard.length > 0) {
        e.preventDefault();
        const edits = pasteEffects(doc, buildIndex(doc), s.clipboard, s.playheadMs);
        void s.edit(edits).then((ok) => ok && s.select(edits.map((x) => (x.type === "addEffect" ? x.effect.id : ""))));
      } else if (lower === "d" && s.selection.length > 0) {
        e.preventDefault();
        const index = buildIndex(doc);
        const copies = s.selection
          .map((id) => index.byId.get(id))
          .filter((p) => p !== undefined)
          .map((p) => ({ rowId: p.rowId, effect: p.effect }));
        const end = Math.max(...copies.map((c) => c.effect.endMs));
        const edits = pasteEffects(doc, index, copies, end);
        void s.edit(edits).then((ok) => ok && s.select(edits.map((x) => (x.type === "addEffect" ? x.effect.id : ""))));
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
}
