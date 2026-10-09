import { AlertTriangle, AudioLines, CheckCircle2, Download, FileInput, FilePlus, FolderOpen, History, Info, Lightbulb, ListMusic, ListPlus, Magnet, MoreHorizontal, Pause, Play, Repeat, Save, Send, Square, X } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import { errorMessage } from "../api/backend";
import type { AudioProgress } from "../api/types";
import { MissingFileNotice, useMissingBannerNames } from "../components/MissingFiles";
import { ProgressBar } from "../components/ProgressBar";
import { SendToFppDialog } from "../components/SendToFppDialog";
import { EffectPalette } from "../components/sequencer/EffectPalette";
import { EffectSettings } from "../components/sequencer/EffectSettings";
import { FindLyrics, LyricsSource } from "../components/sequencer/FindLyrics";
import { SequencePreview } from "../components/sequencer/SequencePreview";
import { AddTimingTrackDialog } from "../components/sequencer/TimingDialogs";
import { AddRowMenu, Timeline } from "../components/sequencer/Timeline";
import { useSequenceKeys } from "../components/sequencer/useSequenceKeys";
import { Button, EmptyState, Input, UnsavedBadge } from "../components/ui";
import { ago, fileName, shownPath } from "../lib/format";
import { formatTime } from "../lib/timelineMath";
import { MAX_ROWS, type Sequence, rowsForShow } from "../api/sequence";
import { sequenceArrangement, sidePreview } from "../lib/sequenceLayout";
import { useElementWidth } from "../lib/useWidth";
import { type RecentSequence, recentFor, useSequencer } from "../state/sequencer";
import { saveSequenceAndShow } from "../state/saveAll";
import { ariaKeysFor, comboLabel, hintFor } from "../lib/shortcuts";
import { useApp } from "../state/store";
import { useAudioProgress } from "../state/audioProgress";

/** How often playback is checked while a sequence plays. */
const POLL_MS = 50;

/** The preview's size, remembered on this computer. */
const PANE_KEY = "pixelflow.sequencePreview";
/** The preview's share of the column until it's resized, and when made bigger. */
const DEFAULT_SHARE = 0.34;
const BIG_SHARE = 0.75;
/** The least room the preview and the timeline each keep. */
const MIN_PREVIEW_PX = 120;
const MIN_TIMELINE_PX = 240;
/** How far an arrow key moves the divider. */
const STEP_PX = 20;

/** Beside the timeline, the preview's column starts this wide, and keeps between these. */
const SIDE_PX = 360;
const MIN_SIDE_PX = 280;
const BIG_SIDE = "55%";

interface PaneSize {
  /** The preview's height (px) above the timeline, or null for its share of the column. */
  height: number | null;
  /** Made bigger: the preview takes most of the room. */
  big: boolean;
  /** Where the preview goes when there's room beside the timeline (null: beside it). */
  place?: "side" | "top" | null;
  /** The preview column's width beside the timeline (px), or null for the default. */
  side?: number | null;
}

function loadPane(): PaneSize {
  try {
    const saved = JSON.parse(localStorage.getItem(PANE_KEY) ?? "{}") as Record<string, unknown>;
    const number = (v: unknown) => (typeof v === "number" && Number.isFinite(v) ? v : null);
    const place = saved.place === "side" || saved.place === "top" ? saved.place : null;
    return { height: number(saved.height), big: saved.big === true, place, side: number(saved.side) };
  } catch {
    return { height: null, big: false };
  }
}

function savePane(pane: PaneSize) {
  try {
    localStorage.setItem(PANE_KEY, JSON.stringify(pane));
  } catch {
    // Storage unavailable: the size still applies until the screen closes.
  }
}

/** A preview height that leaves both the preview and the timeline usable in a column `total` px tall. */
function fitPreview(height: number, total: number): number {
  return Math.round(Math.max(MIN_PREVIEW_PX, Math.min(Math.max(MIN_PREVIEW_PX, total - MIN_TIMELINE_PX), height)));
}

/** The screen where a show is made: effects on props, timed to music. */
export function SequenceScreen() {
  const doc = useSequencer((s) => s.doc);
  const status = useSequencer((s) => s.status);
  const pollPlayback = useSequencer((s) => s.pollPlayback);
  const showRevision = useApp((s) => s.snapshot?.revision);
  const [creating, setCreating] = useState(false);
  useSequenceKeys();

  // The sequence's problems depend on the show (props removed or added): check again when the
  // show changes, and when the screen opens.
  useEffect(() => {
    void useSequencer.getState().refreshIssues();
  }, [showRevision]);

  useEffect(() => {
    if (!status) return;
    const timer = setInterval(() => void pollPlayback(), POLL_MS);
    return () => clearInterval(timer);
  }, [status !== null, pollPlayback]); // eslint-disable-line react-hooks/exhaustive-deps

  /** Runs `action` now, or after asking when the open sequence has unsaved changes. */
  const guard = (action: () => void) => void useSequencer.getState().replaceAfterAsking(action);
  const openFile = async (path?: string) => {
    const api = useSequencer.getState().api;
    const target = path ?? (await api?.pickSequenceDocPath());
    if (target) await useSequencer.getState().open(target);
  };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <Toolbar onNew={() => guard(() => setCreating(true))} onOpen={() => guard(() => void openFile())} />
      <NoticeLine />
      {doc && <MissingMusicLine />}
      <RecoveryOffer onRecover={(id) => guard(() => void useSequencer.getState().recover(id))} />
      {doc ? <Workspace /> : <Start onNew={() => setCreating(true)} onOpen={openFile} />}
      {creating && <NewSequenceDialog onClose={() => setCreating(false)} />}
    </div>
  );
}

function Workspace() {
  const doc = useSequencer((s) => s.doc)!;
  const show = useApp((s) => s.snapshot?.show);
  const [adding, setAdding] = useState(false);
  const column = useRef<HTMLDivElement>(null);
  const workspace = useRef<HTMLDivElement>(null);
  const arrangement = useElementWidth(workspace, (w) => ({ ...sequenceArrangement(w), side: sidePreview(w) }));
  const [pane, setPane] = useState(loadPane);
  const paneRef = useRef<HTMLDivElement>(null);
  /** The column's height, for the divider's range (kept up to date as the window changes). */
  const [columnHeight, setColumnHeight] = useState(0);
  useLayoutEffect(() => {
    const el = column.current;
    if (!el) return;
    const measure = () => setColumnHeight(Math.round(el.getBoundingClientRect().height));
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, []);
  /** A divider drag: where it started, from what height, and the size to go back to if it's called off. */
  const resizing = useRef<{ startY: number; from: number; before: PaneSize } | null>(null);
  const update = (next: PaneSize) => {
    setPane(next);
    savePane(next);
  };
  const total = () => column.current?.getBoundingClientRect().height ?? 0;
  /** The preview's height now, in pixels. */
  const shown = () => (pane.big ? Math.round(total() * BIG_SHARE) : (pane.height ?? Math.round(total() * DEFAULT_SHARE)));
  const resizedTo = (e: { clientY: number }) => {
    const r = resizing.current;
    return r ? fitPreview(r.from + e.clientY - r.startY, total()) : null;
  };
  const side = arrangement.side !== null && pane.place !== "top";
  const onPlace = arrangement.side ? (place: "side" | "top") => update({ ...pane, place }) : undefined;
  const palette = side ? arrangement.side!.palette : arrangement.palette;
  return (
    <div ref={workspace} data-sequence-workspace data-preview={side ? "side" : "top"} className="relative flex min-h-0 flex-1">
      <EffectPalette compact={palette === "icons"} />
      <div ref={column} className="flex min-w-0 flex-1 flex-col">
        <BeatsBanner />
        {!side && (
          <>
            <div ref={paneRef} className="min-h-30 shrink px-2 pt-2 pb-1" style={{ height: pane.big ? `${BIG_SHARE * 100}%` : pane.height !== null ? `${pane.height}px` : `${DEFAULT_SHARE * 100}%` }}>
              <SequencePreview doc={doc} expanded={pane.big} onExpand={(big) => update({ ...pane, big })} place="top" onPlace={onPlace} />
            </div>
            {/* Drag (or use the arrow keys) to share the room between the preview and the timeline;
                a double-click puts it back. */}
            <div
              role="separator"
              aria-orientation="horizontal"
              aria-label="Preview size"
              aria-valuemin={MIN_PREVIEW_PX}
              aria-valuemax={Math.max(MIN_PREVIEW_PX, columnHeight - MIN_TIMELINE_PX)}
              aria-valuenow={shown()}
              tabIndex={0}
              title="Drag to resize the preview (double-click to reset)"
              className="h-1.5 shrink-0 cursor-row-resize touch-none border-b border-neutral-200 outline-none hover:bg-accent-400/40 focus-visible:bg-accent-400/40 dark:border-neutral-800"
              onPointerDown={(e) => {
                if (e.button !== 0) return;
                e.currentTarget.setPointerCapture?.(e.pointerId);
                // Bigger, the pane may be smaller than its share (the timeline keeps its room): start
                // from the height it really has.
                const from = pane.big ? Math.round(paneRef.current?.getBoundingClientRect().height ?? shown()) : shown();
                resizing.current = { startY: e.clientY, from, before: pane };
              }}
              onPointerMove={(e) => {
                const height = resizedTo(e);
                if (height !== null) setPane({ ...pane, height, big: false });
              }}
              onPointerUp={(e) => {
                const height = resizedTo(e);
                resizing.current = null;
                if (height !== null) update({ ...pane, height, big: false });
              }}
              onPointerCancel={() => {
                const r = resizing.current;
                resizing.current = null;
                if (r) setPane(r.before);
              }}
              onDoubleClick={() => update({ ...pane, height: null, big: false })}
              onKeyDown={(e) => {
                if (e.key !== "ArrowUp" && e.key !== "ArrowDown") return;
                // The arrows move the divider here, not the selected row.
                e.preventDefault();
                e.stopPropagation();
                update({ ...pane, height: fitPreview(shown() + (e.key === "ArrowDown" ? STEP_PX : -STEP_PX), total()), big: false });
              }}
            />
          </>
        )}
        {doc.rows.length === 0 && doc.timingTracks.length === 0 ? (
          <div className="relative flex-1 p-6">
            <EmptyState title="Add rows for your props">
              <p>Each row lights one prop or a group of props. Drag effects onto a row to make it light up.</p>
              <div className="mt-3">
                <Button variant="primary" onClick={() => setAdding(true)}>
                  Add a row
                </Button>
              </div>
            </EmptyState>
            {adding && <AddRowMenu doc={doc} show={show} onClose={() => setAdding(false)} />}
          </div>
        ) : (
          <Timeline doc={doc} />
        )}
      </div>
      {side ? (
        <PreviewColumn doc={doc} pane={pane} update={update} onPlace={onPlace!} />
      ) : (
        <EffectSettings doc={doc} placement={arrangement.settings === "floating" ? "floating" : "docked"} />
      )}
    </div>
  );
}

/**
 * The preview beside the timeline, sized to the display, with the effect settings under it: the
 * timeline gets the screen's full height. Its width is set from its edge (and remembered).
 */
function PreviewColumn({ doc, pane, update, onPlace }: { doc: Sequence; pane: PaneSize; update: (p: PaneSize) => void; onPlace: (p: "side" | "top") => void }) {
  const [width, setWidth] = useState(pane.side ?? SIDE_PX);
  const resizing = useRef<{ startX: number; from: number } | null>(null);
  const widthNow = (next: number) => Math.round(Math.max(MIN_SIDE_PX, next));
  const keep = (w: number) => update({ ...pane, side: w, big: false });
  return (
    <>
      <div
        role="separator"
        aria-orientation="vertical"
        aria-label="Preview width"
        aria-valuemin={MIN_SIDE_PX}
        aria-valuenow={width}
        tabIndex={0}
        title="Drag to resize the preview (double-click to reset)"
        className="w-1.5 shrink-0 cursor-col-resize touch-none border-l border-neutral-200 outline-none hover:bg-accent-400/40 focus-visible:bg-accent-400/40 dark:border-neutral-800"
        onPointerDown={(e) => {
          if (e.button !== 0) return;
          e.currentTarget.setPointerCapture?.(e.pointerId);
          resizing.current = { startX: e.clientX, from: width };
        }}
        onPointerMove={(e) => {
          const r = resizing.current;
          if (r) setWidth(widthNow(r.from - (e.clientX - r.startX)));
        }}
        onPointerUp={(e) => {
          const r = resizing.current;
          resizing.current = null;
          if (r) keep(widthNow(r.from - (e.clientX - r.startX)));
        }}
        onPointerCancel={() => {
          resizing.current = null;
          setWidth(pane.side ?? SIDE_PX);
        }}
        onDoubleClick={() => {
          setWidth(SIDE_PX);
          keep(SIDE_PX);
        }}
        onKeyDown={(e) => {
          if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
          // The arrows move the edge here, not the playhead. Left widens the preview.
          e.preventDefault();
          e.stopPropagation();
          const next = widthNow(width + (e.key === "ArrowLeft" ? STEP_PX : -STEP_PX));
          setWidth(next);
          keep(next);
        }}
      />
      <section
        aria-label="Preview and effect settings"
        style={{ width: pane.big ? BIG_SIDE : `${width}px`, minWidth: MIN_SIDE_PX, maxWidth: "calc(100% - 40rem)" }}
        className="flex shrink-0 flex-col"
      >
        <div className="shrink-0 px-2 pt-2 pb-2">
          <SequencePreview doc={doc} expanded={pane.big} onExpand={(big) => update({ ...pane, big })} place="side" onPlace={onPlace} />
        </div>
        <EffectSettings doc={doc} placement="stacked" />
      </section>
    </>
  );
}

function ToolButton({
  label,
  shortcut,
  hint,
  onClick,
  disabled,
  children,
  pressed,
}: {
  label: string;
  /** Said on hover instead of the label. */
  hint?: string;
  /** The key that does the same, shown in the tooltip. */
  shortcut?: string;
  onClick: () => void;
  disabled?: boolean;
  pressed?: boolean;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={hint ?? (shortcut ? `${label} (${comboLabel(shortcut.split(" ")[0])})` : label)}
      aria-keyshortcuts={shortcut}
      aria-pressed={pressed}
      onClick={onClick}
      disabled={disabled}
      className={`inline-flex items-center gap-1.5 rounded-md px-2 py-1.5 text-sm text-neutral-700 hover:bg-neutral-200/70 disabled:opacity-40 disabled:hover:bg-transparent dark:text-neutral-200 dark:hover:bg-neutral-800 ${
        pressed ? "bg-accent-50 text-accent-600 dark:bg-accent-600/15 dark:text-accent-400" : ""
      }`}
    >
      {children}
    </button>
  );
}

/** How far Detect beats has got: a hairline under its button (its own component: it changes
 * several times a second). */
function BeatsProgress() {
  const progress = useAudioProgress("beats");
  return <ProgressBar slim label="Finding the beats" fraction={progress?.fraction ?? null} className="absolute inset-x-2 bottom-0" />;
}

/** "Alt" in tooltips, or "Option" on a Mac keyboard. */
const ALT_KEY = typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.userAgent) ? "Option" : "Alt";

function Toolbar({ onNew, onOpen }: { onNew: () => void; onOpen: () => void }) {
  // Only what the buttons show: the playhead and export progress change many times a second and
  // have components of their own.
  const s = useSequencer(
    useShallow((st) => ({
      name: st.doc?.name ?? null,
      durationMs: st.doc?.durationMs ?? 0,
      hasMusic: Boolean(st.doc?.audio),
      path: st.path,
      dirty: st.dirty,
      playing: st.status?.state === "playing",
      active: st.status !== null,
      atStart: st.playheadMs === 0,
      looping: st.looping,
      detecting: st.detecting,
      snapping: st.snapping,
      sendToControllers: st.sendToControllers,
    })),
  );
  const act = useSequencer.getState;
  const [addingTrack, setAddingTrack] = useState(false);
  return (
    // Labels give way by the bar's own width, least needed first: New, Open and Import (named on
    // hover), then the editing tools.
    <div role="toolbar" aria-label="Sequence" className="@container flex shrink-0 flex-wrap items-center gap-1 border-b border-neutral-200 px-2 py-1.5 dark:border-neutral-800">
      <ToolButton label="New sequence" onClick={onNew}>
        <FilePlus size={16} /> <span className="hidden @min-[1200px]:inline">New sequence</span>
      </ToolButton>
      <ToolButton label="Open sequence" onClick={onOpen}>
        <FolderOpen size={16} /> <span className="hidden @min-[1200px]:inline">Open sequence</span>
      </ToolButton>
      {/* Asks about unsaved changes like New and Open do (the import goes through the same question). */}
      <ToolButton
        label="Import from xLights…"
        hint="Import an xLights sequence (.xsq), or a vendor's package (.zip), onto this show's props and groups"
        onClick={() => void useApp.getState().importXlightsSequence()}
      >
        <FileInput size={16} /> <span className="hidden @min-[1200px]:inline">Import from xLights</span>
      </ToolButton>
      <ToolButton label="Save" hint={`Save the sequence, and the show if it changed (${hintFor("save")})`} onClick={() => void saveSequenceAndShow()} disabled={s.name === null}>
        <Save size={16} />
      </ToolButton>
      {s.name !== null && (
        <>
          <span className="mx-1 max-w-48 truncate font-medium" title={s.path ? shownPath(s.path) : undefined}>
            {s.name}
          </span>
          {s.dirty && <UnsavedBadge doc="sequence" />}
          <span className="mx-1 h-5 w-px bg-neutral-200 dark:bg-neutral-800" />
          <ToolButton label={s.playing ? "Pause" : "Play"} shortcut={ariaKeysFor("seq-play")} onClick={() => void (s.playing ? act().pause() : act().play())}>
            {s.playing ? <Pause size={16} /> : <Play size={16} />}
          </ToolButton>
          {/* Stop leaves the playhead where it is; pressed again, it goes back to the start. */}
          <ToolButton label={s.active || s.atStart ? "Stop" : "Back to the start"} onClick={() => void act().stop()} disabled={!s.active && s.atStart}>
            <Square size={15} />
          </ToolButton>
          <ToolButton label="Loop playback" shortcut={ariaKeysFor("seq-loop")} pressed={s.looping} onClick={() => act().setLooping(!s.looping)}>
            <Repeat size={16} />
          </ToolButton>
          <PlayheadTime durationMs={s.durationMs} />
          <span className="mx-1 h-5 w-px bg-neutral-200 dark:bg-neutral-800" />
          <span className="relative inline-flex">
            <ToolButton
              label={s.detecting ? "Finding the beats…" : "Detect beats"}
              hint="Find the song's beats, bars, sections, accents, moments, and drums as timing tracks. Sections, Accents, and Moments you already have stay as they are."
              onClick={() => void act().detectBeats()}
              disabled={!s.hasMusic || s.detecting}
            >
              <AudioLines size={16} /> <span className="hidden @min-[760px]:inline">{s.detecting ? "Finding beats…" : "Detect beats"}</span>
            </ToolButton>
            {s.detecting && <BeatsProgress />}
          </span>
          <FindLyrics hasMusic={s.hasMusic} />
          <ToolButton label="Add timing track" onClick={() => setAddingTrack(true)}>
            <ListPlus size={16} /> <span className="hidden @min-[760px]:inline">Add timing track</span>
          </ToolButton>
          {addingTrack && <AddTrack onClose={() => setAddingTrack(false)} />}
          <ToolButton label={`Snap to beats and effect edges (hold ${ALT_KEY} while dragging to turn off)`} pressed={s.snapping} onClick={() => act().setSnapping(!s.snapping)}>
            <Magnet size={16} /> <span className="hidden @min-[760px]:inline">Snap</span>
          </ToolButton>
          <ToolButton
            label="Show it on my lights while editing"
            hint="When on, playing here also sends each frame to your controllers live, so your real lights show the sequence as you edit. When off, it plays only in the preview."
            pressed={s.sendToControllers}
            onClick={() => void act().setSendToControllers(!s.sendToControllers)}
          >
            <Lightbulb size={16} /> <span className="hidden lg:inline">Show on my lights</span>
          </ToolButton>
          <SequenceIssues />
          <ExportControls />
        </>
      )}
    </div>
  );
}

/** The Add timing track dialog, for the open sequence. */
function AddTrack({ onClose }: { onClose: () => void }) {
  const doc = useSequencer((s) => s.doc);
  return doc ? <AddTimingTrackDialog doc={doc} onClose={onClose} /> : null;
}

/** Where the playhead is (its own component: it changes many times a second while playing). */
function PlayheadTime({ durationMs }: { durationMs: number }) {
  const playheadMs = useSequencer((s) => s.playheadMs);
  return (
    <span className="w-36 text-sm text-neutral-600 tabular-nums dark:text-neutral-300">
      <span className="sr-only">Playhead at </span>
      {formatTime(playheadMs)} <span className="text-neutral-500">/ {formatTime(durationMs, 1000)}</span>
    </span>
  );
}

/** Send to FPP (with more ways to export behind a menu), or an export's progress with Cancel. */
function ExportControls() {
  const exporting = useSequencer((s) => s.exporting);
  const doc = useSequencer((s) => s.doc);
  const path = useSequencer((s) => s.path);
  const [sending, setSending] = useState(false);
  const openSend = async () => {
    // Send what's on screen: every edit made so far lands first.
    await useSequencer.getState().settled();
    setSending(true);
  };
  const name = path ? fileName(path).replace(/\.pfseq\.json$|\.json$/i, "") : (doc?.name ?? "Sequence");
  return (
    <div className="ml-auto flex items-center gap-1">
      {exporting !== null ? (
        <span className="flex items-center gap-2 text-sm" role="status">
          Exporting… {exporting}%
          <progress className="w-24 accent-violet-600" max={100} value={exporting} />
          <Button variant="ghost" onClick={() => void useSequencer.getState().cancelExport()}>
            Cancel
          </Button>
        </span>
      ) : (
        <>
          <ToolButton label="Send to FPP…" hint="Put this sequence and its music on your FPP, so it plays there on its own" onClick={() => void openSend()}>
            <Send size={16} /> <span className="hidden lg:inline">Send to FPP…</span>
          </ToolButton>
          <ExportMenu />
        </>
      )}
      {sending && doc && (
        <SendToFppDialog source={{ kind: "openSequence", name }} title={doc.name} music={doc.audio} onClose={() => setSending(false)} />
      )}
    </div>
  );
}

/** Exporting the .fseq file yourself, for people who want the file. */
function ExportMenu() {
  const [open, setOpen] = useState(false);
  const trigger = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    menu.current?.querySelector("button")?.focus();
    const close = (refocus: boolean) => {
      setOpen(false);
      if (refocus) trigger.current?.focus();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        close(true);
        return;
      }
      // Up and Down move between the items, round the ends.
      if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
      const items = [...(menu.current?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]') ?? [])];
      if (items.length === 0) return;
      e.preventDefault();
      const at = items.indexOf(document.activeElement as HTMLButtonElement);
      const step = e.key === "ArrowDown" ? 1 : -1;
      items[(at + step + items.length) % items.length].focus();
    };
    const onPointer = (e: PointerEvent) => {
      if (!menu.current?.contains(e.target as Node) && !trigger.current?.contains(e.target as Node)) close(false);
    };
    window.addEventListener("keydown", onKey);
    window.addEventListener("pointerdown", onPointer);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("pointerdown", onPointer);
    };
  }, [open]);
  const choose = (action: () => void) => {
    setOpen(false);
    action();
  };
  const item = "flex w-full items-center gap-2 rounded px-2 py-1.5 text-left hover:bg-neutral-100 focus:bg-neutral-100 focus:outline-none dark:hover:bg-neutral-800 dark:focus:bg-neutral-800";
  return (
    <span className="relative">
      <button
        ref={trigger}
        type="button"
        aria-label="More ways to export"
        title="More ways to export"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
        className="inline-flex items-center rounded-md px-1.5 py-1.5 text-neutral-700 hover:bg-neutral-200/70 dark:text-neutral-200 dark:hover:bg-neutral-800"
      >
        <MoreHorizontal size={16} />
      </button>
      {open && (
        <div
          ref={menu}
          role="menu"
          aria-label="More ways to export"
          className="absolute top-9 right-0 z-30 w-72 rounded-lg border border-neutral-200 bg-white p-1 text-sm shadow-xl dark:border-neutral-800 dark:bg-neutral-900"
        >
          <button type="button" role="menuitem" className={item} onClick={() => choose(() => void useSequencer.getState().exportFseq(false))}>
            <Download size={14} /> Export .fseq…
          </button>
          <button type="button" role="menuitem" className={item} onClick={() => choose(() => void exportToPlaylist())}>
            <ListMusic size={14} /> Export and add to this show's playlist…
          </button>
        </div>
      )}
    </span>
  );
}

/** Problems the engine found in the sequence (overlapping effects, missing props); clicking one
 * selects its effect. The list takes the focus and closes with Escape. */
function SequenceIssues() {
  const issues = useSequencer((s) => s.issues);
  const [open, setOpen] = useState(false);
  const trigger = useRef<HTMLButtonElement>(null);
  const list = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    list.current?.querySelector("button")?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      setOpen(false);
      trigger.current?.focus();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open]);
  if (issues.length === 0) return null;
  const errors = issues.filter((i) => i.severity === "error").length;
  return (
    <span className="relative">
      <button
        ref={trigger}
        type="button"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
        className={`flex items-center gap-1 rounded px-2 py-1 text-sm ${errors ? "text-red-600 dark:text-red-400" : "text-amber-600 dark:text-amber-400"}`}
      >
        <AlertTriangle size={14} /> {issues.length === 1 ? "1 problem" : `${issues.length} problems`}
      </button>
      {open && (
        <div
          ref={list}
          role="dialog"
          aria-label="Problems in this sequence"
          className="absolute top-9 left-0 z-30 max-h-80 w-[26rem] overflow-auto rounded-lg border border-neutral-200 bg-white p-3 text-sm shadow-xl dark:border-neutral-800 dark:bg-neutral-900"
        >
          <ul className="flex flex-col gap-2">
            {issues.map((issue, i) => (
              <li key={i}>
                <button
                  type="button"
                  className="text-left hover:underline"
                  onClick={() => {
                    const st = useSequencer.getState();
                    if (issue.effect) st.select([issue.effect], issue.row ?? null);
                    else if (issue.row) st.setActiveRow(issue.row);
                    setOpen(false);
                    st.reveal();
                  }}
                >
                  <span className={issue.severity === "error" ? "text-red-600 dark:text-red-400" : "text-amber-600 dark:text-amber-400"}>
                    {issue.severity === "error" ? "Error" : "Warning"}:
                  </span>{" "}
                  {issue.message}
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
    </span>
  );
}

/** What an export (or adding to the playlist) did, until dismissed. */
function NoticeLine() {
  const notice = useSequencer((s) => s.notice);
  const dismiss = useSequencer((s) => s.dismissNotice);
  const showDirty = useApp((s) => s.snapshot?.dirty ?? false);
  if (!notice) return null;
  const done = notice.tone === "done";
  return (
    <div
      role="status"
      className={`flex items-start gap-3 border-b px-3 py-2 text-sm ${
        done ? "border-emerald-200 bg-emerald-50 dark:border-emerald-900 dark:bg-emerald-950/30" : "border-neutral-200 bg-neutral-50 dark:border-neutral-800 dark:bg-neutral-900"
      }`}
    >
      {done ? (
        <CheckCircle2 size={16} className="mt-0.5 shrink-0 text-emerald-600 dark:text-emerald-400" aria-hidden />
      ) : (
        <Info size={16} className="mt-0.5 shrink-0 text-neutral-500" aria-hidden />
      )}
      <div className="min-w-0 flex-1">
        <p>{notice.text}</p>
        {notice.lyrics && <LyricsSource found={notice.lyrics} run={notice.lyricsRun} />}
        {notice.notes.length > 0 && (
          <ul className="mt-1 list-disc pl-5 text-xs text-neutral-600 dark:text-neutral-400">
            {notice.notes.map((note, i) => (
              <li key={i}>{note}</li>
            ))}
          </ul>
        )}
      </div>
      {notice.saveShow && showDirty && (
        <Button variant="primary" onClick={() => void useApp.getState().save()}>
          Save show
        </Button>
      )}
      <button type="button" aria-label="Dismiss" data-tip="Dismiss" className="rounded p-1 hover:bg-neutral-200/70 dark:hover:bg-neutral-800" onClick={dismiss}>
        <X size={14} />
      </button>
    </div>
  );
}

/** The sequence's music, when it isn't where the sequence says: find it again or locate it. */
function MissingMusicLine() {
  const audio = useSequencer((s) => s.doc?.audio ?? null);
  const path = useSequencer((s) => s.path);
  const docKey = useSequencer((s) => s.docKey);
  const missing = useSequencer((s) => s.musicMissing);
  const { checkMusic, findMusic, locateMusic } = useSequencer.getState();
  // One alarm at a time: while the show's banner names this file, it speaks for it.
  const bannerShown = useMissingBannerNames(missing?.name ?? null);
  useEffect(() => {
    void checkMusic();
  }, [audio, path, docKey, checkMusic]);
  if (!missing || bannerShown) return null;
  return (
    <div className="border-b border-amber-200 px-3 py-2 dark:border-amber-900/70">
      <MissingFileNotice missing={missing} onFind={() => void findMusic()} onLocate={() => void locateMusic()} />
    </div>
  );
}

/** Unsaved sequences PixelFlow kept when it last closed, to open again or throw away. */
function RecoveryOffer({ onRecover }: { onRecover: (id: string) => void }) {
  const recoveries = useSequencer((s) => s.recoveries);
  const discard = useSequencer((s) => s.discardRecovery);
  if (recoveries.length === 0) return null;
  return (
    <section aria-label="Unsaved sequences from last time" className="flex flex-col gap-2 border-b border-amber-200 bg-amber-50 px-3 py-2 text-sm dark:border-amber-900 dark:bg-amber-950/30">
      {recoveries.map((r) => (
        <div key={r.id} className="flex flex-wrap items-center gap-3">
          <History size={16} className="shrink-0 text-amber-600 dark:text-amber-400" aria-hidden />
          <span className="min-w-0 flex-1">
            PixelFlow kept unsaved changes to <strong>{r.name}</strong> from {ago(r.savedAtMs)}
            {r.path ? ` (${fileName(r.path)})` : " (never saved)"}.
          </span>
          <Button variant="primary" aria-label={`Recover unsaved sequence ${r.name}`} onClick={() => onRecover(r.id)}>
            Recover
          </Button>
          <Button variant="ghost" aria-label={`Discard unsaved sequence ${r.name}`} onClick={() => void discard(r.id)}>
            Discard
          </Button>
        </div>
      ))}
    </section>
  );
}

async function exportToPlaylist() {
  await useSequencer.getState().exportFseq(true);
}

function BeatsBanner() {
  const suggestBeats = useSequencer((s) => s.suggestBeats);
  const detecting = useSequencer((s) => s.detecting);
  const { detectBeats, dismissBeats } = useSequencer.getState();
  if (!suggestBeats) return null;
  return (
    <div role="status" className="flex items-center gap-3 border-b border-violet-200 bg-violet-50 px-3 py-2 text-sm dark:border-violet-900 dark:bg-violet-950/30">
      <AudioLines size={16} className="shrink-0 text-violet-600 dark:text-violet-400" />
      <span className="flex-1">Find the beats, bars, and sections in this song? Effects then snap to them.</span>
      <Button variant="primary" disabled={detecting} onClick={() => void detectBeats()}>
        Detect beats
      </Button>
      <button type="button" aria-label="Not now" data-tip="Not now" className="rounded p-1 hover:bg-violet-100 dark:hover:bg-violet-900" onClick={dismissBeats}>
        <X size={14} />
      </button>
    </div>
  );
}

function RecentSequences({ label, list, onOpen }: { label: string; list: RecentSequence[]; onOpen: (path: string) => Promise<void> }) {
  if (list.length === 0) return null;
  return (
    <section className="mt-6" aria-label={label}>
      <h2 className="text-sm font-semibold text-neutral-500">{label}</h2>
      <ul className="mt-2 flex flex-col">
        {list.map(({ path }) => (
          <li key={path}>
            <button type="button" className="w-full truncate rounded px-2 py-1.5 text-left text-sm hover:bg-neutral-100 dark:hover:bg-neutral-800" title={shownPath(path)} onClick={() => void onOpen(path)}>
              {fileName(path)}
            </button>
          </li>
        ))}
      </ul>
    </section>
  );
}

function Start({ onNew, onOpen }: { onNew: () => void; onOpen: (path?: string) => Promise<void> }) {
  const recent = useSequencer((s) => s.recent);
  const showPath = useApp((s) => s.snapshot?.path ?? null);
  // Sequences used with this show come first.
  const { mine, others } = recentFor(recent, showPath);
  return (
    <div className="flex flex-1 items-start justify-center overflow-auto p-10">
      <div className="w-full max-w-xl">
        <h1 className="text-xl font-semibold">Sequence</h1>
        <p className="mt-1 text-sm text-neutral-500">Make a show: put effects on your props, timed to a song.</p>
        <div className="mt-6 grid grid-cols-2 gap-3">
          <button type="button" onClick={onNew} className="flex flex-col items-start gap-1 rounded-lg border border-neutral-200 p-4 text-left hover:border-accent-500 dark:border-neutral-800">
            <FilePlus size={20} className="text-accent-600 dark:text-accent-400" />
            <span className="font-medium">New sequence</span>
            <span className="text-sm text-neutral-500">Start from a song.</span>
          </button>
          <button type="button" onClick={() => void onOpen()} className="flex flex-col items-start gap-1 rounded-lg border border-neutral-200 p-4 text-left hover:border-accent-500 dark:border-neutral-800">
            <FolderOpen size={20} className="text-accent-600 dark:text-accent-400" />
            <span className="font-medium">Open a sequence</span>
            <span className="text-sm text-neutral-500">A .pfseq.json file you saved.</span>
          </button>
          <button
            type="button"
            onClick={() => void useApp.getState().importXlightsSequence()}
            className="col-span-2 flex flex-col items-start gap-1 rounded-lg border border-neutral-200 p-4 text-left hover:border-accent-500 dark:border-neutral-800"
          >
            <FileInput size={20} className="text-accent-600 dark:text-accent-400" />
            <span className="font-medium">Import an xLights sequence</span>
            <span className="text-sm text-neutral-500">An .xsq file, or a vendor&apos;s .zip package mapped onto this show&apos;s props and groups.</span>
          </button>
        </div>
        <RecentSequences label="With this show" list={mine} onOpen={onOpen} />
        <RecentSequences label={mine.length > 0 ? "Other recent sequences" : "Recent sequences"} list={others} onOpen={onOpen} />
      </div>
    </div>
  );
}

function Modal({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="fixed inset-0 z-40 flex items-center justify-center bg-black/40">
      <div role="dialog" aria-modal="true" aria-label={label} className="w-[28rem] rounded-lg border border-neutral-200 bg-white p-5 shadow-xl dark:border-neutral-800 dark:bg-neutral-900">
        {children}
      </div>
    </div>
  );
}

/** Starts a sequence from a song (its length comes from the music), or a silent one of a set length. */
function NewSequenceDialog({ onClose }: { onClose: () => void }) {
  const backend = useApp((s) => s.backend);
  const show = useApp((s) => s.snapshot?.show);
  // Groups with no members light nothing, so they get no row.
  const rowCount = (show?.props.length ?? 0) + (show?.groups.filter((g) => g.members.length > 0).length ?? 0);
  const [everyRow, setEveryRow] = useState(true);
  const [music, setMusic] = useState<{ path: string; durationMs: number } | null>(null);
  const [name, setName] = useState("");
  const [seconds, setSeconds] = useState(60);
  const [reading, setReading] = useState(false);
  /** While a file that doesn't say how long it is is read through. */
  const [readProgress, setReadProgress] = useState<AudioProgress | null>(null);
  const [problem, setProblem] = useState<string | null>(null);

  const chooseMusic = async () => {
    const path = await backend?.pickAudioPath();
    if (!path || !backend) return;
    setReading(true);
    setProblem(null);
    try {
      // Its header says how long it is (quick); the waveform is drawn once the sequence opens.
      const info = await backend.probeAudio(path, (p) => setReadProgress(p.fraction >= 1 ? null : p));
      setMusic({ path, durationMs: info.durationMs });
      if (!name) setName(fileName(path).replace(/\.[^.]+$/, ""));
    } catch (e) {
      setProblem(errorMessage(e));
    } finally {
      setReading(false);
      setReadProgress(null);
    }
  };

  const create = async () => {
    const durationMs = music ? music.durationMs : Math.round(seconds * 1000);
    const latest = useApp.getState().snapshot?.show;
    const rows = everyRow && latest ? rowsForShow(latest) : [];
    const ok = await useSequencer.getState().newSequence(name.trim() || "New sequence", durationMs, music?.path ?? null, rows);
    if (ok) onClose();
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <Modal label="New sequence">
      <h2 className="text-lg font-semibold">New sequence</h2>
      <div className="mt-4 flex flex-col gap-3 text-sm">
        <div className="flex items-center gap-2">
          <Button onClick={() => void chooseMusic()} disabled={reading}>
            {reading ? "Reading the music…" : music ? "Choose other music…" : "Choose music…"}
          </Button>
          {music && (
            <span className="truncate text-neutral-600 dark:text-neutral-300" title={shownPath(music.path)}>
              {fileName(music.path)} · {formatTime(music.durationMs, 1000)}
            </span>
          )}
        </div>
        {reading && readProgress && <ProgressBar label={readProgress.stage} fraction={readProgress.fraction} />}
        {problem && <p className="text-red-600 dark:text-red-400">{problem}</p>}
        <label className="flex flex-col gap-1">
          <span className="text-neutral-600 dark:text-neutral-400">Name</span>
          <Input value={name} placeholder="New sequence" onChange={(e) => setName(e.target.value)} />
        </label>
        {!music && (
          <label className="flex flex-col gap-1">
            <span className="text-neutral-600 dark:text-neutral-400">Length without music (seconds)</span>
            <Input type="number" min={1} max={14_400} value={seconds} onChange={(e) => setSeconds(Math.max(1, Number(e.target.value) || 1))} />
          </label>
        )}
        <fieldset className="flex flex-col gap-1.5">
          <legend className="mb-1 text-neutral-600 dark:text-neutral-400">Rows</legend>
          <label className="flex items-start gap-2">
            <input type="radio" name="new-sequence-rows" checked={everyRow} onChange={() => setEveryRow(true)} className="mt-0.5 accent-accent-500" />
            <span>
              {rowCount > MAX_ROWS
                ? `A row for the first ${MAX_ROWS.toLocaleString("en-US")} props and groups (of ${rowCount.toLocaleString("en-US")})`
                : `A row for every prop and group (${rowCount.toLocaleString("en-US")})`}
              <span className="block text-xs text-neutral-500">In layout order, groups first, as xLights does. Remove the ones you don&apos;t need.
                {rowCount > MAX_ROWS && ` A sequence holds at most ${MAX_ROWS.toLocaleString("en-US")} rows; add the rest by hand where you need them.`}</span>
            </span>
          </label>
          <label className="flex items-start gap-2">
            <input type="radio" name="new-sequence-rows" checked={!everyRow} onChange={() => setEveryRow(false)} className="mt-0.5 accent-accent-500" />
            <span>
              Start empty
              <span className="block text-xs text-neutral-500">Add rows yourself as you go.</span>
            </span>
          </label>
        </fieldset>
        <p className="text-xs text-neutral-500">Frames are 25 ms apart (40 per second).</p>
      </div>
      <div className="mt-5 flex justify-end gap-2">
        <Button variant="ghost" onClick={onClose}>
          Cancel
        </Button>
        <Button variant="primary" onClick={() => void create()} disabled={reading}>
          Create
        </Button>
      </div>
    </Modal>
  );
}
