import { AlertTriangle, CheckCircle2, Download, FolderOpen, ListPlus } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { errorMessage } from "../../api/backend";
import type { DownloadClash, FppDownloadName, FppDownloadPlan, FppDownloadProgress, FppDownloadResult } from "../../api/types";
import { fileName, shownPath, sizeText } from "../../lib/format";
import { useApp } from "../../state/store";
import { Button } from "../ui";

type Phase =
  | { kind: "folder"; error?: string }
  | { kind: "choose"; error?: string }
  | { kind: "downloading"; progress: FppDownloadProgress | null }
  | { kind: "done"; result: FppDownloadResult; added: boolean };

let nextDownloadId = 1;

/** A file of that name is already in the folder: Replace it or Keep both (Cancel closes). */
function ClashChoice({
  group,
  file,
  what,
  value,
  onChange,
}: {
  group: string;
  file: FppDownloadName;
  what: string;
  value: DownloadClash | null;
  onChange: (value: DownloadClash) => void;
}) {
  const label = `There's already ${what} called ${file.name} in ${fileName(file.folder)}.`;
  return (
    <fieldset
      role="radiogroup"
      aria-label={label}
      className="flex flex-col gap-1 rounded-md border border-amber-300 bg-amber-50 p-2 text-sm dark:border-amber-800 dark:bg-amber-950/30"
    >
      <legend className="px-1 font-medium text-amber-800 dark:text-amber-300">{label}</legend>
      <p className="text-xs text-neutral-600 dark:text-neutral-400">Choose what to do:</p>
      <label className="flex items-center gap-2">
        <input type="radio" name={group} checked={value === "replace"} onChange={() => onChange("replace")} /> Replace it
      </label>
      <label className="flex items-center gap-2">
        <input type="radio" name={group} checked={value === "keepBoth"} onChange={() => onChange("keepBoth")} /> Keep both: save this one as {file.keepBothName}
      </label>
    </fieldset>
  );
}

/**
 * Downloads a sequence and the music it names from an FPP into the show's folder ("sequences"
 * and "music"), or a folder the user picks while the show isn't saved. Only reads from the FPP;
 * nothing on this computer is replaced unless the user picks Replace; then offers Add to Play.
 */
export function FppDownloadDialog({ address, fppName, sequence, onClose }: { address: string; fppName: string; sequence: string; onClose: () => void }) {
  const backend = useApp((s) => s.backend);
  const saved = useApp((s) => Boolean(s.snapshot?.path));
  const [folder, setFolder] = useState<string | null>(null);
  const [plan, setPlan] = useState<FppDownloadPlan | null>(null);
  const [checkTurn, setCheckTurn] = useState(0);
  const [sequenceClash, setSequenceClash] = useState<DownloadClash | null>(null);
  const [musicClash, setMusicClash] = useState<DownloadClash | null>(null);
  const [phase, setPhase] = useState<Phase>(saved ? { kind: "choose" } : { kind: "folder" });
  const dialog = useRef<HTMLDivElement>(null);
  const cancelButton = useRef<HTMLButtonElement>(null);
  const doneButton = useRef<HTMLButtonElement>(null);
  const downloadingRef = useRef(false);
  downloadingRef.current = phase.kind === "downloading";
  const shown = sequence.replace(/\.fseq$/i, "");
  const cancel = () => void backend?.cancelFppDownload();

  // What will be saved where, read again after a failed download (reading changes nothing).
  // Clash choices start empty each time.
  const ready = saved || folder !== null;
  useEffect(() => {
    setPlan(null);
    setSequenceClash(null);
    setMusicClash(null);
    if (!backend || !ready) return;
    let current = true;
    backend.fppDownloadPlan(address, sequence, folder).then(
      (p) => current && setPlan(p),
      (e) => current && setPhase((p) => (p.kind === "choose" ? { kind: "choose", error: errorMessage(e) } : p)),
    );
    return () => {
      current = false;
    };
  }, [backend, address, sequence, folder, ready, checkTurn]);

  useEffect(() => {
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    dialog.current?.querySelector<HTMLElement>("button, input")?.focus();
    return () => {
      // Gone mid-download (a screen change): stop it; nothing it fetched is kept.
      if (downloadingRef.current) cancel();
      if (opener?.isConnected) opener.focus();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  // Escape closes; while downloading it cancels instead.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      if (phase.kind === "downloading") cancel();
      else onClose();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [phase.kind, onClose]);
  useEffect(() => {
    if (phase.kind === "downloading") cancelButton.current?.focus();
    if (phase.kind === "done") doneButton.current?.focus();
  }, [phase.kind]);

  const chooseFolder = async () => {
    if (!backend) return;
    try {
      const picked = await backend.pickDownloadFolder();
      if (!picked) return;
      setFolder(picked);
      setPhase({ kind: "choose" });
    } catch (e) {
      setPhase({ kind: "folder", error: errorMessage(e) });
    }
  };

  const clashesChosen = Boolean(plan) && (!plan!.sequence.exists || sequenceClash !== null) && (!plan!.music?.exists || musicClash !== null);

  const download = async () => {
    if (!backend || !plan || !clashesChosen) return;
    const downloadId = nextDownloadId++;
    setPhase({ kind: "downloading", progress: null });
    try {
      const result = await backend.fppDownload(
        address,
        {
          sequence: plan.sequence.name,
          music: plan.music?.name ?? null,
          folder,
          sequenceClash: plan.sequence.exists ? sequenceClash : null,
          musicClash: plan.music?.exists ? musicClash : null,
          downloadId,
        },
        (progress) => setPhase((p) => (p.kind === "downloading" ? { kind: "downloading", progress } : p)),
      );
      setPhase({ kind: "done", result, added: false });
    } catch (e) {
      setPhase({ kind: "choose", error: errorMessage(e) });
      setCheckTurn((n) => n + 1);
    }
  };

  // Into the show's sequence library with its music, as one undo step (errors show as usual).
  const addToPlay = async (result: FppDownloadResult) => {
    if (await useApp.getState().run((b) => b.addSequence(result.sequencePath, result.musicPath))) {
      setPhase({ kind: "done", result, added: true });
    }
  };

  const importXsq = () => {
    onClose();
    void useApp.getState().importXlightsSequence();
  };

  const progress = phase.kind === "downloading" ? phase.progress : null;
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4">
      <div
        ref={dialog}
        role="dialog"
        aria-modal="true"
        aria-label="Download from FPP"
        className="flex max-h-[calc(100vh-2rem)] w-[34rem] max-w-full flex-col gap-4 overflow-auto rounded-lg border border-neutral-200 bg-white p-5 text-sm shadow-xl dark:border-neutral-800 dark:bg-neutral-900"
        onKeyDown={(e) => e.stopPropagation()}
      >
        <div>
          <h2 className="text-lg font-semibold">Download from FPP</h2>
          <p className="mt-1 text-neutral-600 dark:text-neutral-300">
            Saves <span className="font-medium">{shown}</span> and its music from {fppName} to this computer. Nothing on the FPP changes.
          </p>
        </div>

        {phase.kind === "folder" && (
          <>
            <p>Your show isn't saved yet, so choose a folder to save the sequence in. It goes in a "sequences" folder there, and its music in a "music" folder.</p>
            {phase.error && (
              <p role="alert" className="text-red-600 dark:text-red-400">
                {phase.error}
              </p>
            )}
            <div className="flex justify-end gap-2">
              <Button onClick={onClose}>Cancel</Button>
              <Button variant="primary" onClick={chooseFolder}>
                <FolderOpen size={14} aria-hidden /> Choose folder…
              </Button>
            </div>
          </>
        )}

        {phase.kind === "choose" && (
          <>
            {phase.error && (
              <p role="alert" className="text-red-600 dark:text-red-400">
                {phase.error}
              </p>
            )}
            {!plan && !phase.error && <p className="text-neutral-500">Reading the sequence on {fppName}…</p>}
            {plan && (
              <>
                <ul aria-label="What will be saved" className="flex flex-col gap-1">
                  <li>
                    <span className="font-medium">{plan.sequence.name}</span>
                    {plan.sequence.sizeBytes !== null && <span className="text-neutral-500"> · {sizeText(plan.sequence.sizeBytes)}</span>}
                    <span className="block text-xs break-all text-neutral-500">to {shownPath(plan.sequence.folder)}</span>
                  </li>
                  {plan.music && (
                    <li>
                      <span className="font-medium">{plan.music.name}</span>
                      {plan.music.sizeBytes !== null && <span className="text-neutral-500"> · {sizeText(plan.music.sizeBytes)}</span>}
                      <span className="block text-xs break-all text-neutral-500">to {shownPath(plan.music.folder)}</span>
                    </li>
                  )}
                </ul>
                {!plan.music && (
                  <p className="text-neutral-600 dark:text-neutral-300">
                    {plan.missingMusic
                      ? `No music: the sequence names ${plan.missingMusic}, which isn't on ${fppName}. You can choose music for it on the Play screen.`
                      : "No music: the sequence doesn't name any."}
                  </p>
                )}
                {plan.channelWarning && (
                  <p className="flex gap-2 rounded-md bg-amber-50 p-2 text-amber-900 dark:bg-amber-950/30 dark:text-amber-200">
                    <AlertTriangle size={16} className="mt-0.5 shrink-0" aria-hidden />
                    <span>{plan.channelWarning}</span>
                  </p>
                )}
                {plan.sequence.exists && <ClashChoice group="download-sequence-clash" file={plan.sequence} what="a sequence" value={sequenceClash} onChange={setSequenceClash} />}
                {plan.music?.exists && <ClashChoice group="download-music-clash" file={plan.music} what="music" value={musicClash} onChange={setMusicClash} />}
              </>
            )}
            <NotEditable onImport={importXsq} />
            <div className="flex justify-end gap-2">
              <Button onClick={onClose}>Cancel</Button>
              {phase.error && !plan && <Button onClick={() => setCheckTurn((n) => n + 1)}>Check again</Button>}
              <Button variant="primary" disabled={!clashesChosen} onClick={download} title={plan && !clashesChosen ? "Choose what to do about the file already there" : undefined}>
                <Download size={14} aria-hidden /> Download
              </Button>
            </div>
          </>
        )}

        {phase.kind === "downloading" && (
          <div className="flex flex-col gap-3">
            <p role="status">{progress ? (progress.step === "music" ? "Downloading the music…" : "Downloading the sequence…") : `Asking ${fppName} for the files…`}</p>
            <progress aria-label="Downloading from the FPP" className="w-full accent-violet-600" max={100} value={progress?.percent ?? 0} />
            <p className="text-xs text-neutral-500 tabular-nums">
              {progress?.percent ?? 0}%{progress && progress.total > 0 ? ` · ${sizeText(progress.done)} of ${sizeText(progress.total)}` : ""}
            </p>
            <div className="flex justify-end">
              <Button ref={cancelButton} onClick={cancel} title="Stop downloading; nothing is saved">
                Cancel
              </Button>
            </div>
          </div>
        )}

        {phase.kind === "done" && (
          <>
            <p role="status" className="flex items-start gap-2">
              <CheckCircle2 size={16} className="mt-0.5 shrink-0 text-green-600" aria-hidden />
              <span>
                Saved {fileName(phase.result.sequencePath)}
                {phase.result.musicPath ? ` and ${fileName(phase.result.musicPath)}` : ""}.
                {phase.added && " It's on the Play screen now."}
              </span>
            </p>
            {!phase.added && <p className="text-neutral-600 dark:text-neutral-300">Add it to Play to play it with its music on your controllers and in the preview.</p>}
            <NotEditable onImport={importXsq} />
            <div className="flex justify-end gap-2">
              <Button ref={doneButton} onClick={onClose}>
                Close
              </Button>
              {!phase.added && (
                <Button variant="primary" onClick={() => addToPlay(phase.result)}>
                  <ListPlus size={14} aria-hidden /> Add to Play
                </Button>
              )}
            </div>
          </>
        )}
      </div>
    </div>
  );
}

/** A downloaded .fseq plays, but holds only channel values: no effects to edit. */
function NotEditable({ onImport }: { onImport: () => void }) {
  return (
    <p className="text-xs text-neutral-600 dark:text-neutral-400">
      A downloaded sequence plays as it is, but its effects can't be edited: the .fseq holds only the lights' values. To edit it, bring in its xLights file (.xsq) with{" "}
      <button type="button" onClick={onImport} className="text-accent-700 underline hover:no-underline dark:text-accent-400">
        Import xLights sequence…
      </button>
    </p>
  );
}
