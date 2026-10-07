import { useEffect } from "react";
import { markIndices, nudgeEdits, stepTime } from "../../lib/timelineMath";
import { copyEffects, deleteEffects, duplicateEffects, pasteEffectsAtPlayhead } from "../../state/sequenceActions";
import { newGesture, useSequencer } from "../../state/sequencer";
import { useApp } from "../../state/store";

/** Beat times from the sequence's beats track (or its first track), for Shift-steps. */
function beatTimes(): number[] {
  const doc = useSequencer.getState().doc;
  const track = doc?.timingTracks.find((t) => t.kind === "beats") ?? doc?.timingTracks[0];
  return track?.marks.map((m) => m.startMs) ?? [];
}

/**
 * The Sequence screen's keys (not while typing in a field): Space plays and pauses; arrows move
 * the selected effects, or the playhead, by a frame (with Shift, to the next beat); Up and Down pick
 * the row above or below and the effect under the playhead on it; Home and End
 * jump (the timeline follows the playhead there); Delete removes; ⌘C, ⌘V, and ⌘D copy, paste at
 * the playhead, and duplicate; ⌘A selects everything; Escape clears the selection; T taps a timing
 * mark in at the playhead (tap to time); L turns looping on and off.
 * Undo, redo, and save are global shortcuts.
 */
export function useSequenceKeys() {
  useEffect(() => {
    /** The arrow key being held: its repeats make one undo step. */
    let nudge = "";
    const onKey = (e: KeyboardEvent) => {
      // Something else already took the key (a palette item adding an effect, a dialog closing).
      if (e.defaultPrevented) return;
      const target = e.target as HTMLElement | null;
      if (target && (["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName) || target.isContentEditable)) return;
      // Not while a dialog or a menu (a timing track's ⋯ menu) has the keys.
      if (target?.closest?.("[role=dialog], [role=menu]")) return;
      const app = useApp.getState();
      if (app.paletteOpen || app.pendingReplace) return;
      const s = useSequencer.getState();
      if (s.replacing) return;
      const doc = s.doc;
      if (!doc) return;
      const mod = e.metaKey || e.ctrlKey;
      const key = e.key;
      if (key === " " && !mod) {
        // Space on a focused button presses that button instead.
        if (target?.closest?.("button, a, [role=button], [role=checkbox], [role=menuitem]")) return;
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
          s.reveal();
          return;
        }
        // Holding the key down is one step to undo. Each step is worked out from where the effects
        // are when its turn comes, so quick presses add up even while the engine is answering.
        if (!e.repeat || !nudge) nudge = newGesture();
        const ids = s.selection;
        const byBeat = e.shiftKey;
        void s.edit((latest) => nudgeEdits(latest, ids, direction, byBeat), nudge).then((ok) => ok && useSequencer.getState().reveal());
        return;
      }
      if ((key === "ArrowUp" || key === "ArrowDown") && !mod && doc.rows.length > 0) {
        // Up and down pick the row above or below, and the effect under the playhead on it.
        e.preventDefault();
        const at = doc.rows.findIndex((r) => r.id === s.activeRow);
        const next = at < 0 ? 0 : Math.max(0, Math.min(doc.rows.length - 1, at + (key === "ArrowDown" ? 1 : -1)));
        const row = doc.rows[next];
        const under = [...row.layers].reverse().flatMap((l) => l.effects).find((x) => x.startMs <= s.playheadMs && s.playheadMs < x.endMs);
        s.select(under ? [under.id] : [], row.id);
        s.reveal();
        return;
      }
      if (key === "Home" || key === "End") {
        e.preventDefault();
        void s.seek(key === "Home" ? 0 : doc.durationMs);
        s.reveal("playhead");
        return;
      }
      if ((key === "l" || key === "L") && !mod && !e.altKey) {
        e.preventDefault();
        if (!e.repeat) s.setLooping(!s.looping);
        return;
      }
      if ((key === "t" || key === "T") && !mod && !e.altKey) {
        // Tap to time: a mark at the playhead on the picked timing track.
        e.preventDefault();
        if (!e.repeat) s.tap();
        return;
      }
      if ((key === "Delete" || key === "Backspace") && s.markSelection) {
        e.preventDefault();
        const { track, starts } = s.markSelection;
        s.selectMarks(track, []);
        void s.edit((latest) => {
          const t = latest.timingTracks.find((x) => x.id === track);
          const indices = t ? markIndices(t, starts) : [];
          return t && indices.length > 0 ? [{ type: "removeMarks", track, indices }] : [];
        });
        return;
      }
      if (key === "Escape" && s.markSelection) {
        s.selectMarks(s.markSelection.track, []);
        return;
      }
      if ((key === "Delete" || key === "Backspace") && s.selection.length > 0) {
        e.preventDefault();
        deleteEffects(s.selection);
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
        copyEffects();
      } else if (lower === "v" && s.clipboard.length > 0) {
        e.preventDefault();
        pasteEffectsAtPlayhead();
      } else if (lower === "d" && s.selection.length > 0) {
        e.preventDefault();
        duplicateEffects(s.selection);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
}
