// The preview's own window (see state/previewWindow.ts): the app's page opened with
// `?window=preview` shows this instead of the app. It only looks: it asks the engine for the show,
// its pixels, frames and where playback is, and reads the main window's note for what to show
// (the open sequence at its playhead, or the lights live) and how. What's chosen here (2D or 3D,
// glow) and play/pause go back in its own note, for the main window to take on.

import { Maximize, Minimize, Pause, Play } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import { appWindow } from "./api/appWindow";
import { type Backend, errorMessage } from "./api/backend";
import type { SequencerApi } from "./api/sequencer";
import type { PlaybackStatus } from "./api/types";
import { GlowControl } from "./components/layout3d/GlowControl";
import { ModeSwitch } from "./components/layout3d/ModeSwitch";
import { BarButton } from "./components/play/TransportBar";
import { PreviewStage } from "./components/preview/PreviewStage";
import { useSequenceFrame } from "./components/sequencer/useSequenceFrame";
import { TooltipLayer } from "./components/Tooltip";
import { readingFrom } from "./lib/avSync";
import { clock } from "./lib/format";
import { readViewOptions, sameView } from "./lib/viewOptions";
import { type PreviewFeed, type PreviewReply, readFeed } from "./state/previewWindow";
import { playClock, usePreviewSync } from "./state/previewSync";
import { useSequencer } from "./state/sequencer";
import { useApp } from "./state/store";
import { type LayoutMode, useView3d } from "./state/view3d";

/** How often the main window's note is read (and this window's left, when it changed). */
const LINK_MS = 40;
/** How often playback is asked after: while something is loaded, and while nothing is. */
const POLL_MS = 50;
const IDLE_MS = 250;
/** How often the lights' live colors are fetched. */
const FRAME_MS = 50;
/** In full screen, how long the bar stays up after the pointer or a key was last used. */
const HIDE_MS = 2500;

/**
 * Gives the preview's window its engine. Nothing else of the app starts here: no menus, no
 * dialogs, no listening for events, no questions about unsaved work.
 */
export async function connectPreviewWindow(backend: Backend, sequencer: SequencerApi) {
  useApp.setState({ backend });
  useSequencer.setState({ api: sequencer });
  try {
    useApp.setState({ snapshot: await backend.getSnapshot() });
  } catch (e) {
    useApp.setState({ error: errorMessage(e) });
  }
}

/** The open sequence at the main window's playhead, and live while it plays. */
function SequenceStage({ feed, mode, playing }: { feed: PreviewFeed; mode: LayoutMode; playing: boolean }) {
  const frame = useSequenceFrame(feed.durationMs, feed.stamp);
  // The same row from one note to the next is the same thing to draw.
  const key = JSON.stringify(feed.only);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const only = useMemo(() => feed.only, [key]);
  return <PreviewStage frame={frame} mode={mode} only={only} playing={playing} />;
}

/** The lights as the engine is sending them now (a rendered sequence playing, a test pattern). */
function LiveStage({ mode, playing }: { mode: LayoutMode; playing: boolean }) {
  const backend = useApp((s) => s.backend);
  const [frame, setFrame] = useState<Uint8Array | null>(null);
  useEffect(() => {
    if (!backend) return;
    let live = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const poll = () => {
      backend.liveFrame().then(
        (f) => {
          if (!live) return;
          setFrame(f.length ? f : null);
          timer = setTimeout(poll, f.length ? FRAME_MS : IDLE_MS);
        },
        () => {
          if (live) timer = setTimeout(poll, IDLE_MS);
        },
      );
    };
    poll();
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [backend]);
  return <PreviewStage frame={frame} mode={mode} playing={playing} label="Preview of the lights" />;
}

export function PreviewWindow() {
  const backend = useApp((s) => s.backend);
  const showName = useApp((s) => s.snapshot?.show.name ?? null);
  const glow = useView3d((s) => s.glow);
  const view = useView3d(useShallow((s) => readViewOptions(s)));
  const [feed, setFeed] = useState<PreviewFeed | null>(null);
  const [mode, setMode] = useState<LayoutMode>(() => useView3d.getState().playMode);
  const [toggles, setToggles] = useState(0);
  const [status, setStatus] = useState<PlaybackStatus | null>(null);
  const [full, setFull] = useState(false);
  const [awake, setAwake] = useState(true);

  // A night scene, whatever the app's theme.
  useEffect(() => {
    document.documentElement.dataset.theme = "dark";
    document.title = "PixelFlow Preview";
  }, []);

  // The two notes: the main window's is read, and this window's is left when it has changed.
  const mine = useRef<PreviewReply>({ mode, glow, view, toggles });
  mine.current = { mode, glow, view, toggles };
  useEffect(() => {
    if (!backend) return;
    let live = true;
    let busy = false;
    let sent = "";
    let got = "";
    let last: PreviewFeed | null = null;
    const turn = async () => {
      if (busy) return;
      busy = true;
      try {
        // Nothing is said until the main window's note has been read: what was chosen there comes first.
        const note = last ? JSON.stringify(mine.current) : null;
        const linked = await backend.previewLink("preview", note !== null && note !== sent ? mine.current : undefined);
        if (note !== null) sent = note;
        if (!live) return;
        const next = readFeed(linked.theirs);
        const text = JSON.stringify(next);
        if (!next || text === got) return;
        got = text;
        if (next.mode !== last?.mode) setMode(next.mode);
        if (next.glow !== last?.glow && next.glow !== useView3d.getState().glow) useView3d.setState({ glow: next.glow });
        if ((!last || !sameView(next.view, last.view)) && !sameView(next.view, readViewOptions(useView3d.getState()))) useView3d.setState(next.view);
        if (next.syncMs !== last?.syncMs) usePreviewSync.setState({ offsetMs: next.syncMs });
        last = next;
        setFeed(next);
      } catch {
        // The next turn tries again.
      } finally {
        busy = false;
      }
    };
    void turn();
    const timer = setInterval(() => void turn(), LINK_MS);
    return () => {
      live = false;
      clearInterval(timer);
    };
  }, [backend]);

  // The show, again whenever the main window says it changed.
  const showRevision = feed?.showRevision;
  useEffect(() => {
    if (!backend || showRevision === undefined) return;
    let live = true;
    backend.getSnapshot().then(
      (snapshot) => live && useApp.setState({ snapshot }),
      () => undefined,
    );
    return () => {
      live = false;
    };
  }, [backend, showRevision]);

  // Where playback is, from the engine: the open sequence follows its clock while it plays.
  useEffect(() => {
    if (!backend) return;
    let live = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const poll = async () => {
      const sentAt = performance.now();
      const next = await backend.playbackStatus().catch(() => null);
      const gotAt = performance.now();
      if (!live) return;
      const authored = next?.authored && next.state !== "ended" ? next : null;
      playClock.set(authored ? readingFrom(authored.nowMs ?? authored.positionMs, authored.speed ?? 1, authored.state === "playing", sentAt, gotAt) : null);
      useSequencer.setState(authored?.state === "playing" ? { status: authored, playheadMs: authored.positionMs } : { status: authored });
      setStatus(next);
      timer = setTimeout(() => void poll(), next ? POLL_MS : IDLE_MS);
    };
    void poll();
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [backend]);

  // While the open sequence is still, it's drawn at the main window's playhead.
  const sequence = feed?.source === "sequence";
  const running = status?.authored === true && status.state === "playing";
  const playheadMs = feed?.playheadMs;
  useEffect(() => {
    if (sequence && !running && playheadMs !== undefined) useSequencer.setState({ playheadMs });
  }, [sequence, running, playheadMs]);

  // Filling the display: asked of this window, wherever it was dragged to.
  useEffect(() => appWindow.onFullscreenChange(setFull), []);
  const fill = (on: boolean) => {
    setFull(on);
    void appWindow.setFullscreen(on).catch(() => undefined);
  };
  useEffect(() => {
    if (!full) {
      setAwake(true);
      return;
    }
    // The button that was clicked lets go of the keys: Space is play and pause again.
    if (document.activeElement instanceof HTMLElement) document.activeElement.blur();
    let timer: ReturnType<typeof setTimeout> | undefined;
    const wake = () => {
      setAwake(true);
      clearTimeout(timer);
      timer = setTimeout(() => setAwake(false), HIDE_MS);
    };
    wake();
    const events = ["pointermove", "pointerdown", "keydown"] as const;
    for (const name of events) window.addEventListener(name, wake);
    return () => {
      clearTimeout(timer);
      for (const name of events) window.removeEventListener(name, wake);
    };
  }, [full]);

  const transport = feed?.transport === true;
  const keys = useRef({ full, transport });
  keys.current = { full, transport };
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.metaKey || e.ctrlKey || e.altKey) return;
      const target = e.target as HTMLElement | null;
      if (target?.closest?.("[role=dialog]") || target?.tagName === "INPUT") return;
      if (e.key === "Escape" && keys.current.full) {
        e.preventDefault();
        fill(false);
      } else if ((e.key === "f" || e.key === "F") && !e.repeat) {
        e.preventDefault();
        fill(!keys.current.full);
      } else if (e.key === " " && keys.current.transport && !target?.closest?.("button")) {
        e.preventDefault();
        setToggles((n) => n + 1);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const playing = status?.state === "playing";
  const hidden = full && !awake;
  const title = feed?.title ?? showName;
  // The open sequence's own time; for the lights live, whatever is playing.
  const shownStatus = sequence ? (status?.authored ? status : null) : status;
  const positionMs = shownStatus?.positionMs ?? (sequence ? (feed?.playheadMs ?? null) : null);
  const durationMs = shownStatus?.durationMs ?? (sequence ? (feed?.durationMs ?? null) : null);

  return (
    <div data-preview-window className={`relative flex h-full flex-col bg-black text-neutral-100 ${hidden ? "cursor-none" : ""}`}>
      <header
        data-hidden={hidden || undefined}
        className={`flex items-center gap-2 px-2 py-1.5 transition-opacity duration-300 ${
          full
            ? `absolute inset-x-0 top-0 z-10 bg-black/75 backdrop-blur-sm ${hidden ? "pointer-events-none opacity-0 has-[:focus-visible]:pointer-events-auto has-[:focus-visible]:opacity-100" : ""}`
            : "shrink-0 border-b border-neutral-800 bg-neutral-950"
        }`}
      >
        <ModeSwitch mode={mode} onChange={setMode} />
        <GlowControl iconOnly />
        <h1 className="ml-1 min-w-0 flex-1 truncate text-sm font-medium" title={title ?? undefined}>
          {title ?? "Preview"}
        </h1>
        {positionMs !== null && durationMs !== null && (
          <p className="shrink-0 text-xs text-neutral-400 tabular-nums">
            {clock(positionMs / 1000)} / {clock(durationMs / 1000)}
          </p>
        )}
        {transport && (
          <BarButton label={playing ? "Pause" : "Play"} shortcut="Space" onClick={() => setToggles((n) => n + 1)}>
            {playing ? <Pause size={16} aria-hidden /> : <Play size={16} aria-hidden />}
          </BarButton>
        )}
        <BarButton label={full ? "Leave full screen" : "Full screen"} hint={full ? "Leave full screen" : "Fill this display with the lights"} shortcut={full ? "Esc" : "F"} pressed={full} onClick={() => fill(!full)}>
          {full ? <Minimize size={16} aria-hidden /> : <Maximize size={16} aria-hidden />}
        </BarButton>
      </header>
      <main className="relative min-h-0 flex-1">
        {sequence && feed ? <SequenceStage feed={feed} mode={mode} playing={running} /> : <LiveStage mode={mode} playing={playing} />}
      </main>
      <TooltipLayer />
    </div>
  );
}
