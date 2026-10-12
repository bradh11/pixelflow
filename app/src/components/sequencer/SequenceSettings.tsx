import { Settings2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { MusicLevelSource, RowBlending } from "../../api/sequence";
import { fileName, shownPath } from "../../lib/format";
import { MUSIC_LEVEL, MUSIC_LEVEL_HINT, MUSIC_LEVEL_SOURCES } from "../../lib/labels";
import { useSequencer } from "../../state/sequencer";
import { Button, Select } from "../ui";

/** How a row can meet the rows above it. The timeline lists rows in the order they're drawn, so
 * the rows a row is drawn over are the ones above it in the list. Each of xLights' two ways
 * names its Model Blending setting, which an imported sequence keeps. */
const ROW_BLENDINGS: { value: RowBlending; label: string; help: string }[] = [
  { value: "cover", label: "Cover", help: "A row covers the rows above it where it's lit, and lets them show where it's dark." },
  {
    value: "blend",
    label: "Blend with the rows above",
    help: "A row's effects mix with the rows above it the same way they mix with the layers below (xLights: Model Blending on).",
  },
  {
    value: "replace",
    label: "Replace",
    help: "A row with an effect playing takes over all its pixels, lit or dark. A submodel still mixes with its own prop's row (xLights: Model Blending off).",
  },
];

/** "25 ms apart (40 a second)". */
function frameTime(frameMs: number): string {
  return `${frameMs} ms apart (${Math.round(10_000 / frameMs) / 10} a second)`;
}

/**
 * The open sequence's own settings, behind a small button beside its name: how its rows meet the
 * rows above them and which channel of its music is how loud it is (edits like any other, so
 * Undo takes them back), and its music and frame time. Escape, or a click outside, closes the
 * panel.
 */
export function SequenceSettings() {
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    if (!open) return;
    box.current?.querySelector<HTMLElement>("input:checked")?.focus();
    // Ahead of the screen's own keys, so Escape only closes the panel.
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      setOpen(false);
      box.current?.querySelector<HTMLElement>("button")?.focus();
    };
    const onDown = (e: PointerEvent) => {
      if (!box.current?.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("pointerdown", onDown);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("pointerdown", onDown);
    };
  }, [open]);
  return (
    <span ref={box} className="relative inline-flex">
      <button
        type="button"
        aria-label="Sequence settings"
        title="Sequence settings"
        aria-haspopup="dialog"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
        className="inline-flex items-center rounded-md px-1.5 py-1.5 text-neutral-700 hover:bg-neutral-200/70 dark:text-neutral-200 dark:hover:bg-neutral-800"
      >
        <Settings2 size={16} />
      </button>
      {open && <SettingsPanel />}
    </span>
  );
}

function SettingsPanel() {
  const doc = useSequencer((s) => s.doc);
  if (!doc) return null;
  const rowBlending = doc.rowBlending ?? "cover";
  return (
    <div
      role="dialog"
      aria-label="Sequence settings"
      className="absolute top-full left-0 z-30 mt-1 w-[22rem] max-w-[calc(100vw-2rem)] rounded-lg border border-neutral-200 bg-white p-3 text-sm text-neutral-800 shadow-xl dark:border-neutral-800 dark:bg-neutral-900 dark:text-neutral-100"
    >
      <fieldset className="flex flex-col gap-1.5">
        <legend className="mb-1 text-neutral-600 dark:text-neutral-400">Rows</legend>
        <p className="text-xs text-neutral-500">Rows are drawn from the top of the list down. Where two rows light the same pixels:</p>
        {ROW_BLENDINGS.map((b) => (
          <label key={b.value} className="flex items-start gap-2">
            <input
              type="radio"
              name="sequence-rows"
              checked={rowBlending === b.value}
              onChange={() => void useSequencer.getState().edit([{ type: "setRowBlending", rowBlending: b.value }])}
              className="mt-0.5 accent-accent-500"
            />
            <span>
              {b.label}
              <span className="block text-xs text-neutral-500">{b.help}</span>
            </span>
          </label>
        ))}
      </fieldset>
      <dl className="mt-3 grid grid-cols-[auto_1fr] items-center gap-x-3 gap-y-1 border-t border-neutral-200 pt-2 dark:border-neutral-800">
        <dt className="text-neutral-600 dark:text-neutral-400">Music</dt>
        <dd className="flex min-w-0 items-center gap-1">
          {doc.audio ? (
            <>
              <span className="truncate" title={shownPath(doc.audio)}>
                {fileName(doc.audio)}
              </span>
              <Button
                variant="ghost"
                className="shrink-0 px-2! py-0.5! text-xs"
                aria-label={`Locate ${fileName(doc.audio)}`}
                title="Choose where this file is now"
                onClick={() => void useSequencer.getState().locateMusic()}
              >
                Locate…
              </Button>
            </>
          ) : (
            "None"
          )}
        </dd>
        <dt className="text-neutral-600 dark:text-neutral-400">
          <label htmlFor="sequence-music-level">{MUSIC_LEVEL}</label>
        </dt>
        <dd>
          <Select
            id="sequence-music-level"
            className="py-1!"
            aria-describedby="sequence-music-level-hint"
            value={doc.musicLevelSource ?? "louderChannel"}
            onChange={(e) => void useSequencer.getState().edit([{ type: "setMusicLevelSource", musicLevelSource: e.target.value as MusicLevelSource }])}
          >
            {MUSIC_LEVEL_SOURCES.map((s) => (
              <option key={s.value} value={s.value}>
                {s.label}
              </option>
            ))}
          </Select>
        </dd>
        <dd id="sequence-music-level-hint" className="col-span-2 mb-1 text-xs text-neutral-500">
          {MUSIC_LEVEL_HINT}
        </dd>
        <dt className="text-neutral-600 dark:text-neutral-400">Frames</dt>
        <dd>{frameTime(doc.frameMs)}</dd>
      </dl>
    </div>
  );
}
