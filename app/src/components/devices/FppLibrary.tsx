import { ChevronDown, Loader2, Play, Send } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { errorMessage } from "../../api/backend";
import type { FppFile, FppFolder, SequenceEntry } from "../../api/types";
import { confirmAction } from "../../state/confirm";
import { clock, plural, shortDate, sizeText } from "../../lib/format";
import { useApp } from "../../state/store";
import { SendToFppDialog } from "../SendToFppDialog";
import { Button } from "../ui";
import { Section } from "./Section";

const TABS: { folder: FppFolder; label: string; empty: string }[] = [
  { folder: "sequences", label: "Sequences", empty: "No sequences on this FPP yet." },
  { folder: "music", label: "Music", empty: "No music on this FPP." },
  { folder: "playlists", label: "Playlists", empty: "No playlists on this FPP." },
];

const NO_SEQUENCES: SequenceEntry[] = [];

/** Picks one of the show's sequences to send. */
function SendMenu({ onPick }: { onPick: (entry: SequenceEntry) => void }) {
  const sequences = useApp((s) => s.snapshot?.show.sequences ?? NO_SEQUENCES);
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.stopPropagation();
      setOpen(false);
    };
    const onDown = (e: PointerEvent) => {
      if (!box.current?.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("pointerdown", onDown);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("pointerdown", onDown);
    };
  }, [open]);
  if (sequences.length === 0) {
    return (
      <Button disabled title="Add a sequence to your show first (Play screen)">
        <Send size={14} aria-hidden /> Send a sequence…
      </Button>
    );
  }
  return (
    <div ref={box} className="relative">
      <Button aria-haspopup="menu" aria-expanded={open} title="Put one of your show's sequences and its music on this FPP" onClick={() => setOpen(!open)}>
        <Send size={14} aria-hidden /> Send a sequence… <ChevronDown size={14} aria-hidden />
      </Button>
      {open && (
        <div
          role="menu"
          aria-label="Send a sequence"
          className="absolute top-full right-0 z-40 mt-1 flex max-h-72 w-72 flex-col overflow-auto rounded-lg border border-neutral-200 bg-white p-1 shadow-xl dark:border-neutral-800 dark:bg-neutral-900"
        >
          {sequences.map((entry) => (
            <button
              key={entry.id}
              type="button"
              role="menuitem"
              className="truncate rounded px-2 py-1.5 text-left hover:bg-neutral-100 focus:bg-neutral-100 dark:hover:bg-neutral-800 dark:focus:bg-neutral-800"
              onClick={() => {
                setOpen(false);
                onPick(entry);
              }}
            >
              {entry.name}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

/**
 * The sequences, music, and playlists stored on the FPP, with Play for sequences and playlists
 * (it asks first when a show is running) and Send a sequence. Each tab is read when first shown,
 * and again on Refresh or after a send; reading changes nothing.
 */
export function FppLibrary({ address, fppName, turn, onPlayed }: { address: string; fppName: string; turn: number; onPlayed: () => void }) {
  const backend = useApp((s) => s.backend);
  const [folder, setFolder] = useState<FppFolder>("sequences");
  const [lists, setLists] = useState<Partial<Record<FppFolder, FppFile[] | { error: string }>>>({});
  const [sent, setSent] = useState(0);
  const [sending, setSending] = useState<SequenceEntry | null>(null);
  const [playError, setPlayError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  // A refresh or a send makes every tab stale; the one on show is read again now.
  useEffect(() => setLists({}), [turn, sent]);
  const list = lists[folder];
  useEffect(() => {
    if (!backend || list !== undefined) return;
    let current = true;
    backend.fppFolder(address, folder).then(
      (files) => current && setLists((l) => ({ ...l, [folder]: files })),
      (e) => current && setLists((l) => ({ ...l, [folder]: { error: errorMessage(e) } })),
    );
    return () => {
      current = false;
    };
  }, [backend, address, folder, list]);

  const play = async (name: string, shown: string) => {
    if (!backend || busy) return;
    setBusy(true);
    setPlayError(null);
    try {
      // Starting replaces whatever is playing: ask first if something is.
      const now = await backend.fppStatus(address);
      if (now.state === "playing" || now.state === "paused") {
        const current = now.sequence ?? now.playlist ?? "its show";
        const yes = await confirmAction({
          title: "Stop the running show?",
          message: `${fppName} is playing ${current}. Stop it and play ${shown} now?`,
          confirm: "Stop and play",
        });
        if (!yes) return;
      }
      await backend.fppStart(address, name);
    } catch (e) {
      setPlayError(errorMessage(e));
    } finally {
      setBusy(false);
      onPlayed();
    }
  };

  const tab = TABS.find((t) => t.folder === folder)!;
  const playable = folder !== "music";
  return (
    <Section title="On this FPP" actions={<SendMenu onPick={setSending} />} className="min-h-0">
      <div role="tablist" aria-label="What's on this FPP" className="flex gap-1 border-b border-neutral-200 dark:border-neutral-800">
        {TABS.map((t) => {
          const files = lists[t.folder];
          return (
            <button
              key={t.folder}
              id={`fpp-tab-${t.folder}`}
              type="button"
              role="tab"
              aria-selected={folder === t.folder}
              aria-controls="fpp-tabpanel"
              tabIndex={folder === t.folder ? 0 : -1}
              onClick={() => setFolder(t.folder)}
              onKeyDown={(e) => {
                const i = TABS.findIndex((x) => x.folder === folder);
                const next = { ArrowLeft: i - 1, ArrowRight: i + 1, Home: 0, End: TABS.length - 1 }[e.key];
                if (next === undefined) return;
                e.preventDefault();
                const to = TABS[(next + TABS.length) % TABS.length].folder;
                setFolder(to);
                document.getElementById(`fpp-tab-${to}`)?.focus();
              }}
              className={`-mb-px border-b-2 px-3 py-1.5 font-medium ${
                folder === t.folder ? "border-accent-600 text-neutral-900 dark:text-neutral-100" : "border-transparent text-neutral-500 hover:text-neutral-800 dark:hover:text-neutral-200"
              }`}
            >
              {t.label} {Array.isArray(files) && <span className="text-xs font-normal text-neutral-500 tabular-nums">{files.length}</span>}
            </button>
          );
        })}
      </div>
      <div id="fpp-tabpanel" role="tabpanel" aria-labelledby={`fpp-tab-${folder}`} className="min-h-0 overflow-auto">
        {list === undefined && (
          <p className="flex items-center gap-2 py-2 text-neutral-500">
            <Loader2 size={14} className="animate-spin" aria-hidden /> Reading the FPP's {tab.label.toLowerCase()}…
          </p>
        )}
        {list !== undefined && !Array.isArray(list) && (
          <p role="alert" className="py-2 text-red-600 dark:text-red-400">
            Couldn't read the FPP's {tab.label.toLowerCase()}: {list.error}
          </p>
        )}
        {Array.isArray(list) && list.length === 0 && <p className="py-2 text-neutral-500">{tab.empty}</p>}
        {Array.isArray(list) && list.length > 0 && (
          <table className="w-full">
            <thead>
              <tr className="text-left text-xs text-neutral-500">
                <th className="py-1.5 font-medium">Name</th>
                {folder === "playlists" && <th className="py-1.5 pl-3 text-right font-medium">Items</th>}
                <th className="py-1.5 pl-3 text-right font-medium">Length</th>
                <th className="py-1.5 pl-3 text-right font-medium">Size</th>
                <th className="py-1.5 pl-3 font-medium">Date</th>
                {playable && (
                  <th className="w-px py-1.5">
                    <span className="sr-only">Play</span>
                  </th>
                )}
              </tr>
            </thead>
            <tbody>
              {list.map((f) => {
                const shown = f.name.replace(/\.fseq$/i, "");
                return (
                  <tr key={f.name} className="border-t border-neutral-100 dark:border-neutral-800">
                    <td className="w-full max-w-0 truncate py-1 pr-2" title={f.name}>
                      {shown}
                    </td>
                    {folder === "playlists" && <td className="pl-3 text-right whitespace-nowrap text-neutral-600 tabular-nums dark:text-neutral-300">{f.items === null ? "—" : plural(f.items, "item")}</td>}
                    <td className="pl-3 text-right whitespace-nowrap tabular-nums">{f.durationMs ? clock(f.durationMs / 1000) : "—"}</td>
                    <td className="pl-3 text-right whitespace-nowrap text-neutral-600 tabular-nums dark:text-neutral-300">{f.sizeBytes === null ? "—" : sizeText(f.sizeBytes)}</td>
                    <td className="pl-3 whitespace-nowrap text-neutral-600 dark:text-neutral-300">{f.modified ? shortDate(f.modified) : "—"}</td>
                    {playable && (
                      <td className="py-1 pl-3 text-right">
                        <Button variant="ghost" aria-label={`Play ${shown}`} onClick={() => play(f.name, shown)} disabled={busy}>
                          <Play size={14} aria-hidden /> Play
                        </Button>
                      </td>
                    )}
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </div>
      {playError && (
        <p role="alert" className="text-red-600 dark:text-red-400">
          {playError}
        </p>
      )}
      {sending && (
        <SendToFppDialog
          source={{ kind: "file", path: sending.path }}
          title={sending.name}
          music={sending.audio}
          address={address}
          onClose={() => {
            setSending(null);
            setSent((n) => n + 1);
          }}
        />
      )}
    </Section>
  );
}
