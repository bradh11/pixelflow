import { ArrowDown, ArrowUp, FileQuestion, Music, Plus, Trash2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { SequenceEntry } from "../api/types";
import { useApp } from "../state/store";
import { useMissingFiles } from "./MissingFiles";
import { Button } from "./ui";

/**
 * Adds a rendered sequence (.fseq) to the playlist, picked from a file dialog, and selects it.
 * `label`: the button's words, where "Add sequence" alone wouldn't say enough.
 */
export function AddSequenceButton({ onSelect, label = "Add sequence" }: { onSelect: (id: string) => void; label?: string }) {
  const backend = useApp((s) => s.backend);
  const run = useApp((s) => s.run);
  const add = async () => {
    const path = await backend?.pickSequencePath();
    if (!path) return;
    if (await run((b) => b.addSequence(path))) {
      const added = useApp.getState().snapshot?.show.sequences.at(-1);
      if (added) onSelect(added.id);
    }
  };
  return (
    <Button onClick={add} aria-label={label} className="shrink-0" data-tip="Add a rendered sequence (.fseq) to the playlist; PixelFlow finds its music next to it">
      <Plus size={14} aria-hidden /> {label}
    </Button>
  );
}

/** The show's rendered sequences in playlist order: pick one (it plays), reorder, or remove. */
export function SequenceList({
  selected,
  playing,
  onSelect,
}: {
  selected: string | null;
  playing: string | null;
  onSelect: (id: string) => void;
}) {
  const sequences = useApp((s) => s.snapshot?.show.sequences ?? NO_SEQUENCES);
  const apply = useApp((s) => s.apply);
  const missing = useMissingFiles();
  /** Sequences with a file (the .fseq or its music) that isn't where it was. */
  const lost = new Set(missing.flatMap((m) => ("id" in m.file ? [m.file.id] : [])));
  // Reorder buttons by "<id>:up" / "<id>:down", so focus can follow a moved sequence.
  const buttons = useRef(new Map<string, HTMLButtonElement>());
  const [focusNext, setFocusNext] = useState<string | null>(null);
  useEffect(() => {
    if (!focusNext) return;
    buttons.current.get(focusNext)?.focus();
    setFocusNext(null);
  }, [focusNext, sequences]);

  const move = async (s: SequenceEntry, index: number, direction: "up" | "down") => {
    if (!(await apply([{ type: "moveSequence", id: s.id, index }]))) return;
    // At the top (or bottom) this button is disabled now: keep focus on the other one.
    const atEnd = direction === "up" ? index === 0 : index === sequences.length - 1;
    setFocusNext(`${s.id}:${atEnd ? (direction === "up" ? "down" : "up") : direction}`);
  };

  const keep = (key: string) => (el: HTMLButtonElement | null) => {
    if (el) buttons.current.set(key, el);
    else buttons.current.delete(key);
  };

  if (sequences.length === 0) return <p className="text-sm text-neutral-500">None yet. Add sequence takes a rendered sequence (.fseq) and finds its music next to it.</p>;
  return (
    <ol aria-label="Rendered sequences" className="flex flex-col gap-0.5">
      {sequences.map((s: SequenceEntry, i) => (
        <li key={s.id}>
          <div
            className={`flex items-center gap-1.5 rounded-md py-1 pr-1 pl-2 text-sm ${
              s.id === selected ? "bg-violet-100 dark:bg-violet-950/50" : "hover:bg-neutral-100 dark:hover:bg-neutral-900"
            }`}
          >
            <button className="min-w-0 flex-1 text-left break-words" title={`Play ${s.name}`} onClick={() => onSelect(s.id)} aria-current={s.id === selected ? "true" : undefined}>
              <span className={s.id === playing ? "font-semibold text-violet-700 dark:text-violet-300" : ""}>{s.name}</span>
            </button>
            {lost.has(s.id) ? (
              <FileQuestion size={13} className="shrink-0 text-amber-600 dark:text-amber-400" aria-label="A file isn't where it was">
                <title>A file isn't where it was</title>
              </FileQuestion>
            ) : (
              s.audio && (
                <span title="Has music" className="shrink-0">
                  <Music size={13} className="text-neutral-400" aria-label="Has music" />
                </span>
              )
            )}
            {/* Always shown (touch screens have no hover), quiet until pointed at or focused. */}
            <span className="flex shrink-0 opacity-60 focus-within:opacity-100 hover:opacity-100">
              <button
                ref={keep(`${s.id}:up`)}
                type="button"
                className={ROW_BUTTON}
                aria-label={`Move ${s.name} up`}
                title="Play earlier in the playlist"
                disabled={i === 0}
                onClick={() => move(s, i - 1, "up")}
              >
                <ArrowUp size={12} aria-hidden />
              </button>
              <button
                ref={keep(`${s.id}:down`)}
                type="button"
                className={ROW_BUTTON}
                aria-label={`Move ${s.name} down`}
                title="Play later in the playlist"
                disabled={i === sequences.length - 1}
                onClick={() => move(s, i + 1, "down")}
              >
                <ArrowDown size={12} aria-hidden />
              </button>
              <button
                type="button"
                className={ROW_BUTTON}
                aria-label={`Remove ${s.name}`}
                title={`Take ${s.name} off the playlist (Undo brings it back)`}
                onClick={() => apply([{ type: "removeSequence", id: s.id }])}
              >
                <Trash2 size={12} aria-hidden />
              </button>
            </span>
          </div>
        </li>
      ))}
    </ol>
  );
}

const NO_SEQUENCES: SequenceEntry[] = [];
/** A row's small buttons: the drawer is narrow, and the name comes first. */
const ROW_BUTTON = "rounded p-1.5 hover:bg-neutral-200/70 disabled:cursor-not-allowed disabled:opacity-40 disabled:hover:bg-transparent dark:hover:bg-neutral-800";
