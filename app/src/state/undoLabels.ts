// What Undo and Redo would take back or bring back, for their tooltips ("Undo: Move Mega Tree").
//
// The engine keeps whole snapshots for undo, not what each step was, so the names are kept here,
// beside it: each edit pushes its name, undo and redo move names between the stacks. The stacks
// are only trusted at the revision they were last brought up to; a change made any other way (an
// assistant proposal, a restored backup, another show) puts them aside, and the buttons say plain
// "Undo" until the next edit.

import { create } from "zustand";

interface Step {
  label: string;
  /** Edits in one gesture (a slider pulled) are one undo step. */
  gesture: string | null;
}

export interface LabelStacks {
  undo: Step[];
  redo: Step[];
  /** The revision the stacks are true for; null when they aren't known. */
  at: number | null;
}

export const NO_LABELS: LabelStacks = { undo: [], redo: [], at: null };

/** After an edit took the document from revision `from` to `to`. */
export function edited(s: LabelStacks, from: number, to: number, label: string, gesture: string | null = null): LabelStacks {
  const undo = s.at === from ? s.undo : [];
  const top = undo.at(-1);
  if (gesture !== null && top?.gesture === gesture) return { undo: [...undo.slice(0, -1), { label, gesture }], redo: [], at: to };
  return { undo: [...undo, { label, gesture }], redo: [], at: to };
}

/** After an undo (or, `redo`, a redo) took the document from revision `from` to `to`. */
export function stepped(s: LabelStacks, from: number, to: number, redo = false): LabelStacks {
  if (s.at !== from) return { ...NO_LABELS, at: null };
  const [take, give] = redo ? [s.redo, s.undo] : [s.undo, s.redo];
  const step = take.at(-1);
  if (!step) return { ...NO_LABELS, at: null };
  const taken = take.slice(0, -1);
  const given = [...give, { ...step, gesture: null }];
  return redo ? { undo: given, redo: taken, at: to } : { undo: taken, redo: given, at: to };
}

/** The names of the next undo and redo at `revision`, when known. */
export function nextLabels(s: LabelStacks, revision: number | null | undefined): { undo: string | null; redo: string | null } {
  if (revision === null || revision === undefined || s.at !== revision) return { undo: null, redo: null };
  return { undo: s.undo.at(-1)?.label ?? null, redo: s.redo.at(-1)?.label ?? null };
}

export type UndoDoc = "show" | "sequence";

/**
 * The names for the show and for the open sequence; and on the Sequence screen, which document
 * each undo there took back, newest last (so Redo brings them back in turn).
 */
export const useUndoLabels = create<{ show: LabelStacks; sequence: LabelStacks; undone: UndoDoc[] }>(() => ({
  show: NO_LABELS,
  sequence: NO_LABELS,
  undone: [],
}));
