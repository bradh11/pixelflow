import { Loader2, MicVocal, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { LyricsGate } from "../../api/sequencer";
import { useAssistant } from "../../state/assistant";
import { useSequencer } from "../../state/sequencer";
import { Button } from "../ui";

/** The question asked before a song's audio first goes to OpenAI. */
export const SEND_AUDIO_QUESTION = "This sends the song's audio to OpenAI to find the words. Continue?";

/**
 * Find lyrics, beside Detect beats: the song's words and when each is sung, as Lyrics, Lyrics
 * (words), and Vocals timing tracks. Only once the assistant is set up; until then it's disabled,
 * and pressing it says why with a way to the assistant's settings. With an OpenAI key, it asks
 * before the song's audio first goes to OpenAI. While it runs, it shows what it's doing and a
 * Stop button.
 */
export function FindLyrics({ hasMusic }: { hasMusic: boolean }) {
  const api = useSequencer((s) => s.api);
  const step = useSequencer((s) => s.findingLyrics);
  const docKey = useSequencer((s) => s.docKey);
  const provider = useAssistant((s) => s.provider);
  const hasKey = useAssistant((s) => s.hasKey);
  const lyricsAudioOk = useAssistant((s) => s.lyricsAudioOk);
  const [gate, setGate] = useState<LyricsGate | null>(null);
  const [why, setWhy] = useState(false);
  const [asking, setAsking] = useState(false);

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

  const run = (upload: boolean) => {
    setAsking(false);
    void useSequencer.getState().findLyrics(provider, upload);
  };
  const press = () => {
    if (!gate?.ready) {
      setWhy(!why);
      return;
    }
    if (gate.recognizer && !lyricsAudioOk) setAsking(true);
    else run(gate.recognizer);
  };

  if (step !== null) {
    return (
      <span role="status" className="inline-flex max-w-56 items-center gap-1.5 px-2 text-sm text-neutral-600 dark:text-neutral-300">
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
      </span>
    );
  }
  const ready = Boolean(gate?.ready) && hasMusic;
  const hint = !hasMusic
    ? "Find lyrics needs a song: choose this sequence's music first."
    : gate?.ready
      ? "Find the song's words and when each is sung, as Lyrics, Lyrics (words), and Vocals timing tracks."
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
        onClick={press}
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
