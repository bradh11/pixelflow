import { AlertTriangle, Lightbulb, ListMusic, Maximize, Minimize, PictureInPicture2 } from "lucide-react";
import { type ReactNode, useCallback, useEffect, useRef, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import { errorMessage } from "../api/backend";
import type { ControllerStatus, PlaybackStatus, SequenceEntry, Waveform } from "../api/types";
import { ChannelGrid } from "../components/ChannelGrid";
import { typing } from "../components/layout/useLayoutKeys";
import { LivePreview, LiveViewControls } from "../components/layout3d/LivePreview";
import { useMissingFile } from "../components/MissingFiles";
import { MusicSection } from "../components/play/Music";
import { OnYourFpp } from "../components/play/OnYourFpp";
import { PlaylistDrawer } from "../components/play/PlaylistDrawer";
import { BarButton, TransportBar } from "../components/play/TransportBar";
import { useBusyFpps } from "../components/play/useBusyFpps";
import { PoppedOutNote } from "../components/preview/PopOut";
import { PreviewStage } from "../components/preview/PreviewStage";
import { AddSequenceButton } from "../components/SequenceList";
import { useSequenceFrame, useSequencePolling } from "../components/sequencer/useSequenceFrame";
import { Button } from "../components/ui";
import { plural, sequenceTitle, thousands } from "../lib/format";
import { hintFor } from "../lib/shortcuts";
import { useElementWidth } from "../lib/useWidth";
import { isBusyOrAsking } from "../state/busy";
import { usePreviewToggles, usePreviewWindow } from "../state/previewWindow";
import { recentFor, useSequencer } from "../state/sequencer";
import { useApp } from "../state/store";
import { useTheatre, watchDisplay } from "../state/theatre";
import { useView3d } from "../state/view3d";

/** How often playback state and the live preview refresh. */
const STATUS_MS = 250;
const FRAME_MS = 50;
/** Waveform slices to draw. */
const WAVEFORM_SLICES = 1200;
/** In full screen, how long the transport stays up after the pointer or a key was last used. */
const HIDE_MS = 2500;
/** Narrower than this, the playlist shows over the preview's edge rather than beside it. */
const DRAWER_BESIDE_FROM = 1000;
/** Whether the playlist was left open or closed, on this computer. */
const DRAWER_KEY = "pixelflow.playDrawer";

function loadDrawer(): boolean | null {
  try {
    const saved = localStorage.getItem(DRAWER_KEY);
    return saved === "open" ? true : saved === "closed" ? false : null;
  } catch {
    return null;
  }
}

function saveDrawer(open: boolean) {
  try {
    localStorage.setItem(DRAWER_KEY, open ? "open" : "closed");
  } catch {
    // Storage unavailable: it stays as it is until the screen closes.
  }
}

/** What's picked to play: the sequence open in the editor, or one of the show's rendered ones. */
type Pick = { kind: "doc" } | { kind: "entry"; id: string };

/**
 * What the transport works on: the sequence open in the editor, or a rendered sequence (`entry`
 * is null for one playing that's no longer on the playlist) with its playback, if it's loaded.
 */
type Now = { kind: "doc" } | { kind: "entry"; entry: SequenceEntry | null; status: PlaybackStatus | null };

/** The open sequence at its playhead, and live while it plays. */
function DocStage({ durationMs }: { durationMs: number }) {
  const doc = useSequencer((s) => s.doc);
  const mode = useView3d((s) => s.playMode);
  const frame = useSequenceFrame(durationMs, doc);
  const playing = useSequencer((s) => s.status?.state === "playing");
  return <PreviewStage frame={frame} mode={mode} playing={playing} />;
}

/**
 * The open sequence's transport: the same playback as the Sequence screen's (its state is the
 * sequencer's), with looping and "Show on my lights". Its own component: the playhead changes
 * many times a second.
 */
function DocTransport({ onAct, children }: { onAct: (action: () => Promise<unknown>) => void; children: ReactNode }) {
  const s = useSequencer(
    useShallow((st) => ({
      name: st.doc?.name ?? "",
      durationMs: st.doc?.durationMs ?? 0,
      frameMs: st.doc?.frameMs ?? 25,
      playheadMs: st.playheadMs,
      playing: st.status?.state === "playing",
      active: st.status !== null,
      looping: st.looping,
      sendToControllers: st.sendToControllers,
    })),
  );
  const act = useSequencer.getState;
  const atStart = s.playheadMs === 0;
  return (
    <TransportBar
      name={s.name}
      playing={s.playing}
      // Stop leaves the playhead where it is; pressed again, it goes back to the start.
      stopLabel={s.active || atStart ? "Stop" : "Back to the start"}
      stopDisabled={!s.active && atStart}
      positionMs={s.playheadMs}
      durationMs={s.durationMs}
      stepMs={s.frameMs}
      onToggle={() => onAct(() => (act().status?.state === "playing" ? act().pause() : act().play()))}
      onRestart={() => onAct(() => act().seek(0))}
      onStop={() => onAct(() => act().stop())}
      onScrub={(ms) => void act().seek(ms)}
      onScrubEnd={() => {}}
      loop={{ on: s.looping, set: (on) => act().setLooping(on) }}
      lights={{ on: s.sendToControllers, set: (on) => void act().setSendToControllers(on) }}
    >
      {children}
    </TransportBar>
  );
}

/** A rendered sequence always goes to the controllers: whether they're all getting it. */
function Sending({ controllers }: { controllers: ControllerStatus[] }) {
  if (controllers.length === 0) return null;
  const trouble = controllers.filter((c) => c.state !== "ok").length;
  return (
    <span
      title={controllers.map((c) => `${c.name}: ${c.state === "ok" ? "sending" : c.state}`).join(", ")}
      className={`ml-2 inline-flex shrink-0 items-center gap-1 text-xs ${trouble ? "text-amber-600 dark:text-amber-400" : "text-green-600 dark:text-green-400"}`}
    >
      <Lightbulb size={13} aria-hidden />
      <span className="hidden @min-[760px]:inline">{trouble ? `${plural(trouble, "controller")} in trouble` : `On ${plural(controllers.length, "controller")}`}</span>
    </span>
  );
}

/**
 * Show time: the lights filling the window as a sequence plays, with a transport under them and
 * the playlist in a drawer. It plays what's being worked on (the sequence open in the editor,
 * with the Sequence screen's own playback) or one of the show's rendered sequences (.fseq, with
 * its music, to the controllers), and goes full screen.
 */
export function PlayScreen() {
  const backend = useApp((s) => s.backend);
  const snapshot = useApp((s) => s.snapshot);
  const setScreen = useApp((s) => s.setScreen);
  const [status, setStatus] = useState<PlaybackStatus | null>(null);
  const [frame, setFrame] = useState<Uint8Array | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [scrub, setScrub] = useState<number | null>(null);
  const { busy: busyFpps, recheck: recheckFpps } = useBusyFpps();
  const [stopping, setStopping] = useState<string | null>(null);
  // What the screen last showed, and when the user last did something: answers to status polls
  // that started before the user's last action are out of date and ignored.
  const [notice, setNotice] = useState<string | null>(null);
  const statusRef = useRef<PlaybackStatus | null>(null);
  const actions = useRef(0);
  statusRef.current = status;
  const known = snapshot?.show.controllers.some((c) => c.sequenceChannels) ?? false;
  // Controllers that get sequence data but have no strings in PixelFlow yet: shown as raw grids.
  const unwired = (snapshot?.show.controllers ?? []).filter((c) => c.sequenceChannels && !c.ports.some((p) => p.slots.length > 0));
  const [raw, setRaw] = useState<Uint8Array | null>(null);
  const sequences = snapshot?.show.sequences ?? [];
  const [waveform, setWaveform] = useState<Waveform | null>(null);
  const [waveformLoading, setWaveformLoading] = useState(false);
  const [playAll, setPlayAll] = useState(false);
  const musicVolume = useApp((s) => s.musicVolume);
  const setMusicVolume = useApp((s) => s.setMusicVolume);
  const theatre = useTheatre((s) => s.on);
  const poppedOut = usePreviewWindow((s) => s.poppedOut);
  const root = useRef<HTMLDivElement>(null);
  const narrow = useElementWidth(root, (w) => w !== null && w < DRAWER_BESIDE_FROM);
  useSequencePolling();

  // The sequence open in the editor (only what this screen shows of it: its playhead has a
  // component of its own).
  const doc = useSequencer(useShallow((s) => (s.doc ? { name: s.doc.name, durationMs: s.doc.durationMs, path: s.path } : null)));
  const docActive = useSequencer((s) => s.status !== null);
  const recent = useSequencer((s) => s.recent);
  const saved = recentFor(recent, snapshot?.path ?? null).mine.filter((r) => r.path !== doc?.path);

  // What's picked: the user's choice while it's still there, else the open sequence, else the
  // first rendered one.
  const [picked, setPicked] = useState<Pick | null>(null);
  const pickedEntry = picked?.kind === "entry" ? sequences.find((s) => s.id === picked.id) : undefined;
  const selected: Pick | null = pickedEntry
    ? { kind: "entry", id: pickedEntry.id }
    : doc
      ? { kind: "doc" }
      : sequences[0]
        ? { kind: "entry", id: sequences[0].id }
        : null;
  const selectedEntry = selected?.kind === "entry" ? (sequences.find((s) => s.id === selected.id) ?? null) : null;
  const missingSequence = useMissingFile(selectedEntry ? { kind: "sequence", id: selectedEntry.id } : null);
  const missingMusic = useMissingFile(selectedEntry?.audio ? { kind: "music", id: selectedEntry.id } : null);

  /** A rendered sequence the engine has loaded (playing, paused, or finished). */
  const fseq = status && !status.authored ? status : null;
  /** Its playback, when it's the picked one's (the music's waveform follows it). */
  const current = fseq && selectedEntry && fseq.sequence === selectedEntry.id ? fseq : null;
  // The transport works on what's playing, else on what's picked.
  const now: Now | null =
    fseq && fseq.state !== "ended"
      ? { kind: "entry", entry: sequences.find((s) => s.id === fseq.sequence) ?? null, status: fseq }
      : doc && (docActive || selected?.kind === "doc")
        ? { kind: "doc" }
        : selectedEntry
          ? { kind: "entry", entry: selectedEntry, status: current }
          : null;
  const nowName = now === null ? null : now.kind === "doc" ? (doc?.name ?? null) : (now.entry?.name ?? (now.status ? sequenceTitle(now.status.path) : null));
  const nowIsDoc = now?.kind === "doc";

  // The drawer is closed while a sequence is open (it's what plays), until the user says otherwise.
  const [drawerChoice, setDrawerChoice] = useState(loadDrawer);
  const drawerOpen = (drawerChoice ?? doc === null) && now !== null && !theatre;
  const setDrawer = (open: boolean) => {
    saveDrawer(open);
    setDrawerChoice(open);
  };

  const run = useCallback(async (action: () => Promise<PlaybackStatus | null | void>) => {
    actions.current++;
    try {
      const next = await action();
      if (next !== undefined) setStatus(next);
      setNotice(null);
      setError(null);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      actions.current++;
    }
  }, []);

  // Playback state, whatever started it.
  useEffect(() => {
    if (!backend) return;
    let cancelled = false;
    let polling = false;
    const poll = async () => {
      if (polling) return;
      polling = true;
      const startedAt = actions.current;
      try {
        const s = await backend.playbackStatus();
        if (cancelled || startedAt !== actions.current) return;
        if (s === null && statusRef.current !== null && !statusRef.current.authored) {
          // A rendered sequence ended without the user stopping it: an edit to the show may be why.
          const reason = await backend.playbackStopReason().catch(() => null);
          if (cancelled || startedAt !== actions.current) return;
          setNotice(reason);
        }
        setStatus(s);
      } catch {
        // The next poll tries again.
      } finally {
        polling = false;
      }
    };
    void poll();
    const timer = setInterval(() => void poll(), STATUS_MS);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [backend]);

  // The live preview, while a rendered sequence is loaded (the open sequence draws its own).
  const live = fseq !== null;
  useEffect(() => {
    if (!backend || !live) {
      setFrame(null);
      return;
    }
    let cancelled = false;
    let pending = false;
    const timer = setInterval(() => {
      if (pending) return;
      pending = true;
      backend.liveFrame().then(
        (f) => {
          pending = false;
          if (!cancelled) setFrame(f.length ? f : null);
        },
        () => {
          pending = false;
        },
      );
    }, FRAME_MS);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [backend, live]);

  const wantRaw = live && unwired.length > 0;
  useEffect(() => {
    if (!backend || !wantRaw) {
      setRaw(null);
      return;
    }
    let cancelled = false;
    let pending = false;
    const timer = setInterval(() => {
      if (pending) return;
      pending = true;
      backend.sequenceFrame().then(
        (f) => {
          pending = false;
          if (!cancelled) setRaw(f.length ? f : null);
        },
        () => {
          pending = false;
        },
      );
    }, FRAME_MS);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [backend, wantRaw]);

  // The picked rendered sequence's music, drawn once per file.
  const audio = selectedEntry?.audio ?? null;
  useEffect(() => {
    setWaveform(null);
    if (!backend || !audio) {
      setWaveformLoading(false);
      return;
    }
    let cancelled = false;
    setWaveformLoading(true);
    backend.audioWaveform(audio, WAVEFORM_SLICES).then(
      (w) => {
        if (cancelled) return;
        setWaveform(w);
        setWaveformLoading(false);
      },
      () => {
        if (cancelled) return;
        setWaveform(null);
        setWaveformLoading(false);
      },
    );
    return () => {
      cancelled = true;
    };
  }, [backend, audio]);

  // Play all: when a rendered sequence finishes (lights and music), the next one starts, and
  // after the last, the first. What's picked stays where the user put it.
  const endedId = fseq?.state === "ended" && !fseq.error ? fseq.sequence : null;
  // A sequence that had already finished when play all was turned on isn't followed by the next.
  const skipEnded = useRef<string | null>(null);
  const togglePlayAll = (on: boolean) => {
    skipEnded.current = on ? endedId : null;
    setPlayAll(on);
  };
  useEffect(() => {
    if (!endedId) {
      skipEnded.current = null;
      return;
    }
    if (!playAll || endedId === skipEnded.current || sequences.length === 0) return;
    skipEnded.current = endedId;
    const at = sequences.findIndex((s) => s.id === endedId);
    const next = sequences[(at + 1) % sequences.length];
    void run(() => backend!.playSequence(next.id, 0));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [endedId, playAll]);

  /** Does something with the open sequence's playback, then reads where the engine is: it may
   * have taken over from a rendered sequence, or let go. */
  const docAct = (action: () => Promise<unknown>) =>
    void run(async () => {
      await action();
      return backend!.playbackStatus();
    });

  /** The rendered sequence the transport is on: jumps there, or starts it from there. */
  const seekEntry = (ms: number) => {
    if (now?.kind !== "entry") return;
    if (now.status) void run(() => backend!.seekPlayback(ms));
    else if (now.entry) void run(() => backend!.playSequence(now.entry!.id, ms));
  };

  const stopEntry = () =>
    run(async () => {
      await backend!.stopPlayback();
      setStatus(null);
    });

  /** Play or pause, whatever the transport is on (its button, Space, the preview's own window). */
  const toggle = () => {
    if (!now) return;
    if (now.kind === "doc") {
      const seq = useSequencer.getState();
      docAct(() => (seq.status?.state === "playing" ? seq.pause() : seq.play()));
    } else if (!now.status || now.status.state === "ended") {
      if (now.entry) void run(() => backend!.playSequence(now.entry!.id, 0));
    } else {
      const playing = now.status.state === "playing";
      void run(() => backend!.pausePlayback(playing));
    }
  };
  usePreviewToggles(toggle);

  // Picking from the playlist plays it.
  const pickDoc = () => {
    setPicked({ kind: "doc" });
    const seq = useSequencer.getState();
    if (seq.status?.state !== "playing") docAct(() => seq.play());
  };
  const pickEntry = (id: string) => {
    setPicked({ kind: "entry", id });
    if (!(fseq && fseq.state !== "ended" && fseq.sequence === id)) void run(() => backend!.playSequence(id, 0));
  };
  /** Opens a saved sequence in the editor (asking first about unsaved changes to the open one)
   * and plays it. */
  const openSaved = (path: string) =>
    void useSequencer.getState().replaceAfterAsking(async () => {
      if (!(await useSequencer.getState().open(path))) return;
      setPicked({ kind: "doc" });
      docAct(() => useSequencer.getState().play());
    });

  const scrubRef = useRef<number | null>(null);
  scrubRef.current = scrub;
  /** Jumps to where the seek bar was left; when the sequence isn't loaded, it starts playing from there. */
  const commitSeek = () => {
    const target = scrubRef.current;
    if (target === null) return;
    scrubRef.current = null;
    setScrub(null);
    seekEntry(target);
  };

  const setVolume = (volume: number) => {
    setMusicVolume(volume);
    // The engine keeps the volume for whatever plays next, too.
    void run(() => backend!.setPlaybackVolume(volume));
  };

  // What the preview shows, for its own window when it's popped out.
  useEffect(() => {
    usePreviewWindow.getState().setPlay({ source: nowIsDoc ? "sequence" : "live", title: nowName });
  }, [nowIsDoc, nowName]);

  // Full screen ends with the screen, and when the window stops filling its display some other way.
  useEffect(() => {
    const stop = watchDisplay();
    return () => {
      stop();
      useTheatre.getState().exit();
    };
  }, []);
  const canFill = now !== null && !poppedOut;

  // In full screen the transport hides itself while a sequence plays, until the pointer or a key
  // is used again.
  const [awake, setAwake] = useState(true);
  useEffect(() => {
    if (!theatre) {
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
  }, [theatre]);
  const docPlaying = useSequencer((s) => s.status?.state === "playing");
  const playing = now?.kind === "doc" ? docPlaying : now?.status?.state === "playing";
  const hidden = theatre && !awake && playing;

  // The screen's keys, as on the Sequence screen: Space plays and pauses, L loops, Home and End
  // jump; F goes full screen and Escape leaves it.
  const keys = useRef({ toggle, canFill, now, seekEntry });
  keys.current = { toggle, canFill, now, seekEntry };
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.metaKey || e.ctrlKey || e.altKey) return;
      const target = e.target as HTMLElement | null;
      if (target?.closest?.("[role=dialog], [role=menu]")) return;
      if (useApp.getState().paletteOpen || isBusyOrAsking()) return;
      const k = keys.current;
      if (e.key === "Escape") {
        if (useTheatre.getState().on) {
          e.preventDefault();
          useTheatre.getState().exit();
        }
        return;
      }
      // A field keeps its own keys (the seek bar has Home and End of its own).
      if (typing(target) || target?.tagName === "INPUT") return;
      if (e.key === " ") {
        // Space on a focused button presses that button instead.
        if (target?.closest?.("button, a, [role=button], [role=checkbox], [role=menuitem]")) return;
        e.preventDefault();
        k.toggle();
      } else if (e.key === "f" || e.key === "F") {
        e.preventDefault();
        if (e.repeat) return;
        if (useTheatre.getState().on) useTheatre.getState().exit();
        else if (k.canFill) useTheatre.getState().enter();
      } else if ((e.key === "l" || e.key === "L") && k.now?.kind === "doc") {
        e.preventDefault();
        const seq = useSequencer.getState();
        if (!e.repeat) seq.setLooping(!seq.looping);
      } else if ((e.key === "Home" || e.key === "End") && k.now) {
        e.preventDefault();
        const seq = useSequencer.getState();
        if (k.now.kind === "doc") void seq.seek(e.key === "Home" ? 0 : (seq.doc?.durationMs ?? 0));
        else k.seekEntry(e.key === "Home" ? 0 : (k.now.status?.durationMs ?? 0));
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const volume = current?.volume ?? musicVolume;
  const entryStatus = now?.kind === "entry" ? now.status : null;
  /** How long the rendered sequence on the transport lasts, as far as is known (its music, until it plays). */
  const entryLength = entryStatus?.durationMs ?? (now?.kind === "entry" && now.entry === selectedEntry ? (waveform?.durationMs ?? 0) : 0);
  const entryPosition = scrub ?? entryStatus?.positionMs ?? 0;
  const unsettled = entryStatus?.controllers.filter((c) => c.state !== "ok") ?? [];

  const chrome = "transition-opacity duration-300";
  // Hidden, it comes back for the keyboard: tabbing onto one of its controls shows it.
  const asleep = hidden ? "pointer-events-none opacity-0 has-[:focus-visible]:pointer-events-auto has-[:focus-visible]:opacity-100" : "";

  /** The bar's own buttons: how it looks, the playlist, its own window, and full screen. */
  const buttons = (
    <>
      <LiveViewControls drop="up" />
      {!theatre && (
        <BarButton label="Playlist" hint={drawerOpen ? "Put the playlist away" : "What there is to play, each sequence's music, and what's on your FPP"} expanded={drawerOpen} onClick={() => setDrawer(!drawerOpen)}>
          <ListMusic size={16} aria-hidden /> <span className="hidden @min-[760px]:inline">Playlist</span>
        </BarButton>
      )}
      {!theatre && (
        <BarButton
          label={poppedOut ? "Bring the preview back" : "Pop out the preview"}
          hint={poppedOut ? "Close the preview's own window and show it here again" : "Open the preview in its own window: the lights on another display, the controls here"}
          pressed={poppedOut}
          onClick={() => void (poppedOut ? usePreviewWindow.getState().bringBack() : usePreviewWindow.getState().popOut())}
        >
          <PictureInPicture2 size={16} aria-hidden />
        </BarButton>
      )}
      {canFill && (
        <BarButton
          label={theatre ? "Leave full screen" : "Full screen"}
          hint={theatre ? "Leave full screen" : "Full screen: only the lights, with these controls when you move the pointer"}
          shortcut={hintFor(theatre ? "play-escape" : "play-fullscreen")}
          pressed={theatre}
          onClick={() => (theatre ? useTheatre.getState().exit() : useTheatre.getState().enter())}
        >
          {theatre ? <Minimize size={16} aria-hidden /> : <Maximize size={16} aria-hidden />}
        </BarButton>
      )}
    </>
  );

  /** The playlist: beside the screen, or over the preview's edge in a narrow window. */
  const drawer = drawerOpen && (
    <PlaylistDrawer
      floating={narrow}
      onClose={() => setDrawer(false)}
      open={doc ? { name: doc.name, picked: selected?.kind === "doc", playing: nowIsDoc && docActive, pick: pickDoc } : null}
      saved={saved}
      onOpenSaved={openSaved}
      selectedEntry={selectedEntry?.id ?? null}
      playingEntry={fseq?.sequence ?? null}
      onPickEntry={pickEntry}
      onAdded={(id) => setPicked({ kind: "entry", id })}
      missingSequence={missingSequence}
      playAll={playAll}
      onPlayAll={togglePlayAll}
    >
      {selectedEntry && (
        <MusicSection
          entry={selectedEntry}
          missing={missingMusic}
          waveform={waveform}
          loading={waveformLoading}
          positionMs={current ? (scrub ?? current.positionMs) : null}
          durationMs={current?.durationMs ?? waveform?.durationMs ?? 0}
          volume={volume}
          onSeek={(ms) => {
            if (current) void run(() => backend!.seekPlayback(ms));
            else void run(() => backend!.playSequence(selectedEntry.id, ms));
          }}
          onVolume={setVolume}
        />
      )}
      <OnYourFpp sequences={sequences} />
    </PlaylistDrawer>
  );

  return (
    <div ref={root} data-play-screen data-theatre={theatre || undefined} className={`relative flex h-full min-h-0 ${hidden ? "cursor-none" : ""}`}>
      <h1 className="sr-only">Play</h1>
      <div className="relative flex min-w-0 flex-1 flex-col">
        <div data-theme={theatre ? "dark" : undefined} className={`${chrome} ${theatre ? `absolute inset-x-0 top-0 z-10 ${asleep}` : "shrink-0"}`}>
          {busyFpps.map((fpp) => (
            <div
              key={fpp.address}
              role="alert"
              aria-label="FPP is playing"
              className="flex items-center gap-3 border-b border-amber-300 bg-amber-50 px-3 py-1.5 text-sm text-amber-800 dark:border-amber-800 dark:bg-amber-950 dark:text-amber-300"
            >
              <AlertTriangle size={15} className="shrink-0" aria-hidden />
              <p className="min-w-0 flex-1">
                {fpp.name} is playing {fpp.status.sequence ?? fpp.status.playlist}. While it plays, it overrides PixelFlow on the same controllers.
              </p>
              <Button
                className="shrink-0"
                disabled={stopping === fpp.address}
                onClick={async () => {
                  setStopping(fpp.address);
                  try {
                    await run(async () => backend!.fppStop(fpp.address, false));
                    await recheckFpps();
                  } finally {
                    setStopping(null);
                  }
                }}
              >
                {stopping === fpp.address ? "Stopping…" : "Stop FPP"}
              </Button>
            </div>
          ))}
        </div>

        {now === null ? (
          <div className="flex min-h-0 flex-1 items-center justify-center overflow-auto p-6">
            <section aria-label="Nothing to play yet" className="flex max-w-lg flex-col items-center gap-3 rounded-lg border border-dashed border-neutral-300 p-8 text-center dark:border-neutral-700">
              <h2 className="font-medium">Nothing to play yet</h2>
              <p className="text-sm text-neutral-500 dark:text-neutral-400">
                Play shows your lights filling the window while a sequence plays. Whatever is open on the Sequence screen plays here; a sequence that's already rendered
                (.fseq) plays on your controllers too, with its music.
              </p>
              <div className="flex flex-wrap justify-center gap-2">
                <Button variant="primary" onClick={() => setScreen("sequence")}>
                  Make a sequence
                </Button>
                <AddSequenceButton label="Add a rendered sequence (.fseq)" onSelect={(id) => setPicked({ kind: "entry", id })} />
              </div>
              {!known && (
                <p className="text-sm text-neutral-500 dark:text-neutral-400">
                  PixelFlow doesn't know which channels go to which controller yet, so a rendered sequence can't reach your lights. On the Controllers screen, open your FPP
                  and add the controllers it sends to.{" "}
                  <button type="button" className="font-medium text-accent-600 underline dark:text-accent-400" onClick={() => setScreen("devices")}>
                    Go to Controllers
                  </button>
                </p>
              )}
              {error && (
                <p role="alert" className="text-sm text-red-600 dark:text-red-400">
                  {error}
                </p>
              )}
            </section>
          </div>
        ) : (
          <>
            <div className="relative flex min-h-0 flex-1 flex-col">
              <div data-theme="dark" className={`relative min-h-0 flex-1 text-neutral-100 ${theatre ? "bg-black" : "bg-neutral-950"}`}>
                {poppedOut ? <PoppedOutNote tall /> : nowIsDoc ? <DocStage durationMs={doc?.durationMs ?? 0} /> : <LivePreview frame={frame} playing={playing} />}
              </div>
              {fseq && unwired.length > 0 && !theatre && (
                <div className="flex max-h-40 shrink-0 flex-col gap-3 overflow-auto border-t border-neutral-200 p-3 dark:border-neutral-800">
                  {unwired.map((c) => (
                    <ChannelGrid
                      key={c.id}
                      frame={raw}
                      start={c.sequenceChannels!.start}
                      count={c.sequenceChannels!.count}
                      label={`${c.name}: ${thousands(Math.ceil(c.sequenceChannels!.count / 3))} pixels as received (import its strings to see them on your props)`}
                    />
                  ))}
                </div>
              )}
              {/* Over the preview, not the transport under it. */}
              {narrow && drawer}
            </div>

            <div
              data-theme={theatre ? "dark" : undefined}
              data-hidden={hidden || undefined}
              className={`${chrome} ${
                theatre
                  ? `absolute inset-x-0 bottom-0 z-10 bg-black/75 text-neutral-100 backdrop-blur-sm ${asleep}`
                  : "shrink-0 border-t border-neutral-200 dark:border-neutral-800"
              }`}
            >
              <div className="flex flex-col gap-1 px-3 text-sm empty:hidden [&>*:first-child]:pt-2">
                {error && (
                  <p role="alert" className="text-red-600 dark:text-red-400">
                    {error}
                  </p>
                )}
                {notice && !status && <p className="text-amber-700 dark:text-amber-400">{notice}</p>}
                {!known && !status && !nowIsDoc && (
                  <p className="text-amber-700 dark:text-amber-400">
                    PixelFlow doesn't know which channels go to which controller yet. On the Controllers screen, open your FPP and add the controllers it sends to.{" "}
                    <button type="button" className="font-medium underline" onClick={() => setScreen("devices")}>
                      Go to Controllers
                    </button>
                  </p>
                )}
                {entryStatus?.state === "ended" && !entryStatus.error && <p className="text-neutral-500 dark:text-neutral-400">Finished. Press play to start again.</p>}
                {entryStatus?.error && <p className="text-red-600 dark:text-red-400">{entryStatus.error}</p>}
                {entryStatus?.notes.map((note) => (
                  <p key={note} className="text-amber-700 dark:text-amber-400">
                    {note}
                  </p>
                ))}
                {unsettled.map((c) => (
                  <p key={c.id} className="text-amber-700 dark:text-amber-400">
                    {c.name}: {c.state}
                    {c.lastError && <span className="text-neutral-500"> — {c.lastError}</span>}
                  </p>
                ))}
              </div>
              {now.kind === "doc" ? (
                <DocTransport onAct={docAct}>{buttons}</DocTransport>
              ) : (
                <TransportBar
                  name={nowName ?? ""}
                  playing={entryStatus?.state === "playing"}
                  stopDisabled={!entryStatus}
                  positionMs={entryPosition}
                  durationMs={entryLength}
                  stepMs={entryStatus?.frameMs ?? 1000}
                  onToggle={toggle}
                  onRestart={() => seekEntry(0)}
                  onStop={() => void stopEntry()}
                  onScrub={setScrub}
                  onScrubEnd={commitSeek}
                  status={entryStatus && entryStatus.state !== "ended" ? <Sending controllers={entryStatus.controllers} /> : undefined}
                >
                  {buttons}
                </TransportBar>
              )}
            </div>
          </>
        )}
      </div>

      {!narrow && drawer}
    </div>
  );
}
