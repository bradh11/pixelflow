import { CheckCircle2, Music, Play, Send } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { errorMessage } from "../api/backend";
import type { Controller, FppSendPlan, FppSendProgress, FppSendResult, PlaylistChoice, SendSource } from "../api/types";
import { fileName, shownPath } from "../lib/format";
import { type KnownDevice, useApp } from "../state/store";
import { Button, Input, Select } from "./ui";

/** An FPP the user can send to. */
export interface FppChoice {
  address: string;
  name: string;
}

/** The FPPs the show knows (its FPP controllers) and the ones found on the network, each once. */
export function fppChoices(controllers: Controller[] | undefined, devices: KnownDevice[] | undefined): FppChoice[] {
  const choices: FppChoice[] = [];
  const add = (address: string, name: string) => {
    if (address && !choices.some((c) => c.address === address)) choices.push({ address, name });
  };
  for (const c of controllers ?? []) if (c.adapter === "fpp") add(c.address, c.name);
  for (const d of devices ?? []) if (d.kind === "fpp") add(d.address, d.name);
  return choices;
}

/** The FPP last sent to, remembered on this computer. */
const LAST_FPP_KEY = "pixelflow.sendToFpp";

function lastFpp(): string | null {
  try {
    return localStorage.getItem(LAST_FPP_KEY);
  } catch {
    return null;
  }
}

function rememberFpp(address: string) {
  try {
    localStorage.setItem(LAST_FPP_KEY, address);
  } catch {
    // Storage unavailable: the next send asks again.
  }
}

function sizeText(bytes: number): string {
  if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(1)} GB`;
  if (bytes >= 1024 ** 2) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
  return `${Math.ceil(bytes / 1024)} KB`;
}

const STEP_TEXT: Record<FppSendProgress["step"], string> = {
  export: "Preparing the sequence for the FPP…",
  sequence: "Sending the sequence…",
  music: "Sending the music…",
  playlist: "Adding it to the playlist…",
};

/** Replace the FPP's file, keep both (send under another name), or (music only) use the FPP's copy. */
type Clash = "replace" | "keep" | "use";

type Phase =
  | { kind: "choose"; note?: string }
  | { kind: "sending"; progress: FppSendProgress | null }
  | { kind: "done"; result: FppSendResult; playing: "no" | "starting" | "yes"; playError?: string }
  | { kind: "failed"; message: string };

const OTHER = "__other__";

/**
 * Sends a sequence and its music to an FPP: pick the FPP, the music, and a playlist, then Send.
 * Nothing changes on the FPP until Send is clicked; playing it there is a further click.
 */
export function SendToFppDialog({
  source,
  title,
  music: initialMusic,
  address: initialAddress,
  onClose,
  onSent,
}: {
  source: SendSource;
  /** The sequence's name, for the heading. */
  title: string;
  /** The sequence's music on this computer, if any. */
  music: string | null;
  /** An FPP to start with (else the one last sent to, else the first known). */
  address?: string;
  onClose: () => void;
  onSent?: (address: string, result: FppSendResult) => void;
}) {
  const backend = useApp((s) => s.backend);
  const controllers = useApp((s) => s.snapshot?.show.controllers);
  const devices = useApp((s) => s.discovery?.devices);
  const [choices] = useState(() => fppChoices(controllers, devices));
  const [address, setAddress] = useState<string>(() => {
    const remembered = lastFpp();
    const start = initialAddress ?? (remembered && choices.some((c) => c.address === remembered) ? remembered : choices[0]?.address);
    return start ?? "";
  });
  const [typing, setTyping] = useState(choices.length === 0);
  const [typed, setTyped] = useState("");
  const target = typing ? typed.trim() : address;
  const [music, setMusic] = useState<string | null>(initialMusic);
  const [plan, setPlan] = useState<FppSendPlan | null>(null);
  const [planError, setPlanError] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);
  const [sequenceClash, setSequenceClash] = useState<Clash>("replace");
  const [musicClash, setMusicClash] = useState<Clash>("replace");
  const [playlistKind, setPlaylistKind] = useState<PlaylistChoice["kind"]>("none");
  const [existing, setExisting] = useState("");
  const [newName, setNewName] = useState("");
  const [phase, setPhase] = useState<Phase>({ kind: "choose" });
  const dialog = useRef<HTMLDivElement>(null);
  const fppName = choices.find((c) => c.address === target)?.name ?? target;
  const sending = phase.kind === "sending";

  // What's on the FPP, read again when the FPP or the music changes (reading changes nothing).
  const [checkTurn, setCheckTurn] = useState(0);
  useEffect(() => {
    setPlan(null);
    setPlanError(null);
    if (!backend || !target) return;
    let current = true;
    setChecking(true);
    const timer = setTimeout(
      () => {
        backend.fppSendPlan(target, source, music).then(
          (p) => {
            if (!current) return;
            setPlan(p);
            setExisting((name) => (p.playlists.includes(name) ? name : (p.playlists[0] ?? "")));
            setNewName((name) => name || p.newPlaylistName);
            setChecking(false);
          },
          (e) => {
            if (!current) return;
            setPlanError(errorMessage(e));
            setChecking(false);
          },
        );
      },
      typing ? 400 : 0,
    );
    return () => {
      current = false;
      clearTimeout(timer);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [backend, target, music, checkTurn]);

  // Escape closes (not while sending: Cancel stops the send first).
  useEffect(() => {
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    dialog.current?.querySelector<HTMLElement>("select, input, button")?.focus();
    return () => {
      if (opener?.isConnected) opener.focus();
    };
  }, []);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      if (!sending) onClose();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [sending, onClose]);

  const chooseMusic = async () => {
    const picked = await backend?.pickAudioPath();
    if (picked) setMusic(picked);
  };

  const playlist = (): PlaylistChoice => {
    if (playlistKind === "existing" && existing) return { kind: "existing", name: existing };
    if (playlistKind === "new" && newName.trim()) return { kind: "new", name: newName.trim() };
    return { kind: "none" };
  };

  const send = async () => {
    if (!backend || !plan || !target) return;
    rememberFpp(target);
    const sequenceName = plan.sequence.exists && sequenceClash === "keep" ? plan.sequence.keepBothName : plan.sequence.name;
    const musicName = plan.music ? (plan.music.exists && musicClash === "keep" ? plan.music.keepBothName : plan.music.name) : null;
    setPhase({ kind: "sending", progress: null });
    try {
      const result = await backend.fppSend(
        target,
        {
          source,
          music,
          sequenceName,
          musicName,
          uploadMusic: !(plan.music?.exists && musicClash === "use"),
          playlist: playlist(),
        },
        (progress) => setPhase((p) => (p.kind === "sending" ? { kind: "sending", progress } : p)),
      );
      setPhase({ kind: "done", result, playing: "no" });
      onSent?.(target, result);
    } catch (e) {
      const message = errorMessage(e);
      if (message === "The upload was cancelled.") {
        setPhase({ kind: "choose", note: "Sending was stopped. Send again when you're ready." });
        setCheckTurn((n) => n + 1);
      } else {
        setPhase({ kind: "failed", message });
      }
    }
  };

  const playNow = async (result: FppSendResult) => {
    if (!backend) return;
    setPhase({ kind: "done", result, playing: "starting" });
    try {
      await backend.fppStart(target, result.playName);
      setPhase({ kind: "done", result, playing: "yes" });
    } catch (e) {
      setPhase({ kind: "done", result, playing: "no", playError: errorMessage(e) });
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4">
      <div
        ref={dialog}
        role="dialog"
        aria-modal="true"
        aria-label="Send to FPP"
        className="flex max-h-[calc(100vh-2rem)] w-[34rem] max-w-full flex-col gap-4 overflow-auto rounded-lg border border-neutral-200 bg-white p-5 shadow-xl dark:border-neutral-800 dark:bg-neutral-900"
        onKeyDown={(e) => e.stopPropagation()}
      >
        <div>
          <h2 className="text-lg font-semibold">Send to FPP</h2>
          <p className="mt-1 text-sm text-neutral-600 dark:text-neutral-300">
            Puts <span className="font-medium">{title}</span> and its music on your FPP, so it can play there without this computer.
          </p>
        </div>

        {(phase.kind === "choose" || phase.kind === "failed") && (
          <>
            <FppPicker
              choices={choices}
              address={address}
              typing={typing}
              typed={typed}
              onPick={(value) => {
                if (value === OTHER) setTyping(true);
                else {
                  setTyping(false);
                  setAddress(value);
                }
              }}
              onType={setTyped}
            />

            <div className="flex flex-col gap-1 text-sm">
              <span className="text-neutral-600 dark:text-neutral-400">Music</span>
              <div className="flex flex-wrap items-center gap-2">
                <Music size={14} className="text-neutral-400" aria-hidden />
                {music ? (
                  <span className="min-w-0 truncate" title={shownPath(music)}>
                    {fileName(music)}
                  </span>
                ) : (
                  <span className="text-neutral-500">No music (lights only)</span>
                )}
                <Button variant="ghost" onClick={() => void chooseMusic()}>
                  {music ? "Change…" : "Choose music…"}
                </Button>
                {music && (
                  <Button variant="ghost" onClick={() => setMusic(null)}>
                    No music
                  </Button>
                )}
              </div>
            </div>

            {checking && (
              <p role="status" className="text-sm text-neutral-500">
                Checking {fppName}…
              </p>
            )}
            {planError && (
              <div role="alert" className="flex items-start justify-between gap-2 rounded-md bg-red-50 p-2 text-sm text-red-700 dark:bg-red-950/40 dark:text-red-300">
                <span>{planError}</span>
                <Button variant="ghost" onClick={() => setCheckTurn((n) => n + 1)}>
                  Try again
                </Button>
              </div>
            )}

            {plan && (
              <>
                {plan.sequence.exists && (
                  <ClashChoice
                    label={`${fppName} already has a sequence called ${plan.sequence.name}.`}
                    keepBothName={plan.sequence.keepBothName}
                    value={sequenceClash}
                    onChange={setSequenceClash}
                  />
                )}
                {plan.music?.exists && (
                  <ClashChoice
                    label={`${fppName} already has music called ${plan.music.name}.`}
                    keepBothName={plan.music.keepBothName}
                    value={musicClash}
                    onChange={setMusicClash}
                    offerUse
                  />
                )}
                <PlaylistPicker
                  playlists={plan.playlists}
                  kind={playlistKind}
                  existing={existing}
                  newName={newName}
                  onKind={setPlaylistKind}
                  onExisting={setExisting}
                  onNewName={setNewName}
                />
                {plan.freeBytes !== null && <p className="text-xs text-neutral-500">{fppName} has {sizeText(plan.freeBytes)} free.</p>}
              </>
            )}

            {phase.kind === "choose" && phase.note && (
              <p role="status" className="text-sm text-amber-700 dark:text-amber-400">
                {phase.note}
              </p>
            )}
            {phase.kind === "failed" && (
              <p role="alert" className="text-sm text-red-600 dark:text-red-400">
                {phase.message}
              </p>
            )}

            <div className="flex justify-end gap-2">
              <Button onClick={onClose}>Close</Button>
              <Button variant="primary" disabled={!plan || checking} onClick={() => void send()}>
                <Send size={14} /> {phase.kind === "failed" ? "Try again" : "Send"}
              </Button>
            </div>
          </>
        )}

        {phase.kind === "sending" && <Sending progress={phase.progress} fppName={fppName} onCancel={() => void backend?.cancelFppSend()} />}

        {phase.kind === "done" && (
          <div className="flex flex-col gap-3">
            <p role="status" className="flex items-start gap-2 text-sm">
              <CheckCircle2 size={16} className="mt-0.5 shrink-0 text-green-600 dark:text-green-400" />
              <span>
                {phase.result.sequenceName}
                {phase.result.musicName ? ` and ${phase.result.musicName}` : ""} are on {fppName}
                {phase.result.playlist ? `, on the playlist “${phase.result.playlist}”` : ""}.
              </span>
            </p>
            {phase.result.notes.map((note) => (
              <p key={note} className="text-sm text-amber-700 dark:text-amber-400">
                {note}
              </p>
            ))}
            {phase.playError && (
              <p role="alert" className="text-sm text-red-600 dark:text-red-400">
                {phase.playError}
              </p>
            )}
            {phase.playing === "yes" && <p className="text-sm text-green-700 dark:text-green-400">Playing on {fppName}.</p>}
            <div className="flex justify-end gap-2">
              <Button onClick={onClose}>Close</Button>
              {phase.playing !== "yes" && (
                <Button
                  variant="primary"
                  disabled={phase.playing === "starting"}
                  title={`Starts ${phase.result.playName} on ${fppName} now`}
                  onClick={() => void playNow(phase.result)}
                >
                  <Play size={14} /> {phase.playing === "starting" ? "Starting…" : "Play it now on the FPP"}
                </Button>
              )}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function FppPicker({
  choices,
  address,
  typing,
  typed,
  onPick,
  onType,
}: {
  choices: FppChoice[];
  address: string;
  typing: boolean;
  typed: string;
  onPick: (value: string) => void;
  onType: (value: string) => void;
}) {
  return (
    <div className="flex flex-col gap-1 text-sm">
      <label htmlFor="send-fpp" className="text-neutral-600 dark:text-neutral-400">
        FPP
      </label>
      {choices.length > 0 && (
        <Select id="send-fpp" value={typing ? OTHER : address} onChange={(e) => onPick(e.target.value)}>
          {choices.map((c) => (
            <option key={c.address} value={c.address}>
              {c.name === c.address ? c.address : `${c.name} (${c.address})`}
            </option>
          ))}
          <option value={OTHER}>Another address…</option>
        </Select>
      )}
      {typing && (
        <Input
          id={choices.length ? undefined : "send-fpp"}
          aria-label="FPP address"
          placeholder="e.g. 192.168.1.50 or fpp.local"
          value={typed}
          onChange={(e) => onType(e.target.value)}
        />
      )}
      {choices.length === 0 && (
        <span className="text-xs text-neutral-500">Type its address, or find it first on the Devices screen.</span>
      )}
    </div>
  );
}

function ClashChoice({
  label,
  keepBothName,
  value,
  onChange,
  offerUse,
}: {
  label: string;
  keepBothName: string;
  value: Clash;
  onChange: (value: Clash) => void;
  /** Music: the FPP's copy may be the same song. */
  offerUse?: boolean;
}) {
  return (
    <fieldset className="flex flex-col gap-1 rounded-md border border-amber-300 bg-amber-50 p-2 text-sm dark:border-amber-800 dark:bg-amber-950/30">
      <legend className="px-1 font-medium text-amber-800 dark:text-amber-300">{label}</legend>
      <label className="flex items-center gap-2">
        <input type="radio" checked={value === "replace"} onChange={() => onChange("replace")} /> Replace it
      </label>
      <label className="flex items-center gap-2">
        <input type="radio" checked={value === "keep"} onChange={() => onChange("keep")} /> Keep both: send this one as {keepBothName}
      </label>
      {offerUse && (
        <label className="flex items-center gap-2">
          <input type="radio" checked={value === "use"} onChange={() => onChange("use")} /> Use the one already on the FPP
        </label>
      )}
    </fieldset>
  );
}

function PlaylistPicker({
  playlists,
  kind,
  existing,
  newName,
  onKind,
  onExisting,
  onNewName,
}: {
  playlists: string[];
  kind: PlaylistChoice["kind"];
  existing: string;
  newName: string;
  onKind: (kind: PlaylistChoice["kind"]) => void;
  onExisting: (name: string) => void;
  onNewName: (name: string) => void;
}) {
  const taken = kind === "new" && playlists.includes(newName.trim());
  return (
    <fieldset className="flex flex-col gap-1.5 text-sm">
      <legend className="mb-1 text-neutral-600 dark:text-neutral-400">FPP playlist</legend>
      <label className="flex items-center gap-2">
        <input type="radio" checked={kind === "none"} onChange={() => onKind("none")} /> Don't add it to a playlist
      </label>
      <label className="flex flex-wrap items-center gap-2">
        <input type="radio" checked={kind === "existing"} disabled={playlists.length === 0} onChange={() => onKind("existing")} /> Add it to
        {playlists.length > 0 ? (
          <Select
            aria-label="Playlist"
            value={existing}
            onChange={(e) => {
              onExisting(e.target.value);
              onKind("existing");
            }}
          >
            {playlists.map((p) => (
              <option key={p} value={p}>
                {p}
              </option>
            ))}
          </Select>
        ) : (
          <span className="text-neutral-500">a playlist (the FPP has none yet)</span>
        )}
      </label>
      <label className="flex flex-wrap items-center gap-2">
        <input type="radio" checked={kind === "new"} onChange={() => onKind("new")} /> A new playlist called
        <Input
          aria-label="New playlist name"
          value={newName}
          onChange={(e) => {
            onNewName(e.target.value);
            onKind("new");
          }}
        />
      </label>
      {taken && <span className="text-xs text-neutral-500">There's already a playlist called {newName.trim()}; it'll be added to that one.</span>}
      <span className="text-xs text-neutral-500">A playlist is what FPP's scheduler plays at set times.</span>
    </fieldset>
  );
}

function Sending({ progress, fppName, onCancel }: { progress: FppSendProgress | null; fppName: string; onCancel: () => void }) {
  const percent = progress?.percent ?? 0;
  const bytes = progress && (progress.step === "sequence" || progress.step === "music") && progress.total > 0;
  return (
    <div className="flex flex-col gap-3">
      <p role="status" className="text-sm">
        {progress ? STEP_TEXT[progress.step] : `Getting ready to send to ${fppName}…`}
      </p>
      <progress aria-label="Sending to the FPP" className="w-full accent-violet-600" max={100} value={percent} />
      <p className="text-xs text-neutral-500 tabular-nums">
        {percent}%{bytes ? ` · ${sizeText(progress.done)} of ${sizeText(progress.total)}` : ""}
      </p>
      <div className="flex justify-end">
        <Button onClick={onCancel}>Cancel</Button>
      </div>
    </div>
  );
}
