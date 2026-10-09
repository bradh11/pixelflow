import { Download, Trash2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { AlignModels, ModelsProgress } from "../../api/assistant";
import { errorMessage } from "../../api/backend";
import { useAssistant } from "../../state/assistant";
import { ProgressBar } from "../ProgressBar";
import { Button } from "../ui";

/** "189 MB". */
export function megabytes(bytes: number): string {
  return `${Math.round(bytes / 1_000_000)} MB`;
}

/**
 * Settings → AI → Lyrics: on-device alignment. Turned on, Find lyrics times each word, syllable,
 * and mouth shape on this computer. Its model is downloaded only after the user agrees, in a
 * dialog that says what it is, where it comes from, and how big it is; it can be removed again.
 */
export function AlignmentSettings() {
  const api = useAssistant((s) => s.api);
  const on = useAssistant((s) => s.lyricsAlign);
  const [models, setModels] = useState<AlignModels | null>(null);
  const [asking, setAsking] = useState(false);
  const [progress, setProgress] = useState<ModelsProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!api) return;
    let current = true;
    api.alignModels().then(
      (found) => {
        if (!current) return;
        setModels(found);
        // Turned on, but the model is gone: off until it's downloaded again.
        if (!found.installed && useAssistant.getState().lyricsAlign) useAssistant.getState().setLyricsAlign(false);
      },
      (e) => current && setError(errorMessage(e)),
    );
    return () => {
      current = false;
    };
  }, [api]);

  if (!api) return null;

  const toggle = (checked: boolean) => {
    setError(null);
    if (!checked) useAssistant.getState().setLyricsAlign(false);
    else if (models?.installed) useAssistant.getState().setLyricsAlign(true);
    else setAsking(true);
  };

  const download = async () => {
    setAsking(false);
    setError(null);
    setProgress({ received: 0, total: models?.bytes ?? 0 });
    try {
      const done = await api.downloadAlignModels((p) => setProgress(p));
      setModels(done);
      useAssistant.getState().setLyricsAlign(true);
    } catch (e) {
      const message = errorMessage(e);
      if (message !== "Stopped.") setError(message);
    } finally {
      setProgress(null);
    }
  };

  const remove = async () => {
    setBusy(true);
    setError(null);
    try {
      setModels(await api.removeAlignModels());
      useAssistant.getState().setLyricsAlign(false);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const size = models ? megabytes(models.bytes) : "";
  return (
    <div className="flex flex-col gap-1 text-sm">
      <label className="flex items-start gap-2">
        <input
          type="checkbox"
          className="mt-0.5"
          checked={on}
          disabled={!models?.available || progress !== null}
          onChange={(e) => toggle(e.target.checked)}
          aria-describedby="ai-align-about"
        />
        <span>On-device alignment</span>
      </label>
      <p id="ai-align-about" className="pl-6 text-xs text-neutral-500">
        Times each word, syllable, and mouth shape on this computer from the song itself, so published lyrics need nothing sent to OpenAI. For songs
        sung in English.
      </p>
      <div className="pl-6">
        {progress ? (
          <div className="flex items-end gap-2">
            <ProgressBar
              className="min-w-0 flex-1"
              label={`Downloading the model: ${megabytes(progress.received)} of ${megabytes(progress.total)}`}
              fraction={progress.total > 0 ? progress.received / progress.total : null}
            />
            <Button variant="ghost" onClick={() => void api.cancelAlignDownload()}>
              Cancel
            </Button>
          </div>
        ) : models?.installed ? (
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-xs text-green-700 dark:text-green-400">Model downloaded ({size}).</span>
            <Button variant="ghost" disabled={busy} onClick={() => void remove()}>
              <Trash2 size={14} aria-hidden /> Remove models
            </Button>
          </div>
        ) : models ? (
          <span className="text-xs text-neutral-500">
            {models.available ? `Needs a one-time download (${size}).` : "This computer has no folder for PixelFlow to keep the model in."}
          </span>
        ) : null}
        {error && (
          <p role="alert" className="mt-1 text-xs text-red-700 dark:text-red-300">
            {error}
          </p>
        )}
      </div>
      {asking && models && <DownloadConsent models={models} onDownload={() => void download()} onCancel={() => setAsking(false)} />}
    </div>
  );
}

/** Asks before the alignment model is downloaded: what it is, its licence, where it comes from,
 * and its size. */
function DownloadConsent({ models, onDownload, onCancel }: { models: AlignModels; onDownload: () => void; onCancel: () => void }) {
  const cancel = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    cancel.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      onCancel();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onCancel]);
  const host = (url: string) => {
    try {
      return new URL(url).host;
    } catch {
      return url;
    }
  };
  return (
    <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/40" onClick={(e) => e.stopPropagation()}>
      <div
        role="alertdialog"
        aria-modal="true"
        aria-label="Download the alignment model?"
        aria-describedby="align-consent-what"
        className="w-[30rem] max-w-[calc(100vw-2rem)] rounded-lg border border-neutral-200 bg-white p-5 shadow-xl dark:border-neutral-800 dark:bg-neutral-900"
        onKeyDown={(e) => e.stopPropagation()}
      >
        <h2 className="text-lg font-semibold">Download the alignment model?</h2>
        <p id="align-consent-what" className="mt-2 text-sm text-neutral-600 dark:text-neutral-300">
          On-device alignment needs a speech model, downloaded once ({megabytes(models.bytes)}) from {host(models.urls[0] ?? models.source)} and kept in
          PixelFlow&apos;s data folder. Each file is checked before it&apos;s kept. Your songs never leave this computer for it.
        </p>
        <dl className="mt-3 grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-xs">
          <dt className="text-neutral-500">Model</dt>
          <dd>{models.name}</dd>
          <dt className="text-neutral-500">Licence</dt>
          <dd>{models.licence}</dd>
          <dt className="text-neutral-500">Size</dt>
          <dd>{megabytes(models.bytes)}</dd>
          <dt className="text-neutral-500">From</dt>
          <dd className="break-all">{models.source}</dd>
        </dl>
        <div className="mt-5 flex justify-end gap-2">
          <Button ref={cancel} onClick={onCancel}>
            Cancel
          </Button>
          <Button variant="primary" onClick={onDownload}>
            <Download size={14} aria-hidden /> Download
          </Button>
        </div>
      </div>
    </div>
  );
}
