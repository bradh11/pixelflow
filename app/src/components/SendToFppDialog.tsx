import { CheckCircle2, Music, Play, Send } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { errorMessage } from "../api/backend";
import type { Controller, FppSendPlan, FppSendProgress, FppSendResult, NameCheck, PlaylistChoice, SendSource } from "../api/types";
import { fileName, shownPath, sizeText } from "../lib/format";
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

/** Between an error and the FPP's own words for it (as the shell writes errors). */
const DETAIL = "\n\nFPP said: ";

/** An error, with the FPP's own words (if any) under a Details disclosure. */
function ErrorText({ message }: { message: string }) {
  const [text, detail] = message.split(DETAIL);
  return (
    <div role="alert" className="flex flex-col gap-1 text-sm text-red-600 dark:text-red-400">
      <p>{text}</p>
      {detail && (
        <details className="text-xs text-neutral-600 dark:text-neutral-400">
          <summary className="cursor-pointer">Details</summary>
          <p className="mt-1 break-words">{detail}</p>
        </details>
      )}
    </div>
  );
}

const STEP_TEXT: Record<FppSendProgress["step"], string> = {
  export: "Preparing the sequence for the FPP…",
  sequence: "Sending the sequence…",
  music: "Sending the music…",
  commit: "Finishing on the FPP…",
  playlist: "Finishing on the FPP…",
};

/** Replace the FPP's file, keep both (send under another name), or (music only) use the FPP's
 * copy; null until the user picks. */
type Clash = "replace" | "keep" | "use" | null;

/** "Play it now" waits for a yes when the FPP is busy or has a show coming up. */
type Playing = "no" | "checking" | { ask: string } | "starting" | "yes";

type Phase =
  | { kind: "choose"; error?: string }
  | { kind: "sending"; progress: FppSendProgress | null }
  | { kind: "done"; result: FppSendResult; playing: Playing; playError?: string };

const OTHER = "__other__";
let nextSendId = 1;

/**
 * Sends a sequence and its music to an FPP: pick the FPP, the music, and a playlist, then Send.
 * Nothing changes on the FPP until Send is clicked, nothing there is replaced unless the user
 * picks Replace, and playing it there is a further click.
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
  /** The typed address, once the user asks for it to be checked. */
  const [checkedTyped, setCheckedTyped] = useState("");
  const target = typing ? checkedTyped : address;
  const [music, setMusic] = useState<string | null>(initialMusic);
  const [plan, setPlan] = useState<FppSendPlan | null>(null);
  const [planError, setPlanError] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);
  const [sequenceClash, setSequenceClash] = useState<Clash>(null);
  const [musicClash, setMusicClash] = useState<Clash>(null);
  const [playlistKind, setPlaylistKind] = useState<PlaylistChoice["kind"]>("none");
  const [existing, setExisting] = useState("");
  const [newName, setNewName] = useState("");
  const [phase, setPhase] = useState<Phase>({ kind: "choose" });
  const dialog = useRef<HTMLDivElement>(null);
  const cancelButton = useRef<HTMLButtonElement>(null);
  const doneButton = useRef<HTMLButtonElement>(null);
  const fppName = choices.find((c) => c.address === target)?.name ?? target;
  const committing = phase.kind === "sending" && (phase.progress?.step === "commit" || phase.progress?.step === "playlist");
  const sendingRef = useRef(false);
  sendingRef.current = phase.kind === "sending";
  const cancel = () => void backend?.cancelFppSend();

  // What's on the FPP, read again when the FPP or the music changes, or after a failed send
  // (reading changes nothing). Clash choices start empty each time.
  const [checkTurn, setCheckTurn] = useState(0);
  useEffect(() => {
    setPlan(null);
    setPlanError(null);
    setSequenceClash(null);
    setMusicClash(null);
    if (!backend || !target) return;
    let current = true;
    setChecking(true);
    backend.fppSendPlan(target, source, music).then(
      (p) => {
        if (!current) return;
        setPlan(p);
        setExisting((name) => (p.playlists.includes(name) ? name : ""));
        setNewName((name) => name || p.newPlaylistName);
        setChecking(false);
      },
      (e) => {
        if (!current) return;
        setPlanError(errorMessage(e));
        setChecking(false);
      },
    );
    return () => {
      current = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [backend, target, music, checkTurn]);

  useEffect(() => {
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    dialog.current?.querySelector<HTMLElement>("select, input, button")?.focus();
    return () => {
      // Gone mid-send (a screen change): stop it before anything is replaced.
      if (sendingRef.current) cancel();
      if (opener?.isConnected) opener.focus();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  // Escape closes; while sending it cancels instead (until files start moving into place).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      if (phase.kind === "sending") {
        if (!committing) cancel();
      } else onClose();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [phase.kind, committing, onClose]);
  // Focus follows the dialog's step.
  useEffect(() => {
    if (phase.kind === "sending") cancelButton.current?.focus();
    if (phase.kind === "done") doneButton.current?.focus();
  }, [phase.kind]);

  const chooseMusic = async () => {
    const picked = await backend?.pickAudioPath();
    if (picked) setMusic(picked);
  };

  const newNameTaken = plan?.playlists.find((p) => p.toLowerCase() === newName.trim().toLowerCase()) ?? null;
  const playlist = (): PlaylistChoice | null => {
    if (playlistKind === "existing") return existing ? { kind: "existing", name: existing } : null;
    if (playlistKind === "new") return newName.trim() && !newNameTaken ? { kind: "new", name: newName.trim() } : null;
    return { kind: "none" };
  };
  const clashesChosen = Boolean(plan) && (!plan!.sequence.exists || sequenceClash !== null) && (!plan!.music?.exists || musicClash !== null);
  const ready = Boolean(plan) && !checking && clashesChosen && playlist() !== null;

  /** The name to send under: the FPP's own spelling to replace or reuse, else ours or keep-both. */
  const nameFor = (check: NameCheck, clash: Clash) =>
    !check.exists ? check.name : clash === "keep" ? check.keepBothName : (check.fppName ?? check.name);

  const send = async () => {
    const choice = playlist();
    if (!backend || !plan || !target || !ready || !choice) return;
    rememberFpp(target);
    const sendId = nextSendId++;
    setPhase({ kind: "sending", progress: null });
    try {
      const result = await backend.fppSend(
        target,
        {
          source,
          music,
          sequenceName: nameFor(plan.sequence, sequenceClash),
          musicName: plan.music ? nameFor(plan.music, musicClash) : null,
          uploadMusic: !(plan.music?.exists && musicClash === "use"),
          replaceSequence: plan.sequence.exists && sequenceClash === "replace",
          replaceMusic: Boolean(plan.music?.exists && musicClash === "replace"),
          playlist: choice,
          sendId,
        },
        (progress) => setPhase((p) => (p.kind === "sending" ? { kind: "sending", progress } : p)),
      );
      setPhase({ kind: "done", result, playing: "no" });
      onSent?.(target, result);
    } catch (e) {
      // Read the FPP again: what's there may have changed, and nothing is pre-chosen.
      setPhase({ kind: "choose", error: errorMessage(e) });
      setCheckTurn((n) => n + 1);
    }
  };

  const start = async (result: FppSendResult) => {
    if (!backend) return;
    setPhase({ kind: "done", result, playing: "starting" });
    try {
      await backend.fppStart(target, result.playName);
      setPhase({ kind: "done", result, playing: "yes" });
    } catch (e) {
      setPhase({ kind: "done", result, playing: "no", playError: errorMessage(e) });
    }
  };

  /** Checks what the FPP is doing first: starting replaces whatever is playing. */
  const playNow = async (result: FppSendResult) => {
    if (!backend) return;
    setPhase({ kind: "done", result, playing: "checking" });
    let status;
    try {
      status = await backend.fppStatus(target);
    } catch (e) {
      setPhase({ kind: "done", result, playing: "no", playError: `Couldn't check what ${fppName} is doing: ${errorMessage(e)}` });
      return;
    }
    if (status.state === "playing" || status.state === "paused") {
      const current = status.playlist ?? status.sequence ?? "what it's playing";
      setPhase({ kind: "done", result, playing: { ask: `Stop ${current} and play this now?` } });
    } else if (status.nextPlaylist) {
      const when = status.nextStart ? ` (${status.nextStart})` : "";
      setPhase({ kind: "done", result, playing: { ask: `${status.nextPlaylist} is scheduled${when}. Play this now anyway?` } });
    } else {
      await start(result);
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

        {phase.kind === "choose" && (
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
              onCheck={() => {
                setCheckedTyped(typed.trim());
                setCheckTurn((n) => n + 1);
              }}
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
              <div className="flex items-start justify-between gap-2 rounded-md bg-red-50 p-2 dark:bg-red-950/40">
                <ErrorText message={planError} />
                <Button variant="ghost" onClick={() => setCheckTurn((n) => n + 1)}>
                  Check again
                </Button>
              </div>
            )}

            {plan && (
              <>
                {plan.sequence.exists && (
                  <ClashChoice
                    group="send-sequence-clash"
                    check={plan.sequence}
                    label={`${fppName} already has a sequence called ${plan.sequence.fppName ?? plan.sequence.name}.`}
                    replaceNote={`Playlists on the FPP that play ${plan.sequence.fppName ?? plan.sequence.name} will play this one instead.`}
                    value={sequenceClash}
                    onChange={setSequenceClash}
                  />
                )}
                {plan.music?.exists && (
                  <ClashChoice
                    group="send-music-clash"
                    check={plan.music}
                    label={`${fppName} already has music called ${plan.music.fppName ?? plan.music.name}.`}
                    replaceNote={`Anything on the FPP that uses ${plan.music.fppName ?? plan.music.name} will play this file instead.`}
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
                  newNameTaken={newNameTaken}
                  onKind={setPlaylistKind}
                  onExisting={setExisting}
                  onNewName={setNewName}
                />
                {plan.layoutWarnings.length > 0 && (
                  <div className="rounded-md border border-amber-300 bg-amber-50 p-2 text-sm text-amber-800 dark:border-amber-800 dark:bg-amber-950/30 dark:text-amber-300">
                    <p className="font-medium">The channels may not match {fppName}'s outputs:</p>
                    <ul className="mt-1 list-disc pl-5">
                      {plan.layoutWarnings.map((w) => (
                        <li key={w}>{w}</li>
                      ))}
                    </ul>
                    <p className="mt-1 text-xs">You can still send it.</p>
                  </div>
                )}
                <p className="text-xs text-neutral-500">
                  {plan.freeBytes !== null ? `${fppName} has ${sizeText(plan.freeBytes)} free.` : "Couldn't read the FPP's free space."}
                </p>
              </>
            )}

            {phase.error && <ErrorText message={phase.error} />}

            <div className="flex justify-end gap-2">
              <Button onClick={onClose}>Close</Button>
              <Button variant="primary" disabled={!ready} onClick={() => void send()}>
                <Send size={14} /> {phase.error ? "Try again" : "Send"}
              </Button>
            </div>
          </>
        )}

        {phase.kind === "sending" && (
          <Sending progress={phase.progress} fppName={fppName} committing={committing} cancelRef={cancelButton} onCancel={cancel} />
        )}

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
            {phase.playError && <ErrorText message={phase.playError} />}
            {phase.playing === "yes" && <p className="text-sm text-green-700 dark:text-green-400">Playing on {fppName}.</p>}
            {typeof phase.playing === "object" ? (
              <div className="flex flex-col gap-2 rounded-md border border-amber-300 bg-amber-50 p-2 text-sm dark:border-amber-800 dark:bg-amber-950/30">
                <p>{phase.playing.ask}</p>
                <div className="flex justify-end gap-2">
                  <Button ref={doneButton} onClick={() => setPhase({ kind: "done", result: phase.result, playing: "no" })}>
                    Not now
                  </Button>
                  <Button variant="primary" onClick={() => void start(phase.result)}>
                    <Play size={14} /> Stop it and play this
                  </Button>
                </div>
              </div>
            ) : (
              <div className="flex justify-end gap-2">
                <Button onClick={onClose}>Close</Button>
                {phase.playing !== "yes" && (
                  <Button
                    ref={doneButton}
                    variant="primary"
                    disabled={phase.playing === "starting" || phase.playing === "checking"}
                    title={`Starts ${phase.result.playName} on ${fppName} now`}
                    onClick={() => void playNow(phase.result)}
                  >
                    <Play size={14} /> {phase.playing === "no" ? "Play it now on the FPP" : "Starting…"}
                  </Button>
                )}
              </div>
            )}
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
  onCheck,
}: {
  choices: FppChoice[];
  address: string;
  typing: boolean;
  typed: string;
  onPick: (value: string) => void;
  onType: (value: string) => void;
  /** Check the typed address (nothing is contacted while typing). */
  onCheck: () => void;
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
        <div className="flex gap-2">
          <Input
            id={choices.length ? undefined : "send-fpp"}
            aria-label="FPP address"
            className="flex-1"
            placeholder="e.g. 192.168.1.50 or fpp.local"
            value={typed}
            onChange={(e) => onType(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && typed.trim()) onCheck();
            }}
          />
          <Button disabled={!typed.trim()} onClick={onCheck}>
            Check
          </Button>
        </div>
      )}
      {choices.length === 0 && (
        <span className="text-xs text-neutral-500">Type its address and press Check, or find it first on the Controllers screen.</span>
      )}
    </div>
  );
}

function ClashChoice({
  group,
  check,
  label,
  replaceNote,
  value,
  onChange,
  offerUse,
}: {
  /** The radio group's name. */
  group: string;
  check: NameCheck;
  label: string;
  /** What Replace affects. */
  replaceNote: string;
  value: Clash;
  onChange: (value: Clash) => void;
  /** Music: the FPP's copy may be the same song. */
  offerUse?: boolean;
}) {
  const otherCapitals = check.fppName !== null && check.fppName !== check.name;
  return (
    <fieldset
      role="radiogroup"
      aria-label={label}
      className="flex flex-col gap-1 rounded-md border border-amber-300 bg-amber-50 p-2 text-sm dark:border-amber-800 dark:bg-amber-950/30"
    >
      <legend className="px-1 font-medium text-amber-800 dark:text-amber-300">{label}</legend>
      {otherCapitals && (
        <p className="text-xs text-amber-800 dark:text-amber-300">
          It's spelled {check.fppName} there (different capitals from {check.name}).
        </p>
      )}
      <p className="text-xs text-neutral-600 dark:text-neutral-400">Choose what to do:</p>
      <label className="flex items-center gap-2">
        <input type="radio" name={group} checked={value === "replace"} onChange={() => onChange("replace")} /> Replace it
      </label>
      <p className="ml-6 text-xs text-amber-800 dark:text-amber-300">{replaceNote}</p>
      <label className="flex items-center gap-2">
        <input type="radio" name={group} checked={value === "keep"} onChange={() => onChange("keep")} /> Keep both: send this one as{" "}
        {check.keepBothName}
      </label>
      {offerUse && (
        <label className="flex items-center gap-2">
          <input type="radio" name={group} checked={value === "use"} onChange={() => onChange("use")} /> Use the one already on the FPP
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
  newNameTaken,
  onKind,
  onExisting,
  onNewName,
}: {
  playlists: string[];
  kind: PlaylistChoice["kind"];
  existing: string;
  newName: string;
  /** The FPP playlist the new name matches, whatever its capitals. */
  newNameTaken: string | null;
  onKind: (kind: PlaylistChoice["kind"]) => void;
  onExisting: (name: string) => void;
  onNewName: (name: string) => void;
}) {
  return (
    <fieldset className="flex flex-col gap-1.5 text-sm">
      <legend className="mb-1 text-neutral-600 dark:text-neutral-400">FPP playlist</legend>
      <label className="flex items-center gap-2">
        <input type="radio" name="send-playlist" checked={kind === "none"} onChange={() => onKind("none")} /> Don't add it to a playlist
      </label>
      <label className="flex flex-wrap items-center gap-2">
        <input type="radio" name="send-playlist" checked={kind === "existing"} disabled={playlists.length === 0} onChange={() => onKind("existing")} /> Add it
        to
        {playlists.length > 0 ? (
          <Select
            aria-label="Playlist"
            value={existing}
            onChange={(e) => {
              onExisting(e.target.value);
              onKind("existing");
            }}
          >
            <option value="">Choose a playlist…</option>
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
        <input type="radio" name="send-playlist" checked={kind === "new"} onChange={() => onKind("new")} /> A new playlist called
        <Input
          aria-label="New playlist name"
          value={newName}
          onChange={(e) => {
            onNewName(e.target.value);
            onKind("new");
          }}
        />
      </label>
      {kind === "new" && newNameTaken && (
        <span className="text-xs text-red-600 dark:text-red-400">
          The FPP already has a playlist called {newNameTaken}. Choose it under "Add it to", or pick another name.
        </span>
      )}
      <span className="text-xs text-neutral-500">A playlist is what FPP's scheduler plays at set times.</span>
    </fieldset>
  );
}

function Sending({
  progress,
  fppName,
  committing,
  cancelRef,
  onCancel,
}: {
  progress: FppSendProgress | null;
  fppName: string;
  /** Files are moving into place: too late to cancel. */
  committing: boolean;
  cancelRef: React.Ref<HTMLButtonElement>;
  onCancel: () => void;
}) {
  const percent = committing ? 100 : (progress?.percent ?? 0);
  const bytes = progress && (progress.step === "sequence" || progress.step === "music") && progress.total > 0;
  return (
    <div className="flex flex-col gap-3">
      <p role="status" className="text-sm">
        {progress ? STEP_TEXT[progress.step] : `Getting ready to send to ${fppName}…`}
      </p>
      <progress aria-label="Sending to the FPP" className="w-full accent-violet-600" max={100} value={percent} />
      <p className="text-xs text-neutral-500 tabular-nums">
        {committing ? "Moving the files into place. This can't be cancelled now." : `${percent}%`}
        {bytes ? ` · ${sizeText(progress.done)} of ${sizeText(progress.total)}` : ""}
      </p>
      <div className="flex justify-end">
        <Button ref={cancelRef} disabled={committing} onClick={onCancel} title={committing ? "Too late to cancel: the files are moving into place" : "Stop sending; nothing on the FPP changes"}>
          Cancel
        </Button>
      </div>
    </div>
  );
}
