import { AudioLines, X } from "lucide-react";
import type { ReactNode } from "react";
import type { MissingFile } from "../../api/types";
import { sequenceTitle, shownPath } from "../../lib/format";
import type { RecentSequence } from "../../state/sequencer";
import { MissingFileNotice } from "../MissingFiles";
import { AddSequenceButton, SequenceList } from "../SequenceList";
import { IconButton } from "../ui";
import { DrawerSection } from "./DrawerSection";

function Group({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div role="group" aria-label={label} className="flex flex-col gap-1">
      <h3 className="text-xs font-medium tracking-wide text-neutral-500 uppercase">{label}</h3>
      {children}
    </div>
  );
}

const ROW = "flex w-full min-w-0 items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm";

/**
 * The Play screen's side panel: what there is to play (the sequence open in the editor, the
 * sequences saved with this show, and the show's rendered ones), then `children`: the picked
 * sequence's music, and what's on the FPP. Picking anything plays it. `floating`: over the
 * preview's edge, in a window too narrow for both.
 */
export function PlaylistDrawer({
  floating,
  onClose,
  open,
  saved,
  onOpenSaved,
  selectedEntry,
  playingEntry,
  onPickEntry,
  onAdded,
  missingSequence,
  playAll,
  onPlayAll,
  children,
}: {
  floating: boolean;
  onClose: () => void;
  /** The sequence open in the editor: its name, whether it's the one picked, and how to pick it. */
  open: { name: string; picked: boolean; playing: boolean; pick: () => void } | null;
  /** Sequences saved lately with this show (other than the open one). */
  saved: RecentSequence[];
  onOpenSaved: (path: string) => void;
  selectedEntry: string | null;
  playingEntry: string | null;
  onPickEntry: (id: string) => void;
  onAdded: (id: string) => void;
  /** The picked rendered sequence's file, when it isn't where the show says. */
  missingSequence: MissingFile | undefined;
  playAll: boolean;
  onPlayAll: (on: boolean) => void;
  children?: ReactNode;
}) {
  return (
    <aside
      aria-label="Playlist"
      className={`flex w-[22rem] max-w-full shrink-0 flex-col overflow-y-auto border-l border-neutral-200 bg-white dark:border-neutral-800 dark:bg-neutral-950 ${
        floating ? "absolute inset-y-0 right-0 z-20 shadow-2xl" : ""
      }`}
    >
      <DrawerSection
        id="playlist"
        title="Playlist"
        actions={
          <>
            <AddSequenceButton onSelect={onAdded} />
            <IconButton label="Close the playlist" className="rounded-md p-1.5 text-neutral-500 hover:bg-neutral-200/70 dark:hover:bg-neutral-800" onClick={onClose}>
              <X size={14} aria-hidden />
            </IconButton>
          </>
        }
      >
        {open && (
          <Group label="Open in the editor">
            <button
              type="button"
              title={`Play ${open.name}`}
              aria-current={open.picked ? "true" : undefined}
              onClick={open.pick}
              className={`${ROW} ${open.picked ? "bg-violet-100 dark:bg-violet-950/50" : "hover:bg-neutral-100 dark:hover:bg-neutral-900"}`}
            >
              <AudioLines size={14} className="shrink-0 text-neutral-400" aria-hidden />
              <span className={`min-w-0 break-words ${open.playing ? "font-semibold text-violet-700 dark:text-violet-300" : ""}`}>{open.name}</span>
            </button>
          </Group>
        )}
        {saved.length > 0 && (
          <Group label="Saved with this show">
            <ul className="flex flex-col gap-0.5">
              {saved.map(({ path }) => (
                <li key={path}>
                  <button type="button" title={`Open ${shownPath(path)} and play it`} onClick={() => onOpenSaved(path)} className={`${ROW} hover:bg-neutral-100 dark:hover:bg-neutral-900`}>
                    <AudioLines size={14} className="shrink-0 text-neutral-400" aria-hidden />
                    <span className="min-w-0 break-words">{sequenceTitle(path)}</span>
                  </button>
                </li>
              ))}
            </ul>
          </Group>
        )}
        <Group label="Rendered (.fseq)">
          <SequenceList selected={selectedEntry} playing={playingEntry} onSelect={onPickEntry} />
          {missingSequence && <MissingFileNotice missing={missingSequence} />}
          <label className="flex items-center gap-2 text-sm text-neutral-600 dark:text-neutral-300" title="When a rendered sequence finishes, the next one starts; after the last, the first">
            <input type="checkbox" checked={playAll} onChange={(e) => onPlayAll(e.target.checked)} />
            Play all and repeat
          </label>
        </Group>
      </DrawerSection>
      {children}
    </aside>
  );
}
