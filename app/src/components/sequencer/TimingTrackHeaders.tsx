// The timing tracks' names beside their strips above the rows: click one to pick it (T taps marks
// onto the picked track), double-click to rename, drag the grip to reorder, and a menu for
// everything else (generate marks, paste lyrics, break into words, nudge lyrics, re-time them to
// the vocals, tap timing, the vocals lane, import, export, delete).

import { GripVertical, MoreHorizontal } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { Sequence, TimingTrack } from "../../api/sequence";
import { canTapTime } from "../../lib/lyricEdits";
import { useLyricTools } from "../../state/lyricTools";
import { useSequencer } from "../../state/sequencer";
import { GenerateMarksDialog, NudgeLyricsDialog, PasteLyricsDialog, breakIntoWords, isLyricTrack, kindLabel } from "./TimingDialogs";
import { RULER_H, TRACK_H, WAVE_H } from "./drawTimeline";

type Open = { kind: "generate" | "lyrics" | "nudge"; track: string } | null;

export function TimingTrackHeaders({ doc }: { doc: Sequence }) {
  const activeTrack = useSequencer((s) => s.activeTrack);
  const [menu, setMenu] = useState<string | null>(null);
  const [renaming, setRenaming] = useState<string | null>(null);
  const [dialog, setDialog] = useState<Open>(null);
  const reorder = useRef<{ id: string; to: number } | null>(null);
  const [dragTo, setDragTo] = useState<number | null>(null);
  const opened = dialog ? doc.timingTracks.find((t) => t.id === dialog.track) : undefined;

  return (
    <>
      {doc.timingTracks.map((track, i) => {
        const active = activeTrack === track.id;
        return (
          <div
            key={track.id}
            role="group"
            aria-label={`Timing track ${track.name}`}
            aria-current={active || undefined}
            className={`group relative flex items-center gap-0.5 pr-0.5 text-neutral-500 ${active ? "bg-accent-50 text-accent-700 dark:bg-accent-600/15 dark:text-accent-300" : ""} ${
              dragTo === i ? "border-t-2 border-t-accent-500" : ""
            }`}
            style={{ height: TRACK_H }}
            onClick={() => useSequencer.getState().setActiveTrack(track.id)}
          >
            <span
              className="cursor-grab touch-none px-0.5 text-neutral-400"
              aria-hidden
              onPointerDown={(e) => {
                e.currentTarget.setPointerCapture?.(e.pointerId);
                reorder.current = { id: track.id, to: i };
              }}
              onPointerMove={(e) => {
                const r = reorder.current;
                if (!r) return;
                const band = e.currentTarget.parentElement!.parentElement!.getBoundingClientRect();
                const y = e.clientY - band.top - RULER_H - WAVE_H;
                r.to = Math.max(0, Math.min(doc.timingTracks.length - 1, Math.floor(y / TRACK_H)));
                setDragTo(r.to);
              }}
              onPointerUp={() => {
                const r = reorder.current;
                reorder.current = null;
                setDragTo(null);
                if (r && r.to !== i) void useSequencer.getState().edit([{ type: "moveTimingTrack", id: r.id, index: r.to }]);
              }}
            >
              <GripVertical size={11} />
            </span>
            {renaming === track.id ? (
              <RenameInput track={track} onDone={() => setRenaming(null)} />
            ) : (
              <span className="min-w-0 flex-1 truncate" title={`${track.name} (${kindLabel(track.kind)}) — double-click to rename`} onDoubleClick={() => setRenaming(track.id)}>
                {track.name}
              </span>
            )}
            {/* What it marks, unless its name already says so. */}
            {!track.name.toLowerCase().includes(kindLabel(track.kind).toLowerCase()) && (
              <span className="shrink-0 text-[10px] text-neutral-400">{kindLabel(track.kind)}</span>
            )}
            <button
              type="button"
              aria-label={`${track.name} menu`}
              aria-haspopup="menu"
              aria-expanded={menu === track.id}
              title="Track options: paste lyrics, make marks, import or export timing"
              className="relative shrink-0 rounded px-0.5 text-neutral-500 before:absolute before:-inset-x-1 before:-inset-y-1.5 hover:bg-neutral-200/70 hover:text-neutral-800 dark:hover:bg-neutral-800 dark:hover:text-neutral-200"
              onClick={(e) => {
                e.stopPropagation();
                setMenu(menu === track.id ? null : track.id);
              }}
            >
              <MoreHorizontal size={13} />
            </button>
            {menu === track.id && (
              <TrackMenu
                doc={doc}
                track={track}
                index={i}
                onClose={() => setMenu(null)}
                onRename={() => setRenaming(track.id)}
                onDialog={(kind) => setDialog({ kind, track: track.id })}
              />
            )}
          </div>
        );
      })}
      {dialog?.kind === "generate" && opened && <GenerateMarksDialog doc={doc} track={opened} onClose={() => setDialog(null)} />}
      {dialog?.kind === "lyrics" && opened && <PasteLyricsDialog doc={doc} track={opened} onClose={() => setDialog(null)} />}
      {dialog?.kind === "nudge" && opened && <NudgeLyricsDialog track={opened} onClose={() => setDialog(null)} />}
    </>
  );
}

function RenameInput({ track, onDone }: { track: TimingTrack; onDone: () => void }) {
  const [name, setName] = useState(track.name);
  const done = useRef(false);
  const commit = (save: boolean) => {
    if (done.current) return;
    done.current = true;
    onDone();
    if (save && name.trim() && name.trim() !== track.name) {
      void useSequencer.getState().edit([{ type: "renameTimingTrack", id: track.id, name }]);
    }
  };
  return (
    <input
      autoFocus
      aria-label={`Rename ${track.name}`}
      value={name}
      onChange={(e) => setName(e.target.value)}
      onFocus={(e) => e.currentTarget.select()}
      onBlur={() => commit(true)}
      onKeyDown={(e) => {
        if (e.key === "Enter") commit(true);
        if (e.key === "Escape") {
          e.preventDefault();
          e.stopPropagation();
          commit(false);
        }
      }}
      className="h-4 min-w-0 flex-1 rounded border border-accent-500 bg-white px-1 text-xs text-neutral-900 outline-none dark:bg-neutral-950 dark:text-neutral-100"
    />
  );
}

function TrackMenu({
  doc,
  track,
  index,
  onClose,
  onRename,
  onDialog,
}: {
  doc: Sequence;
  track: TimingTrack;
  index: number;
  onClose: () => void;
  onRename: () => void;
  onDialog: (kind: "generate" | "lyrics" | "nudge") => void;
}) {
  const box = useRef<HTMLDivElement>(null);
  const close = useRef(onClose);
  close.current = onClose;
  // The menu takes the focus; Escape or a click elsewhere closes it.
  useEffect(() => {
    box.current?.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        close.current();
      }
    };
    const onDown = (e: PointerEvent) => {
      if (!box.current?.contains(e.target as Node)) close.current();
    };
    window.addEventListener("keydown", onKey);
    window.addEventListener("pointerdown", onDown);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("pointerdown", onDown);
    };
  }, []);
  const store = useSequencer.getState();
  const editable = track.kind !== "phonemes";
  const lyric = isLyricTrack(track) && track.marks.length > 0;
  const retiming = useSequencer((s) => s.retimingLyrics);
  const vocalsShown = useLyricTools((s) => s.vocalsShown);
  const items: { label: string; run: () => void; hidden?: boolean; disabled?: boolean; danger?: boolean }[] = [
    { label: "Rename", run: onRename },
    { label: "Generate marks…", run: () => onDialog("generate"), hidden: !editable },
    { label: "Paste lyrics…", run: () => onDialog("lyrics"), hidden: !editable },
    { label: "Break into words", run: () => void breakIntoWords(track.id), hidden: track.kind !== "lyrics" },
    { label: "Break into syllables", run: () => void store.syllablesFromWords(track.id), hidden: track.kind !== "words" || track.marks.length === 0 },
    { label: "Nudge lyrics…", run: () => onDialog("nudge"), hidden: !lyric },
    { label: "Re-time to vocals", run: () => void store.retimeLyrics(track.id), hidden: !lyric || !doc.audio, disabled: retiming },
    { label: "Tap timing…", run: () => useLyricTools.getState().openTap(track.id), hidden: !canTapTime(track) || !doc.audio },
    {
      label: vocalsShown ? "Hide vocals lane" : "Show vocals lane",
      run: () => useLyricTools.getState().setVocalsShown(!vocalsShown),
      hidden: !isLyricTrack(track) || !doc.audio,
    },
    { label: "Move up", run: () => void store.edit([{ type: "moveTimingTrack", id: track.id, index: index - 1 }]), disabled: index === 0 },
    { label: "Move down", run: () => void store.edit([{ type: "moveTimingTrack", id: track.id, index: index + 1 }]), disabled: index === doc.timingTracks.length - 1 },
    { label: "Import timing file…", run: () => void store.importTiming() },
    { label: "Export…", run: () => void store.exportTiming(track.id) },
    { label: "Delete track", run: () => void store.edit([{ type: "removeTimingTrack", id: track.id }]), danger: true },
  ];
  return (
    <div
      ref={box}
      role="menu"
      aria-label={`${track.name} menu`}
      className="absolute top-full left-2 z-30 flex w-48 flex-col rounded-lg border border-neutral-200 bg-white p-1 text-sm text-neutral-800 shadow-xl dark:border-neutral-800 dark:bg-neutral-900 dark:text-neutral-100"
      onClick={(e) => e.stopPropagation()}
    >
      {items
        .filter((item) => !item.hidden)
        .map((item) => (
          <button
            key={item.label}
            type="button"
            role="menuitem"
            disabled={item.disabled}
            className={`rounded px-2 py-1 text-left hover:bg-neutral-100 disabled:opacity-40 dark:hover:bg-neutral-800 ${item.danger ? "text-red-600 dark:text-red-400" : ""}`}
            onClick={() => {
              onClose();
              item.run();
            }}
          >
            {item.label}
          </button>
        ))}
    </div>
  );
}
