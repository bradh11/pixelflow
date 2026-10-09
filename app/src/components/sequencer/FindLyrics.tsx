import { Check, Loader2, MicVocal, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { LyricsFound, LyricsGate } from "../../api/sequencer";
import type { ProviderId } from "../../api/assistant";
import { useAssistant } from "../../state/assistant";
import { useSequencer } from "../../state/sequencer";
import { ProgressBar } from "../ProgressBar";
import { Button } from "../ui";

/** The question asked before a song's audio first goes to OpenAI. */
export const SEND_AUDIO_QUESTION = "This sends the song's audio to OpenAI to find the words. Continue?";

/**
 * Find lyrics, beside Detect beats: the song's words and when each is sung, as Lyrics, Lyrics
 * (words), Lyrics (syllables), Lyrics (phonemes), and Vocals timing tracks. Only once the assistant is set up; until then it's disabled,
 * and pressing it says why with a way to the assistant's settings. With an OpenAI key, it asks
 * before the song's audio first goes to OpenAI. While it runs, it shows what it's doing and a
 * Stop button. Shift-click finds again, without what's kept for the song.
 */
export function FindLyrics({ hasMusic }: { hasMusic: boolean }) {
  const api = useSequencer((s) => s.api);
  const step = useSequencer((s) => s.findingLyrics);
  const fraction = useSequencer((s) => s.lyricsFraction);
  const docKey = useSequencer((s) => s.docKey);
  const provider = useAssistant((s) => s.provider);
  const hasKey = useAssistant((s) => s.hasKey);
  const lyricsAudioOk = useAssistant((s) => s.lyricsAudioOk);
  const language = useAssistant((s) => s.lyricsLanguage);
  const [gate, setGate] = useState<LyricsGate | null>(null);
  const [why, setWhy] = useState(false);
  const [asking, setAsking] = useState(false);
  const [fresh, setFresh] = useState(false);

  // Asked again whenever the assistant's provider or key changes.
  useEffect(() => {
    if (!api) return;
    let current = true;
    api.lyricsGate(provider).then(
      (g) => current && setGate(g),
      () => current && setGate(null),
    );
    return () => {
      current = false;
    };
  }, [api, provider, hasKey, docKey]);

  const run = (upload: boolean, again = fresh) => {
    setAsking(false);
    void useSequencer.getState().findLyrics(provider, upload, { language, fresh: again });
  };
  const press = (again: boolean) => {
    if (!gate?.ready) {
      setWhy(!why);
      return;
    }
    setFresh(again);
    if (gate.recognizer && !lyricsAudioOk) setAsking(true);
    else run(gate.recognizer, again);
  };

  if (step !== null) {
    return (
      <span role="status" className="relative inline-flex max-w-56 items-center gap-1.5 px-2 text-sm text-neutral-600 dark:text-neutral-300">
        <Loader2 size={15} className="shrink-0 animate-spin" aria-hidden />
        <span className="truncate">{step}…</span>
        <button
          type="button"
          aria-label="Stop finding lyrics"
          title="Stop finding lyrics"
          className="rounded p-1 hover:bg-neutral-200/70 dark:hover:bg-neutral-800"
          onClick={() => void useSequencer.getState().cancelLyrics()}
        >
          <X size={14} />
        </button>
        {/* While a step reads the whole song: how far it has got. */}
        {fraction !== null && <ProgressBar slim label={step} fraction={fraction} className="absolute inset-x-2 bottom-0" />}
      </span>
    );
  }
  const ready = Boolean(gate?.ready) && hasMusic;
  const hint = !hasMusic
    ? "Find lyrics needs a song: choose this sequence's music first."
    : gate?.ready
      ? "Find the song's words and when each is sung, as timing tracks for lines, words, syllables, mouth shapes, and singing. Shift-click to find again, without what's kept for this song."
      : (gate?.reason ?? "Finding lyrics needs the assistant: set it up in Settings → AI.");
  return (
    <span className="relative inline-flex">
      <button
        type="button"
        aria-label="Find lyrics"
        aria-disabled={!ready}
        aria-expanded={gate?.ready ? undefined : why}
        title={hint}
        disabled={!hasMusic}
        onClick={(e) => press(e.shiftKey)}
        className={`inline-flex items-center gap-1.5 rounded-md px-2 py-1.5 text-sm text-neutral-700 hover:bg-neutral-200/70 disabled:opacity-40 disabled:hover:bg-transparent dark:text-neutral-200 dark:hover:bg-neutral-800 ${
          ready ? "" : "opacity-40"
        }`}
      >
        <MicVocal size={16} /> <span className="hidden @min-[760px]:inline">Find lyrics</span>
      </button>
      {why && !gate?.ready && <WhyNot reason={hint} onClose={() => setWhy(false)} />}
      {asking && <SendAudio onAnswer={(upload) => (upload === null ? setAsking(false) : run(upload))} />}
    </span>
  );
}

/** Why Find lyrics can't run yet, with a way to the assistant's settings. */
function WhyNot({ reason, onClose }: { reason: string; onClose: () => void }) {
  return (
    <div role="note" className="absolute top-full left-0 z-30 mt-1 w-72 rounded-md border border-neutral-200 bg-white p-3 text-sm shadow-lg dark:border-neutral-800 dark:bg-neutral-900">
      <p>{reason}</p>
      <div className="mt-2 flex justify-end gap-2">
        <Button variant="ghost" onClick={onClose}>
          Not now
        </Button>
        <Button
          variant="primary"
          onClick={() => {
            onClose();
            useAssistant.getState().setSettingsOpen(true);
          }}
        >
          AI settings
        </Button>
      </div>
    </div>
  );
}

/** Asks before the song's audio goes to OpenAI: Continue, published lyrics only, or Cancel
 * (null). "Don't ask again" remembers a Continue. */
function SendAudio({ onAnswer }: { onAnswer: (upload: boolean | null) => void }) {
  const [remember, setRemember] = useState(false);
  const cancel = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    cancel.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      onAnswer(null);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onAnswer]);
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div
        role="alertdialog"
        aria-modal="true"
        aria-label="Send the song to OpenAI?"
        aria-describedby="send-audio-question"
        className="w-[28rem] max-w-[calc(100vw-2rem)] rounded-lg border border-neutral-200 bg-white p-5 shadow-xl dark:border-neutral-800 dark:bg-neutral-900"
        onKeyDown={(e) => e.stopPropagation()}
      >
        <h2 className="text-lg font-semibold">Send the song to OpenAI?</h2>
        <p id="send-audio-question" className="mt-2 text-sm text-neutral-600 dark:text-neutral-300">
          {SEND_AUDIO_QUESTION}
        </p>
        <label className="mt-3 flex items-center gap-2 text-sm">
          <input type="checkbox" checked={remember} onChange={(e) => setRemember(e.target.checked)} />
          Don&apos;t ask again
        </label>
        <div className="mt-5 flex flex-wrap justify-end gap-2">
          <Button ref={cancel} onClick={() => onAnswer(null)}>
            Cancel
          </Button>
          <Button onClick={() => onAnswer(false)}>Published lyrics only</Button>
          <Button
            variant="primary"
            onClick={() => {
              if (remember) useAssistant.getState().setLyricsAudioOk(true);
              onAnswer(true);
            }}
          >
            Continue
          </Button>
        </div>
      </div>
    </div>
  );
}

function duration(seconds: number): string {
  const s = Math.round(seconds);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

/**
 * Where Find lyrics' words came from ("Lyrics: Lantern Band — Lantern Song (LRCLIB) · word
 * timing: OpenAI"), with Wrong song?: the other published lyrics found, to use one instead, or
 * lyrics to paste (plain lines or LRC). Either is lined up again with what was already found,
 * nothing looked up or sent. Find again looks it all up afresh.
 */
export function LyricsSource({ found, run }: { found: LyricsFound; run?: { provider: ProviderId; upload: boolean } }) {
  const [open, setOpen] = useState(false);
  return (
    <span className="relative mt-0.5 flex flex-wrap items-center gap-x-2 text-xs text-neutral-600 dark:text-neutral-400">
      <span>{found.source}</span>
      <button
        type="button"
        aria-expanded={open}
        className="rounded px-1 font-medium text-emerald-800 underline-offset-2 hover:underline dark:text-emerald-300"
        onClick={() => setOpen(!open)}
      >
        Wrong song?
      </button>
      {open && <WrongSong found={found} run={run} onClose={() => setOpen(false)} />}
    </span>
  );
}

function WrongSong({ found, run, onClose }: { found: LyricsFound; run?: { provider: ProviderId; upload: boolean }; onClose: () => void }) {
  const language = useAssistant((s) => s.lyricsLanguage);
  const [text, setText] = useState("");
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      onClose();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onClose]);
  const choose = (choice: { candidate: number } | { pasted: string }) => {
    onClose();
    void useSequencer.getState().chooseLyrics(choice);
  };
  return (
    <div
      role="dialog"
      aria-label="Choose the song's lyrics"
      className="absolute top-full left-0 z-30 mt-1 w-[26rem] max-w-[calc(100vw-2rem)] rounded-md border border-neutral-200 bg-white p-3 text-sm text-neutral-800 shadow-lg dark:border-neutral-800 dark:bg-neutral-900 dark:text-neutral-100"
      onKeyDown={(e) => e.stopPropagation()}
    >
      {found.candidates.length > 0 ? (
        <>
          <p className="text-xs text-neutral-500">Published lyrics found on LRCLIB</p>
          <ul className="mt-1 flex flex-col">
            {found.candidates.map((c) => {
              const used = c.id === found.chosen;
              const facts = [duration(c.durationS), c.language ?? "language unknown", c.synced ? "timed lines" : "no line times"];
              return (
                <li key={c.id}>
                  <button
                    type="button"
                    aria-pressed={used}
                    className="flex w-full items-start gap-2 rounded px-2 py-1.5 text-left hover:bg-neutral-100 dark:hover:bg-neutral-800"
                    onClick={() => (used ? onClose() : choose({ candidate: c.id }))}
                  >
                    <span className="mt-0.5 w-3.5 shrink-0">{used && <Check size={14} aria-hidden className="text-emerald-600 dark:text-emerald-400" />}</span>
                    <span className="min-w-0">
                      <span className="block truncate">{c.artist ? `${c.artist} — ${c.title}` : c.title}</span>
                      <span className="block text-xs text-neutral-500">{facts.join(" · ")}</span>
                    </span>
                  </button>
                </li>
              );
            })}
          </ul>
        </>
      ) : (
        <p className="text-neutral-600 dark:text-neutral-300">LRCLIB had no published lyrics for this song.</p>
      )}
      <label htmlFor="pasted-lyrics" className="mt-3 block text-xs text-neutral-500">
        Or paste the lyrics (plain lines, or LRC with [mm:ss] times)
      </label>
      <textarea
        id="pasted-lyrics"
        rows={4}
        value={text}
        onChange={(e) => setText(e.target.value)}
        className="mt-1 w-full resize-y rounded-md border border-neutral-300 bg-white px-2 py-1.5 text-sm dark:border-neutral-700 dark:bg-neutral-950"
      />
      <div className="mt-2 flex flex-wrap items-center justify-between gap-2">
        <Button
          variant="ghost"
          disabled={!run}
          title="Look the lyrics up and listen again, without what's kept for this song"
          onClick={() => {
            onClose();
            if (run) void useSequencer.getState().findLyrics(run.provider, run.upload, { language, fresh: true });
          }}
        >
          Find again
        </Button>
        <div className="flex gap-2">
          <Button onClick={onClose}>Close</Button>
          <Button variant="primary" disabled={text.trim().length === 0} onClick={() => choose({ pasted: text })}>
            Use pasted lyrics
          </Button>
        </div>
      </div>
    </div>
  );
}
