import { ArrowDown, ArrowUp, Music, Plus, Trash2 } from "lucide-react";
import type { SequenceEntry } from "../api/types";
import { useApp } from "../state/store";
import { Button } from "./ui";

/** The show's sequences in playlist order: pick one, add, reorder, or remove. */
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
  const backend = useApp((s) => s.backend);
  const run = useApp((s) => s.run);
  const apply = useApp((s) => s.apply);

  const add = async () => {
    const path = await backend?.pickSequencePath();
    if (!path) return;
    if (await run((b) => b.addSequence(path))) {
      const added = useApp.getState().snapshot?.show.sequences.at(-1);
      if (added) onSelect(added.id);
    }
  };

  return (
    <aside aria-label="Sequences" className="flex w-64 shrink-0 flex-col gap-2">
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-medium text-neutral-500">Sequences</h2>
        <Button onClick={add} aria-label="Add sequence">
          <Plus size={14} /> Add
        </Button>
      </div>
      {sequences.length === 0 ? (
        <p className="rounded-lg border border-dashed border-neutral-300 p-4 text-sm text-neutral-500 dark:border-neutral-700">
          Add a rendered sequence (.fseq). PixelFlow finds its music next to it.
        </p>
      ) : (
        <ol className="flex flex-col gap-1">
          {sequences.map((s: SequenceEntry, i) => (
            <li key={s.id}>
              <div
                className={`group flex items-center gap-2 rounded-md px-2 py-1.5 text-sm ${
                  s.id === selected ? "bg-violet-100 dark:bg-violet-950/50" : "hover:bg-neutral-100 dark:hover:bg-neutral-900"
                }`}
              >
                <button className="min-w-0 flex-1 truncate text-left" onClick={() => onSelect(s.id)} aria-current={s.id === selected}>
                  <span className={s.id === playing ? "font-semibold text-violet-700 dark:text-violet-300" : ""}>{s.name}</span>
                </button>
                {s.audio && <Music size={13} className="shrink-0 text-neutral-400" aria-label="Has music" />}
                <span className="hidden shrink-0 gap-0.5 group-hover:flex group-focus-within:flex">
                  <Button
                    variant="ghost"
                    aria-label={`Move ${s.name} up`}
                    disabled={i === 0}
                    onClick={() => apply([{ type: "moveSequence", id: s.id, index: i - 1 }])}
                  >
                    <ArrowUp size={12} />
                  </Button>
                  <Button
                    variant="ghost"
                    aria-label={`Move ${s.name} down`}
                    disabled={i === sequences.length - 1}
                    onClick={() => apply([{ type: "moveSequence", id: s.id, index: i + 1 }])}
                  >
                    <ArrowDown size={12} />
                  </Button>
                  <Button variant="ghost" aria-label={`Remove ${s.name}`} onClick={() => apply([{ type: "removeSequence", id: s.id }])}>
                    <Trash2 size={12} />
                  </Button>
                </span>
              </div>
            </li>
          ))}
        </ol>
      )}
    </aside>
  );
}

const NO_SEQUENCES: SequenceEntry[] = [];
