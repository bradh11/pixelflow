// The browser's stand-ins against the engine. Each file in parity/ is a script of commands with
// what the real engine answered and left behind at every step (written by the shell's tests, see
// app/src-tauri/src/tests/parity.rs). The same script is run here against MemoryBackend and
// MemorySequencer, and every step compared. Where the stand-ins differ on purpose, the
// difference is listed in KNOWN or ENGINE_FILLS_IN with the reason; one that's listed but no
// longer there fails too.

import { describe, expect, it } from "vitest";
import { errorMessage } from "../backend";
import { MemoryBackend } from "../memory";
import { MemorySequencer } from "../memorySequencer";
import { type Row, type Sequence, type SequenceChanges, type SequenceEdit, type SequenceEditResult, type SequenceIssue, applySequenceChanges, noChanges } from "../sequence";
import type { ChannelMap, DeviceDetails, Edit, Issue, Show, Summary, UseProps } from "../types";
import { listJson, readJson } from "./support";

/** What a snapshot of the show or the open sequence says about undo and saving. */
interface Flags {
  revision: number;
  path: string | null;
  dirty: boolean;
  canUndo: boolean;
  canRedo: boolean;
}

/** How things stand after a step. The `…Now` parts are there only where the step changed them. */
interface State {
  show: Flags;
  sequence: Flags | null;
  showNow?: Show;
  channelMapNow?: ChannelMap;
  summaryNow?: Summary;
  issuesNow?: Issue[];
  /** The whole document: after a run of edits, or when a step replaced it. */
  sequenceNow?: Sequence | null;
  sequenceIssuesNow?: SequenceIssue[] | null;
}

interface Step extends State {
  command: string;
  args: Record<string, unknown>;
  /** A rejection's message, an edit's changes (only the parts that list something), or a reply that isn't a snapshot. */
  reply: { error?: unknown; changed?: boolean; changes?: Partial<SequenceChanges>; value?: unknown };
}

interface Scenario {
  about: string;
  /** The controllers the stand-in is told about, as the engine sees them with nothing in the show. */
  devices: DeviceDetails[];
  start: State;
  steps: Step[];
}

/** A difference the stand-ins have on purpose: `what` is the part of a step that differs. */
interface Known {
  /** The scenario (its file's name), or any of them. */
  scenario: string;
  /** The steps it shows in (from 0), or every step it shows in. */
  steps: number[] | "any";
  what: string;
  why: string;
}

const KNOWN: Known[] = [
  {
    scenario: "any",
    steps: "any",
    what: "issuesNow",
    why: "the stand-in maps channels but doesn't check the wiring (see memory.ts): only the engine reports a show's problems",
  },
  {
    scenario: "showRejectedEdits",
    steps: [23],
    what: "reply.error",
    why: "the shell refuses this size while reading the edit, so the engine's sentence arrives behind Tauri's \"invalid args `edits` for command `apply_edits`:\"; the stand-in says only the sentence",
  },
];

/**
 * Fields of the show and the sequence the engine writes with its default where the window left
 * them out, and the stand-ins leave out too (`.*` is any field there): only the engine having
 * one isn't a difference.
 */
const ENGINE_FILLS_IN = [
  "groups[].gridSize",
  "groups[].layout",
  "props[].shape.*",
  "rows[].layers[].effects[].blur",
  "rows[].layers[].effects[].sparkleColor",
  "rows[].layers[].effects[].sparkles",
  "rows[].layers[].effects[].params.*",
];
const filledIn = new Set<string>();

type Args = Record<string, unknown>;

/** How each command of a script is asked of the stand-ins. */
const CALLS: Record<string, (backend: MemoryBackend, sequencer: MemorySequencer, args: Args) => Promise<unknown>> = {
  apply_edits: (b, _s, a) => b.applyEdits(a.edits as Edit[]),
  undo: (b) => b.undo(),
  redo: (b) => b.redo(),
  new_show: (b, _s, a) => b.newShow(a.name as string),
  open_sample_show: (b) => b.openSampleShow(),
  open_show: (b, _s, a) => b.openShow(a.path as string),
  save_show: (b) => b.saveShow(),
  save_show_as: (b, _s, a) => b.saveShowAs(a.path as string),
  inspect_device: (b, _s, a) => b.inspectDevice(a.address as string),
  import_device: (b, _s, a) => b.importDevice(a.address as string, (a.useProps as UseProps | undefined) ?? undefined),
  import_fpp_destination: (b, _s, a) => b.importFppDestination(a.address as string, a.destination as string, a.protocol as string),
  fpp_setup_plan: (b, _s, a) => b.fppSetupPlan(a.address as string),
  fpp_set_up_show: (b, _s, a) => b.fppSetUpShow(a.address as string, a.expected as string[]),
  new_sequence_doc: (_b, s, a) => s.newSequenceDoc(a.name as string, a.durationMs as number, a.audio as string | null, a.rows as Row[] | undefined),
  open_sequence_doc: (_b, s, a) => s.openSequenceDoc(a.path as string),
  save_sequence_doc: (_b, s) => s.saveSequenceDoc(),
  save_sequence_doc_as: (_b, s, a) => s.saveSequenceDocAs(a.path as string),
  close_sequence_doc: (_b, s) => s.closeSequenceDoc(),
  edit_sequence: (_b, s, a) => s.editSequence(a.edits as SequenceEdit[], a.gesture as string | undefined),
  undo_sequence: (_b, s) => s.undoSequence(),
  redo_sequence: (_b, s) => s.redoSequence(),
  add_sequence_doc_to_show: (_b, s, a) => s.addSequenceDocToShow(a.path as string),
};

const ID = /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/gi;

/**
 * `value` with its keys in order and the ids that were made at random numbered as they first
 * appear (ids a script chose, "…-0000-4000-8000-…", are kept): two values that differ only in
 * the ids each side made up then read the same.
 */
function settled<T>(value: T): T {
  const ids = new Map<string, string>();
  const text = (s: string) =>
    s.replace(ID, (found) => {
      if (found.slice(8, 24) === "-0000-4000-8000-") return found;
      const key = found.toLowerCase();
      if (!ids.has(key)) ids.set(key, `id-${ids.size + 1}`);
      return ids.get(key)!;
    });
  const walk = (v: unknown): unknown => {
    if (typeof v === "string") return text(v);
    if (Array.isArray(v)) return v.map(walk);
    if (v !== null && typeof v === "object") {
      return Object.fromEntries(
        Object.entries(v)
          .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
          .map(([key, inner]) => [text(key), walk(inner)]),
      );
    }
    return v;
  };
  return walk(value) as T;
}

/** A value in a few words, for saying how two differ. */
function brief(value: unknown): string {
  const text = JSON.stringify(value) ?? "nothing";
  return text.length > 120 ? `${text.slice(0, 117)}…` : text;
}

/** Whether `at` is a field of ENGINE_FILLS_IN (noted as met). */
function engineFillsIn(at: string): boolean {
  const path = at.replace(/\[\d+\]/g, "[]");
  const listed = ENGINE_FILLS_IN.find((p) => p === path || (p.endsWith(".*") && path.startsWith(p.slice(0, -1)) && !path.slice(p.length - 1).includes(".")));
  if (listed) filledIn.add(listed);
  return listed !== undefined;
}

/**
 * The first place `ours` (the stand-in's) and `theirs` (the engine's) differ, or null. Nothing is
 * nothing, written as null or left out. With `document` (the show or the sequence), a field the
 * engine fills in isn't a difference.
 */
function firstDifference(ours: unknown, theirs: unknown, document: boolean, at = ""): string | null {
  const say = () => `${at || "it"}: ${brief(ours)} here, ${brief(theirs)} in the engine`;
  if (ours === null || ours === undefined) return theirs === null || theirs === undefined || (document && engineFillsIn(at)) ? null : say();
  if (Array.isArray(ours)) {
    if (!Array.isArray(theirs)) return say();
    if (ours.length !== theirs.length) return `${at || "it"}: ${ours.length} here (${brief(ours)}), ${theirs.length} in the engine (${brief(theirs)})`;
    for (const [i, item] of ours.entries()) {
      const found = firstDifference(item, theirs[i], document, `${at}[${i}]`);
      if (found) return found;
    }
    return null;
  }
  if (typeof ours === "object") {
    if (theirs === null || typeof theirs !== "object" || Array.isArray(theirs)) return say();
    const keys = new Set([...Object.keys(ours), ...Object.keys(theirs)]);
    for (const key of [...keys].sort()) {
      const found = firstDifference((ours as Record<string, unknown>)[key], (theirs as Record<string, unknown>)[key], document, at ? `${at}.${key}` : key);
      if (found) return found;
    }
    return null;
  }
  return ours === theirs ? null : say();
}

/** How the two differ, ids aside, or null. */
function differs(ours: unknown, theirs: unknown): string | null {
  return firstDifference(settled(ours), settled(theirs), false);
}

/** How two shows or two sequences differ, ids and what the engine fills in aside, or null. */
function documentDiffers(ours: unknown, theirs: unknown): string | null {
  return firstDifference(settled(ours), settled(theirs), true);
}

interface Difference {
  step: number;
  what: string;
  detail: string;
}

/** Runs `scenario` against the stand-ins; every difference from the engine, step by step. */
async function differences(scenario: Scenario): Promise<Difference[]> {
  const backend = new MemoryBackend();
  const sequencer = new MemorySequencer(backend);
  backend.deviceNetwork = { details: structuredClone(scenario.devices), silent: [] };
  const found: Difference[] = [];
  // The engine's side, carried from step to step (the file says only what changed).
  const engine = { ...scenario.start };
  let engineDoc: Sequence | null = scenario.start.sequenceNow ?? null;
  // The stand-ins' side before the step.
  let before = { show: await backend.getSnapshot(), sequence: await sequencer.getSequenceDoc() };

  for (const [index, step] of [undefined, ...scenario.steps].entries()) {
    const at = index - 1;
    const note = (what: string, detail: string | null) => {
      if (detail !== null) found.push({ step: at, what, detail });
    };
    let reply: unknown;
    let refused: string | null = null;
    if (step) {
      const run = CALLS[step.command];
      if (!run) throw new Error(`parity.test.ts doesn't know how to ask the stand-ins for ${step.command}`);
      try {
        reply = await run(backend, sequencer, step.args);
      } catch (error) {
        refused = errorMessage(error);
      }
      note("reply.error", differs(refused, step.reply.error ?? null));
    }
    const after = { show: await backend.getSnapshot(), sequence: await sequencer.getSequenceDoc() };
    const then = step ?? scenario.start;
    const earlier = { ...engine };
    Object.assign(engine, then);

    if (step?.reply.changes !== undefined && refused === null) {
      // The engine's reply, applied the way the window applies it, must give the engine's own
      // document: checked wherever the file has the document whole.
      const changes = { ...noChanges(), ...step.reply.changes };
      engineDoc = applySequenceChanges(engineDoc!, changes);
      if (step.sequenceNow !== undefined) expect(engineDoc, `${step.command} (step ${at}): applySequenceChanges on the engine's reply`).toEqual(step.sequenceNow);
      // And the stand-in's own reply must give the stand-in's own document.
      const ours = reply as SequenceEditResult;
      expect(applySequenceChanges(before.sequence!.sequence, ours.changes), `${step.command} (step ${at}): applySequenceChanges on the stand-in's reply`).toEqual(after.sequence!.sequence);
      note("reply.changed", differs(ours.changed, step.reply.changed));
    } else if (then.sequenceNow !== undefined) {
      engineDoc = then.sequenceNow;
    }
    if (step && "value" in step.reply && refused === null) note("reply.value", differs(reply, step.reply.value));

    // The show.
    const flags = (ours: Flags, theirs: Flags, was: { ours: Flags; theirs: Flags } | null, name: string) => {
      for (const key of ["path", "dirty", "canUndo", "canRedo"] as const) note(`${name}.${key}`, differs(ours[key], theirs[key]));
      if (was) note(`${name}.revision`, differs(ours.revision - was.ours.revision, theirs.revision - was.theirs.revision));
    };
    flags(after.show, engine.show, step ? { ours: before.show, theirs: earlier.show } : null, "show");
    note("showNow", documentDiffers(after.show.show, engine.showNow));
    note("channelMapNow", differs(after.show.channelMap, engine.channelMapNow));
    note("summaryNow", differs(after.show.summary, engine.summaryNow));
    note("issuesNow", differs(after.show.issues, engine.issuesNow));

    // The open sequence.
    if (after.sequence === null || engine.sequence === null) {
      note("sequence", differs(after.sequence === null, engine.sequence === null));
    } else {
      const was = step && before.sequence && earlier.sequence ? { ours: before.sequence, theirs: earlier.sequence } : null;
      flags(after.sequence, engine.sequence, was, "sequence");
      note("sequenceNow", documentDiffers(after.sequence.sequence, engineDoc));
      note("sequenceIssuesNow", differs(after.sequence.issues, engine.sequenceIssuesNow));
    }
    before = after;
  }
  return found;
}

const files = listJson("parity");
const used = new Set<Known>();

describe("the stand-ins do what the engine does", () => {
  it.each(files)("%s", async (file) => {
    const name = file.replace(/\.json$/, "");
    const scenario = readJson<Scenario>(`parity/${file}`);
    const surprises: string[] = [];
    for (const d of await differences(scenario)) {
      const known = KNOWN.find((k) => (k.scenario === "any" || k.scenario === name) && k.what === d.what && (k.steps === "any" || k.steps.includes(d.step)));
      if (known) used.add(known);
      else surprises.push(`step ${d.step} (${d.step < 0 ? "before any" : scenario.steps[d.step].command}), ${d.what}: ${d.detail}`);
    }
    expect(surprises.join("\n"), scenario.about).toBe("");
  });

  it("differ only where it's known and still true", () => {
    expect(files.length).toBeGreaterThan(12);
    const stale = KNOWN.filter((k) => !used.has(k)).map((k) => `${k.scenario}, ${k.what}: no longer differs (${k.why})`);
    expect(stale).toEqual([]);
    expect(ENGINE_FILLS_IN.filter((field) => !filledIn.has(field))).toEqual([]);
  });
});

describe("the check itself", () => {
  it("notices a step the engine answered differently", async () => {
    const scenario = readJson<Scenario>("parity/showEdits.json");
    const at = scenario.steps.findIndex((step) => step.summaryNow !== undefined && step.showNow !== undefined);
    const step = scenario.steps[at];
    step.show.dirty = !step.show.dirty;
    step.summaryNow!.pixels += 1;
    step.showNow!.props[0].name = "Another name";
    const accepted = scenario.steps.findIndex((s) => s.reply.error === undefined);
    scenario.steps[accepted].reply.error = "The engine refused this.";

    const found = (await differences(scenario)).map((d) => `${d.step} ${d.what}`);
    expect(found).toContain(`${at} show.dirty`);
    expect(found).toContain(`${at} summaryNow`);
    expect(found).toContain(`${at} showNow`);
    expect(found).toContain(`${accepted} reply.error`);
  });

  it("lets the engine fill in only the fields that are listed", () => {
    expect(documentDiffers({ props: [{ shape: { type: "arch" } }] }, { props: [{ shape: { type: "arch", arc: 180 } }] })).toBeNull();
    expect(documentDiffers({ props: [{ shape: { type: "arch" } }] }, { props: [{ name: "Arch", shape: { type: "arch" } }] })).toContain("props[0].name");
    expect(differs({ props: [{ shape: { type: "arch" } }] }, { props: [{ shape: { type: "arch", arc: 180 } }] })).toContain("props[0].shape.arc");
  });
});
