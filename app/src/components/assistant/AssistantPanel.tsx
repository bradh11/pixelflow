import { CheckCircle2, Loader2, MessageSquarePlus, Music, Send, Settings, Sparkles, Square, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { providerName } from "../../api/assistant";
import { type ChatItem, useAssistant } from "../../state/assistant";
import { Button } from "../ui";
import { ProposalCard } from "./ProposalCard";

const SUGGESTIONS = ["Create a compelling sequence", "Add two arches beside the garage", "What's in my show?", "Rename the show to Christmas 2026"];

/** The assistant's Choose a song button: starts a new sequence from a song the user picks. */
function ChooseSong({ item }: { item: ChatItem }) {
  const streaming = useAssistant((s) => s.streaming);
  const song = item.song ?? { status: "open" };
  if (song.status === "done") {
    return (
      <p className="flex items-center gap-1.5 rounded-md border border-neutral-200 px-3 py-2 text-xs text-neutral-600 dark:border-neutral-800 dark:text-neutral-300">
        <CheckCircle2 size={14} className="text-green-600 dark:text-green-400" aria-hidden /> New sequence: {song.name}
      </p>
    );
  }
  return (
    <section aria-label="Choose a song" className="rounded-lg border border-accent-500/40 bg-accent-50/60 p-3 dark:border-accent-400/30 dark:bg-accent-600/10">
      <p>Pick a song from your computer. PixelFlow starts a new sequence for it with a row for every prop and group; nothing is saved until you save.</p>
      <Button className="mt-2" variant="primary" disabled={streaming || song.status === "picking"} onClick={() => void useAssistant.getState().chooseSong(item.id)}>
        {song.status === "picking" ? <Loader2 size={14} className="animate-spin" aria-hidden /> : <Music size={14} aria-hidden />}
        {song.status === "picking" ? "Opening the song…" : "Choose a song…"}
      </Button>
    </section>
  );
}

/** The chat with the assistant, beside the current screen. Replies stream in; changes come as a proposal card. */
export function AssistantPanel() {
  const items = useAssistant((s) => s.items);
  const streaming = useAssistant((s) => s.streaming);
  const activity = useAssistant((s) => s.activity);
  const proposal = useAssistant((s) => s.proposal);
  const provider = useAssistant((s) => s.provider);
  const model = useAssistant((s) => s.models[s.provider] ?? null);
  const hasKey = useAssistant((s) => s.hasKey);
  const { send, stop, newChat, setOpen, setSettingsOpen } = useAssistant.getState();
  const [draft, setDraft] = useState("");
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const endRef = useRef<HTMLDivElement>(null);
  const ready = hasKey === true && model !== null;

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  useEffect(() => {
    endRef.current?.scrollIntoView({ block: "end" });
  }, [items, activity]);

  const submit = (text = draft) => {
    if (!text.trim() || streaming) return;
    setDraft("");
    void send(text);
  };

  return (
    <aside
      aria-label="Assistant"
      className="flex w-96 shrink-0 flex-col border-l border-neutral-200 bg-neutral-50/60 dark:border-neutral-800 dark:bg-neutral-950/40"
    >
      <header className="flex h-11 shrink-0 items-center gap-1 border-b border-neutral-200 px-3 dark:border-neutral-800">
        <Sparkles size={16} className="text-accent-600 dark:text-accent-400" aria-hidden />
        <h2 className="font-semibold">Assistant</h2>
        <button
          type="button"
          onClick={() => setSettingsOpen(true)}
          className="ml-1 truncate rounded px-1.5 py-0.5 text-xs text-neutral-500 hover:bg-neutral-200/70 dark:hover:bg-neutral-800"
          title="Change the provider or model"
        >
          {ready ? `${providerName(provider)} · ${model}` : "Not set up"}
        </button>
        <div className="ml-auto flex items-center">
          <Button variant="ghost" aria-label="New chat" title="New chat" disabled={streaming || items.length === 0} onClick={() => void newChat()}>
            <MessageSquarePlus size={16} aria-hidden />
          </Button>
          <Button variant="ghost" aria-label="AI settings" title="AI settings" onClick={() => setSettingsOpen(true)}>
            <Settings size={16} aria-hidden />
          </Button>
          <Button variant="ghost" aria-label="Close assistant" title="Close (⌘L)" onClick={() => setOpen(false)}>
            <X size={16} aria-hidden />
          </Button>
        </div>
      </header>

      <div className="min-h-0 flex-1 overflow-auto px-3 py-3" aria-live="polite" aria-busy={streaming}>
        {!ready && hasKey !== null ? (
          <div className="flex flex-col items-start gap-3 rounded-lg border border-dashed border-neutral-300 p-4 text-sm dark:border-neutral-700">
            <p className="font-medium">Connect an AI model</p>
            <p className="text-neutral-500 dark:text-neutral-400">
              Add your own Anthropic or OpenAI key and pick a model. The assistant can read your show and draft changes for you to
              review; nothing changes until you press Apply.
            </p>
            <Button variant="primary" onClick={() => setSettingsOpen(true)}>
              <Settings size={14} aria-hidden /> Set up in Settings
            </Button>
          </div>
        ) : items.length === 0 ? (
          <div className="flex flex-col gap-2 text-sm">
            <p className="text-neutral-500 dark:text-neutral-400">
              Ask about your show, or ask for a change. You&apos;ll see what would change, with a preview, before anything happens.
            </p>
            {SUGGESTIONS.map((s) => (
              <button
                key={s}
                type="button"
                onClick={() => submit(s)}
                className="rounded-md border border-neutral-200 px-3 py-2 text-left hover:bg-neutral-100 dark:border-neutral-800 dark:hover:bg-neutral-900"
              >
                {s}
              </button>
            ))}
          </div>
        ) : (
          <ol className="flex flex-col gap-3 text-sm">
            {items.map((item) => (
              <li key={item.id}>
                {item.role === "user" ? (
                  <div className="ml-8 rounded-lg bg-accent-600 px-3 py-2 whitespace-pre-wrap text-white">
                    <span className="sr-only">You: </span>
                    {item.text}
                  </div>
                ) : item.role === "assistant" ? (
                  <div className="mr-4 whitespace-pre-wrap">
                    <span className="sr-only">Assistant: </span>
                    {item.text}
                  </div>
                ) : item.role === "error" ? (
                  <div role="alert" className="rounded-md bg-red-50 px-3 py-2 text-red-700 dark:bg-red-950/50 dark:text-red-300">
                    {item.text}
                    {item.details && (
                      <details className="mt-1 text-xs">
                        <summary className="cursor-pointer select-none">Details</summary>
                        <p className="mt-1 font-mono break-words whitespace-pre-wrap opacity-90">{item.details}</p>
                      </details>
                    )}
                  </div>
                ) : item.role === "chooseSong" ? (
                  <ChooseSong item={item} />
                ) : proposal && item.proposalId === proposal.id ? (
                  <ProposalCard proposal={proposal} current />
                ) : (
                  <p className="text-xs text-neutral-500">An earlier proposal, replaced by a newer one.</p>
                )}
              </li>
            ))}
          </ol>
        )}
        {streaming && (
          <p className="mt-3 flex items-center gap-2 text-xs text-neutral-500" role="status">
            <Loader2 size={12} className="animate-spin" aria-hidden />
            {activity ?? "Writing"}…
          </p>
        )}
        <div ref={endRef} />
      </div>

      <form
        className="shrink-0 border-t border-neutral-200 p-3 dark:border-neutral-800"
        onSubmit={(e) => {
          e.preventDefault();
          submit();
        }}
      >
        <label htmlFor="assistant-input" className="sr-only">
          Message the assistant
        </label>
        <textarea
          id="assistant-input"
          ref={inputRef}
          rows={3}
          value={draft}
          disabled={!ready}
          placeholder={ready ? "Ask or describe a change… (Enter to send)" : "Set up a key and model first"}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
              e.preventDefault();
              submit();
            }
          }}
          className="w-full resize-none rounded-md border border-neutral-300 bg-white px-2 py-1.5 text-sm disabled:opacity-60 dark:border-neutral-700 dark:bg-neutral-950"
        />
        <div className="mt-2 flex items-center justify-between gap-2">
          <span className="text-xs text-neutral-500">Can&apos;t save files, start lights, or contact controllers.</span>
          {streaming ? (
            <Button onClick={() => void stop()}>
              <Square size={12} aria-hidden /> Stop
            </Button>
          ) : (
            <Button type="submit" variant="primary" disabled={!ready || !draft.trim()}>
              <Send size={14} aria-hidden /> Send
            </Button>
          )}
        </div>
      </form>
    </aside>
  );
}
