import { AudioLines, Download, FilePlus, FolderOpen, ListMusic, Magnet, Pause, Play, Save, Send, Square, X } from "lucide-react";
import { useEffect, useState } from "react";
import { errorMessage } from "../api/backend";
import { EffectPalette } from "../components/sequencer/EffectPalette";
import { EffectSettings } from "../components/sequencer/EffectSettings";
import { SequencePreview } from "../components/sequencer/SequencePreview";
import { AddRowMenu, Timeline } from "../components/sequencer/Timeline";
import { useSequenceKeys } from "../components/sequencer/useSequenceKeys";
import { Button, EmptyState, Input } from "../components/ui";
import { fileName } from "../lib/format";
import { formatTime } from "../lib/timelineMath";
import { useSequencer } from "../state/sequencer";
import { useApp } from "../state/store";

/** How often playback is checked while a sequence plays. */
const POLL_MS = 50;

/** The screen where a show is made: effects on props, timed to music. */
export function SequenceScreen() {
  const doc = useSequencer((s) => s.doc);
  const status = useSequencer((s) => s.status);
  const pollPlayback = useSequencer((s) => s.pollPlayback);
  const [creating, setCreating] = useState(false);
  const [confirm, setConfirm] = useState<null | (() => void)>(null);
  useSequenceKeys();

  useEffect(() => {
    if (!status) return;
    const timer = setInterval(() => void pollPlayback(), POLL_MS);
    return () => clearInterval(timer);
  }, [status !== null, pollPlayback]); // eslint-disable-line react-hooks/exhaustive-deps

  /** Runs `action` now, or after asking when the open sequence has unsaved changes. */
  const guard = (action: () => void) => {
    if (useSequencer.getState().dirty) setConfirm(() => action);
    else action();
  };
  const openFile = async (path?: string) => {
    const api = useSequencer.getState().api;
    const target = path ?? (await api?.pickSequenceDocPath());
    if (target) await useSequencer.getState().open(target);
  };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <Toolbar onNew={() => guard(() => setCreating(true))} onOpen={() => guard(() => void openFile())} />
      {doc ? <Workspace /> : <Start onNew={() => setCreating(true)} onOpen={openFile} />}
      {creating && <NewSequenceDialog onClose={() => setCreating(false)} />}
      {confirm && (
        <DiscardDialog
          onCancel={() => setConfirm(null)}
          onDiscard={() => {
            const action = confirm;
            setConfirm(null);
            action();
          }}
          onSave={async () => {
            const action = confirm;
            if (await useSequencer.getState().save()) {
              setConfirm(null);
              action();
            }
          }}
        />
      )}
    </div>
  );
}

function Workspace() {
  const doc = useSequencer((s) => s.doc)!;
  const show = useApp((s) => s.snapshot?.show);
  const [adding, setAdding] = useState(false);
  return (
    <div className="flex min-h-0 flex-1">
      <EffectPalette />
      <div className="flex min-w-0 flex-1 flex-col">
        <BeatsBanner />
        <div className="h-[34%] min-h-40 shrink-0 border-b border-neutral-200 p-2 dark:border-neutral-800">
          <SequencePreview doc={doc} />
        </div>
        {doc.rows.length === 0 ? (
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
      <EffectSettings doc={doc} />
    </div>
  );
}

function ToolButton({ label, onClick, disabled, children, pressed }: { label: string; onClick: () => void; disabled?: boolean; pressed?: boolean; children: React.ReactNode }) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
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

function Toolbar({ onNew, onOpen }: { onNew: () => void; onOpen: () => void }) {
  const s = useSequencer();
  const doc = s.doc;
  const playing = s.status?.state === "playing";
  return (
    <div role="toolbar" aria-label="Sequence" className="flex shrink-0 flex-wrap items-center gap-1 border-b border-neutral-200 px-2 py-1.5 dark:border-neutral-800">
      <ToolButton label="New sequence" onClick={onNew}>
        <FilePlus size={16} /> <span className="hidden xl:inline">New</span>
      </ToolButton>
      <ToolButton label="Open sequence" onClick={onOpen}>
        <FolderOpen size={16} /> <span className="hidden xl:inline">Open</span>
      </ToolButton>
      <ToolButton label="Save sequence" onClick={() => void s.save()} disabled={!doc}>
        <Save size={16} />
      </ToolButton>
      {doc && (
        <>
          <span className="mx-1 max-w-48 truncate font-medium" title={s.path ?? undefined}>
            {doc.name}
            {s.dirty && <span className="ml-1 text-xs text-neutral-500" aria-label="Unsaved changes">●</span>}
          </span>
          <span className="mx-1 h-5 w-px bg-neutral-200 dark:bg-neutral-800" />
          <ToolButton label={playing ? "Pause" : "Play"} onClick={() => void (playing ? s.pause() : s.play())}>
            {playing ? <Pause size={16} /> : <Play size={16} />}
          </ToolButton>
          <ToolButton label="Stop" onClick={() => void s.stop()} disabled={!s.status}>
            <Square size={15} />
          </ToolButton>
          <span className="w-36 text-sm text-neutral-600 tabular-nums dark:text-neutral-300" aria-label="Playhead">
            {formatTime(s.playheadMs)} <span className="text-neutral-400">/ {formatTime(doc.durationMs, 1000)}</span>
          </span>
          <span className="mx-1 h-5 w-px bg-neutral-200 dark:bg-neutral-800" />
          <ToolButton label={s.detecting ? "Finding the beats…" : "Detect beats"} onClick={() => void s.detectBeats()} disabled={!doc.audio || s.detecting}>
            <AudioLines size={16} /> <span className="hidden lg:inline">{s.detecting ? "Finding beats…" : "Detect beats"}</span>
          </ToolButton>
          <ToolButton label="Snap to beats and effect edges (hold Alt while dragging to turn off)" pressed={s.snapping} onClick={() => s.setSnapping(!s.snapping)}>
            <Magnet size={16} /> <span className="hidden lg:inline">Snap</span>
          </ToolButton>
          <ToolButton label="Send to controllers while playing" pressed={s.sendToControllers} onClick={() => void s.setSendToControllers(!s.sendToControllers)}>
            <Send size={16} /> <span className="hidden lg:inline">Send to controllers</span>
          </ToolButton>
          <div className="ml-auto flex items-center gap-1">
            {s.exporting !== null ? (
              <span className="flex items-center gap-2 text-sm" role="status">
                Exporting… {s.exporting}%
                <progress className="w-24 accent-violet-600" max={100} value={s.exporting} />
                <Button variant="ghost" onClick={() => void s.cancelExport()}>
                  Cancel
                </Button>
              </span>
            ) : (
              <>
                <ToolButton label="Export .fseq for FPP" onClick={() => void s.exportFseq(false)}>
                  <Download size={16} /> <span className="hidden lg:inline">Export .fseq…</span>
                </ToolButton>
                <ToolButton label="Export and add to the show's playlist" onClick={() => void exportToPlaylist()}>
                  <ListMusic size={16} /> <span className="hidden lg:inline">Add to show playlist…</span>
                </ToolButton>
              </>
            )}
          </div>
        </>
      )}
    </div>
  );
}

async function exportToPlaylist() {
  const summary = await useSequencer.getState().exportFseq(true);
  if (summary) useApp.setState({ error: null });
}

function BeatsBanner() {
  const { suggestBeats, detectBeats, dismissBeats, detecting } = useSequencer();
  if (!suggestBeats) return null;
  return (
    <div role="status" className="flex items-center gap-3 border-b border-violet-200 bg-violet-50 px-3 py-2 text-sm dark:border-violet-900 dark:bg-violet-950/30">
      <AudioLines size={16} className="shrink-0 text-violet-600 dark:text-violet-400" />
      <span className="flex-1">Find the beats and bars in this song? Effects then snap to them.</span>
      <Button variant="primary" disabled={detecting} onClick={() => void detectBeats()}>
        Detect beats
      </Button>
      <button type="button" aria-label="Not now" className="rounded p-1 hover:bg-violet-100 dark:hover:bg-violet-900" onClick={dismissBeats}>
        <X size={14} />
      </button>
    </div>
  );
}

function Start({ onNew, onOpen }: { onNew: () => void; onOpen: (path?: string) => Promise<void> }) {
  const recent = useSequencer((s) => s.recent);
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
        </div>
        {recent.length > 0 && (
          <section className="mt-6" aria-label="Recent sequences">
            <h2 className="text-sm font-semibold text-neutral-500">Recent</h2>
            <ul className="mt-2 flex flex-col">
              {recent.map((path) => (
                <li key={path}>
                  <button type="button" className="w-full truncate rounded px-2 py-1.5 text-left text-sm hover:bg-neutral-100 dark:hover:bg-neutral-800" title={path} onClick={() => void onOpen(path)}>
                    {fileName(path)}
                  </button>
                </li>
              ))}
            </ul>
          </section>
        )}
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
  const [music, setMusic] = useState<{ path: string; durationMs: number } | null>(null);
  const [name, setName] = useState("");
  const [seconds, setSeconds] = useState(60);
  const [reading, setReading] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);

  const chooseMusic = async () => {
    const path = await backend?.pickAudioPath();
    if (!path || !backend) return;
    setReading(true);
    setProblem(null);
    try {
      const waveform = await backend.audioWaveform(path, 100);
      setMusic({ path, durationMs: waveform.durationMs });
      if (!name) setName(fileName(path).replace(/\.[^.]+$/, ""));
    } catch (e) {
      setProblem(errorMessage(e));
    } finally {
      setReading(false);
    }
  };

  const create = async () => {
    const durationMs = music ? music.durationMs : Math.round(seconds * 1000);
    const ok = await useSequencer.getState().newSequence(name.trim() || "New sequence", durationMs, music?.path ?? null);
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
            <span className="truncate text-neutral-600 dark:text-neutral-300" title={music.path}>
              {fileName(music.path)} · {formatTime(music.durationMs, 1000)}
            </span>
          )}
        </div>
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

function DiscardDialog({ onSave, onDiscard, onCancel }: { onSave: () => void; onDiscard: () => void; onCancel: () => void }) {
  const name = useSequencer((s) => s.doc?.name ?? "this sequence");
  return (
    <Modal label="Unsaved changes">
      <h2 className="text-lg font-semibold">Save changes to {name}?</h2>
      <p className="mt-2 text-sm text-neutral-500">Your changes are lost if you don't save them.</p>
      <div className="mt-5 flex justify-end gap-2">
        <Button variant="ghost" onClick={onCancel}>
          Cancel
        </Button>
        <Button variant="danger" onClick={onDiscard}>
          Don't save
        </Button>
        <Button variant="primary" onClick={onSave}>
          Save
        </Button>
      </div>
    </Modal>
  );
}
