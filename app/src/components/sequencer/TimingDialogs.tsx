// Dialogs for timing tracks: adding one, generating its marks, and pasting lyrics onto it. Each
// sends its edits through the sequencer store, so a change is one undo step; a refused change
// keeps the dialog open with the engine's explanation in the error banner.

import { useEffect, useRef, useState } from "react";
import { lyricLines } from "../../api/timingMarks";
import type { Sequence, SequenceEdit, TimingKind, TimingTrack } from "../../api/sequence";
import { formatTime, markIndices, parseTime, wordsTrackFor } from "../../lib/timelineMath";
import { useSequencer } from "../../state/sequencer";
import { Button, Input, Select } from "../ui";

/** Kinds a new timing track can be (phonemes only come from xLights). */
export const TRACK_KINDS: { kind: TimingKind; label: string; hint: string }[] = [
  { kind: "beats", label: "Beats", hint: "One mark per beat" },
  { kind: "bars", label: "Bars", hint: "One mark per bar" },
  { kind: "sections", label: "Sections", hint: "Verse, chorus, bridge…" },
  { kind: "lyrics", label: "Lyrics", hint: "One mark per sung phrase" },
  { kind: "words", label: "Words", hint: "One mark per sung word" },
  { kind: "custom", label: "Custom", hint: "Anything else to line effects up with" },
];

export function kindLabel(kind: TimingKind): string {
  return kind === "phonemes" ? "Phonemes" : (TRACK_KINDS.find((k) => k.kind === kind)?.label ?? kind);
}

function Dialog({ label, onClose, children }: { label: string; onClose: () => void; children: React.ReactNode }) {
  const box = useRef<HTMLDivElement>(null);
  const close = useRef(onClose);
  close.current = onClose;
  // The dialog takes the focus (and gives it back when it closes); Escape closes it.
  useEffect(() => {
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    box.current?.querySelector<HTMLElement>("textarea, input, select, button")?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      close.current();
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
      if (opener?.isConnected) opener.focus();
    };
  }, []);
  return (
    <div className="fixed inset-0 z-40 flex items-center justify-center bg-black/40">
      <div
        ref={box}
        role="dialog"
        aria-modal="true"
        aria-label={label}
        className="w-[30rem] max-w-[calc(100vw-2rem)] rounded-lg border border-neutral-200 bg-white p-5 text-sm shadow-xl dark:border-neutral-800 dark:bg-neutral-900"
      >
        <h2 className="text-lg font-semibold">{label}</h2>
        {children}
      </div>
    </div>
  );
}

function Actions({ onClose, okLabel, onOk, disabled }: { onClose: () => void; okLabel: string; onOk: () => void; disabled?: boolean }) {
  return (
    <div className="mt-5 flex justify-end gap-2">
      <Button variant="ghost" onClick={onClose}>
        Cancel
      </Button>
      <Button variant="primary" onClick={onOk} disabled={disabled}>
        {okLabel}
      </Button>
    </div>
  );
}

/** A time typed as 1:05.250 or 65.25, checked as it's typed. */
function TimeInput({ label, value, onChange }: { label: string; value: string; onChange: (v: string) => void }) {
  const bad = parseTime(value) === null;
  return (
    <label className="flex flex-col gap-1">
      <span className="text-neutral-600 dark:text-neutral-400">{label}</span>
      <Input value={value} aria-invalid={bad || undefined} onChange={(e) => onChange(e.target.value)} className="w-32 tabular-nums" />
    </label>
  );
}

const TIME_HINT = "Type times like 1:05.250 or 65.25 (seconds).";

/** A new, empty timing track: a name and what it marks. */
export function AddTimingTrackDialog({ doc, onClose }: { doc: Sequence; onClose: () => void }) {
  const [kind, setKind] = useState<TimingKind>("lyrics");
  const suggested = (k: TimingKind) => {
    const base = kindLabel(k);
    const taken = new Set(doc.timingTracks.map((t) => t.name));
    let name = base;
    for (let n = 2; taken.has(name); n++) name = `${base} ${n}`;
    return name;
  };
  const [name, setName] = useState(() => suggested("lyrics"));
  const [named, setNamed] = useState(false);
  const add = async () => {
    const track: TimingTrack = { id: crypto.randomUUID(), name: name.trim() || suggested(kind), kind, marks: [] };
    const ok = await useSequencer.getState().edit([{ type: "addTimingTrack", track }]);
    if (!ok) return;
    useSequencer.getState().selectMarks(track.id, []);
    onClose();
  };
  return (
    <Dialog label="Add timing track" onClose={onClose}>
      <p className="mt-1 text-neutral-500">
        Timing tracks hold marks that effects snap to. Select one and press T in time with the music to tap out its marks.
      </p>
      <div className="mt-4 flex flex-col gap-3">
        <label className="flex flex-col gap-1">
          <span className="text-neutral-600 dark:text-neutral-400">What it marks</span>
          <Select
            value={kind}
            onChange={(e) => {
              const k = e.target.value as TimingKind;
              setKind(k);
              if (!named) setName(suggested(k));
            }}
          >
            {TRACK_KINDS.map((k) => (
              <option key={k.kind} value={k.kind}>
                {k.label} — {k.hint}
              </option>
            ))}
          </Select>
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-neutral-600 dark:text-neutral-400">Name</span>
          <Input
            value={name}
            onChange={(e) => {
              setName(e.target.value);
              setNamed(true);
            }}
            onKeyDown={(e) => e.key === "Enter" && void add()}
          />
        </label>
      </div>
      <Actions onClose={onClose} okLabel="Add track" onOk={() => void add()} />
    </Dialog>
  );
}

/** Fills a track with marks: one every so often over a stretch of the song, or every Nth mark of
 * another track (every 4th beat for bars). Marks already there in that time are replaced. */
export function GenerateMarksDialog({ doc, track, onClose }: { doc: Sequence; track: TimingTrack; onClose: () => void }) {
  const others = doc.timingTracks.filter((t) => t.id !== track.id && t.marks.length > 0);
  const [mode, setMode] = useState<"interval" | "track">("interval");
  const [every, setEvery] = useState("500");
  const [from, setFrom] = useState(formatTime(0));
  const [to, setTo] = useState(formatTime(doc.durationMs));
  const [source, setSource] = useState(others.find((t) => t.kind === "beats")?.id ?? others[0]?.id ?? "");
  const [nth, setNth] = useState("1");
  const fromMs = parseTime(from);
  const toMs = parseTime(to);
  const bad = mode === "interval" && (fromMs === null || toMs === null);
  const generate = async () => {
    if (bad) return;
    const edit: SequenceEdit =
      mode === "interval"
        ? { type: "generateMarks", track: track.id, everyMs: Math.round(Number(every) || 0), fromMs: fromMs!, toMs: Math.min(toMs!, doc.durationMs) }
        : { type: "copyMarks", from: source, to: track.id, every: Math.max(1, Math.round(Number(nth) || 1)) };
    if (await useSequencer.getState().edit([edit])) onClose();
  };
  return (
    <Dialog label={`Generate marks on ${track.name}`} onClose={onClose}>
      <div className="mt-4 flex flex-col gap-3">
        <label className="flex items-center gap-2">
          <input type="radio" name="generate" checked={mode === "interval"} onChange={() => setMode("interval")} />
          A mark at a steady pace
        </label>
        {mode === "interval" && (
          <div className="ml-6 flex flex-wrap items-end gap-3">
            <label className="flex flex-col gap-1">
              <span className="text-neutral-600 dark:text-neutral-400">Every (ms)</span>
              <Input type="number" min={10} step={25} value={every} onChange={(e) => setEvery(e.target.value)} className="w-28" />
            </label>
            <TimeInput label="From" value={from} onChange={setFrom} />
            <TimeInput label="To" value={to} onChange={setTo} />
          </div>
        )}
        {bad && <p className="ml-6 text-red-600 dark:text-red-400">{TIME_HINT}</p>}
        <label className={`flex items-center gap-2 ${others.length === 0 ? "opacity-50" : ""}`}>
          <input type="radio" name="generate" disabled={others.length === 0} checked={mode === "track"} onChange={() => setMode("track")} />
          From another track{others.length === 0 && " (no other track has marks yet)"}
        </label>
        {mode === "track" && (
          <div className="ml-6 flex flex-wrap items-end gap-3">
            <label className="flex flex-col gap-1">
              <span className="text-neutral-600 dark:text-neutral-400">Track</span>
              <Select value={source} onChange={(e) => setSource(e.target.value)}>
                {others.map((t) => (
                  <option key={t.id} value={t.id}>
                    {t.name}
                  </option>
                ))}
              </Select>
            </label>
            <label className="flex flex-col gap-1">
              <span className="text-neutral-600 dark:text-neutral-400">Take every</span>
              <Input type="number" min={1} value={nth} onChange={(e) => setNth(e.target.value)} className="w-20" aria-describedby="nth-hint" />
            </label>
            <span id="nth-hint" className="pb-2 text-xs text-neutral-500">
              1 copies every mark; 4 makes one per bar of 4 beats.
            </span>
          </div>
        )}
        <p className="text-xs text-neutral-500">
          {mode === "interval" ? "Marks already on this track in that time are replaced." : "This track's marks are replaced."} Undo puts them back.
        </p>
      </div>
      <Actions onClose={onClose} okLabel="Generate" onOk={() => void generate()} disabled={bad} />
    </Dialog>
  );
}

/** Lyrics pasted one phrase per line, spread over a stretch of the song (longer lines get more time)
 * or put onto the marks selected on this track, one line each. */
export function PasteLyricsDialog({ doc, track, onClose }: { doc: Sequence; track: TimingTrack; onClose: () => void }) {
  const markSelection = useSequencer((s) => s.markSelection);
  const chosen = markSelection?.track === track.id ? markIndices(track, markSelection.starts) : [];
  const first = chosen.length > 0 ? track.marks[chosen[0]] : null;
  const last = chosen.length > 0 ? track.marks[chosen[chosen.length - 1]] : null;
  const [text, setText] = useState("");
  const [mode, setMode] = useState<"range" | "marks">(chosen.length > 1 ? "marks" : "range");
  const [from, setFrom] = useState(formatTime(first?.startMs ?? 0));
  const [to, setTo] = useState(formatTime(last?.endMs ?? doc.durationMs));
  const lines = lyricLines(text);
  const fromMs = parseTime(from);
  const toMs = parseTime(to);
  const badTime = mode === "range" && (fromMs === null || toMs === null);
  const mismatch = mode === "marks" && lines.length > 0 && lines.length !== chosen.length;
  const paste = async () => {
    if (badTime || mismatch) return;
    const edit: SequenceEdit =
      mode === "marks"
        ? { type: "labelMarks", track: track.id, indices: chosen, labels: lines }
        : { type: "spreadLyrics", track: track.id, lines, fromMs: fromMs!, toMs: Math.min(toMs!, doc.durationMs) };
    if (await useSequencer.getState().edit([edit])) onClose();
  };
  return (
    <Dialog label={`Paste lyrics onto ${track.name}`} onClose={onClose}>
      <div className="mt-4 flex flex-col gap-3">
        <label className="flex flex-col gap-1">
          <span className="text-neutral-600 dark:text-neutral-400">Lyrics, one phrase per line</span>
          <textarea
            value={text}
            onChange={(e) => setText(e.target.value)}
            rows={8}
            placeholder={"Deck the halls with boughs of holly\nFa la la la la, la la la la"}
            className="rounded-md border border-neutral-300 bg-white px-2 py-1.5 font-mono text-sm dark:border-neutral-700 dark:bg-neutral-950"
          />
        </label>
        <label className="flex items-center gap-2">
          <input type="radio" name="spread" checked={mode === "range"} onChange={() => setMode("range")} />
          Spread over a stretch of the song (longer lines get more time)
        </label>
        {mode === "range" && (
          <div className="ml-6 flex items-end gap-3">
            <TimeInput label="From" value={from} onChange={setFrom} />
            <TimeInput label="To" value={to} onChange={setTo} />
          </div>
        )}
        {badTime && <p className="ml-6 text-red-600 dark:text-red-400">{TIME_HINT}</p>}
        <label className={`flex items-center gap-2 ${chosen.length === 0 ? "opacity-50" : ""}`}>
          <input type="radio" name="spread" disabled={chosen.length === 0} checked={mode === "marks"} onChange={() => setMode("marks")} />
          Onto the {chosen.length === 1 ? "selected mark" : `${chosen.length} selected marks`}, one line each
        </label>
        {mismatch && (
          <p className="ml-6 text-amber-700 dark:text-amber-400">
            There are {lines.length} lines and {chosen.length} selected marks. Select one mark per line, or spread the lyrics over the song instead.
          </p>
        )}
        <p className="text-xs text-neutral-500">
          {mode === "range" ? "Marks already on this track in that time are replaced." : "The selected marks get the lines as labels."} Then use Break into words to time each word.
        </p>
      </div>
      <Actions onClose={onClose} okLabel={lines.length > 0 ? `Add ${lines.length === 1 ? "1 line" : `${lines.length} lines`}` : "Add lyrics"} onOk={() => void paste()} disabled={lines.length === 0 || badTime || mismatch} />
    </Dialog>
  );
}

/** Breaks the selected phrase marks of a track (or every phrase with words, when none is selected)
 * into word marks on its words track, adding "<name> (words)" right below it when there's none. */
export function breakIntoWords(trackId: string): Promise<boolean> {
  const store = useSequencer.getState();
  const picked = store.markSelection?.track === trackId ? store.markSelection.starts : null;
  return store.edit((doc) => {
    const at = doc.timingTracks.findIndex((t) => t.id === trackId);
    const track = doc.timingTracks[at];
    if (!track) return [];
    const indices = picked ? markIndices(track, picked) : track.marks.flatMap((m, i) => (m.label.trim() ? [i] : []));
    const edits: SequenceEdit[] = [];
    let words = wordsTrackFor(doc, track);
    if (!words) {
      words = { id: crypto.randomUUID(), name: `${track.name} (words)`, kind: "words", marks: [] };
      edits.push({ type: "addTimingTrack", track: words }, { type: "moveTimingTrack", id: words.id, index: at + 1 });
    }
    edits.push({ type: "breakIntoWords", track: track.id, indices, words: words.id });
    return edits;
  });
}
