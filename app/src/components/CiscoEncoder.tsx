import { Download, Trash2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { errorMessage } from "../api/backend";
import type { H264Encoder, H264Progress } from "../api/video";
import { useSequencer } from "../state/sequencer";
import { ProgressBar } from "./ProgressBar";
import { Button } from "./ui";

/** "about 0.5 MB". */
export function downloadSize(bytes: number): string {
  return `about ${Math.max(0.1, Math.round(bytes / 100_000) / 10)} MB`;
}

/** "0.2 of 0.5 MB". */
function downloaded(progress: H264Progress): string {
  const mb = (bytes: number) => (Math.round(bytes / 100_000) / 10).toFixed(1);
  return `${mb(progress.received)} of ${mb(progress.total)} MB`;
}

/**
 * What the download of Cisco's encoder is, from where, how big, and why it comes from Cisco: said
 * before anything is downloaded, in the export dialog and in Settings.
 */
export function EncoderConsent({ encoder, id }: { encoder: H264Encoder; id?: string }) {
  return (
    <div id={id} className="flex flex-col gap-2 text-sm text-neutral-600 dark:text-neutral-300">
      <p>
        Video export uses Cisco&apos;s OpenH264 video encoder. PixelFlow downloads it once from Cisco&apos;s server ({encoder.host},{" "}
        {downloadSize(encoder.bytes)}), checks it, and keeps it in PixelFlow&apos;s data folder.
      </p>
      <p>
        It comes from Cisco rather than with PixelFlow so that the H.264 patent licence Cisco pays for its own copies covers it. Nothing about you or
        your show is sent. You can turn it off or remove it in Settings.
      </p>
      <p className="text-xs text-neutral-500">{encoder.attribution}</p>
    </div>
  );
}

/**
 * Settings → Video export: Cisco's OpenH264 encoder, which the built-in export uses. Turned on, it
 * is downloaded (only after the user agrees, in a dialog that says what, from where, how big, and
 * why); it can be turned off again and removed. Cisco's licence asks that its line be shown here,
 * where it's controlled, and its text with the licensing information.
 */
export function VideoEncoderSettings() {
  const api = useSequencer((s) => s.api);
  const [encoder, setEncoder] = useState<H264Encoder | null>(null);
  const [asking, setAsking] = useState(false);
  const [progress, setProgress] = useState<H264Progress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!api) return;
    let current = true;
    api.h264Encoder().then(
      (found) => current && setEncoder(found),
      (e) => current && setError(errorMessage(e)),
    );
    return () => {
      current = false;
    };
  }, [api]);

  if (!api) return null;

  const change = async (work: () => Promise<H264Encoder>) => {
    setBusy(true);
    setError(null);
    try {
      setEncoder(await work());
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const toggle = (on: boolean) => {
    setError(null);
    if (!encoder) return;
    if (on && encoder.state === "missing") setAsking(true);
    else void change(() => api.setH264EncoderEnabled(on));
  };

  const download = async () => {
    setAsking(false);
    setError(null);
    setProgress({ received: 0, total: encoder?.bytes ?? 0 });
    try {
      setEncoder(await api.downloadH264Encoder((p) => setProgress(p)));
    } catch (e) {
      const message = errorMessage(e);
      if (message !== "Stopped.") setError(message);
    } finally {
      setProgress(null);
    }
  };

  const downloadedHere = encoder?.state === "ready" || encoder?.state === "off";
  const status = !encoder
    ? "Checking…"
    : encoder.state === "ready"
      ? `Downloaded (version ${encoder.version}). Video export uses it.`
      : encoder.state === "off"
        ? `Downloaded (version ${encoder.version}), and turned off: video export asks before using it.`
        : encoder.state === "missing"
          ? `Not downloaded. PixelFlow asks before downloading it (${downloadSize(encoder.bytes)} from ${encoder.host}), the first time you export a video.`
          : (encoder.problem ?? "");
  return (
    <div className="flex flex-col gap-1 text-sm">
      <label className="flex items-start gap-2">
        <input
          type="checkbox"
          className="mt-0.5 accent-accent-500"
          checked={encoder?.state === "ready"}
          disabled={!encoder || encoder.state === "unsupported" || progress !== null || busy}
          onChange={(e) => toggle(e.target.checked)}
          aria-describedby="video-encoder-status"
        />
        <span>Use Cisco&apos;s OpenH264 video encoder</span>
      </label>
      <div className="flex flex-col gap-1.5 pl-6">
        <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
          <p id="video-encoder-status" className="text-xs text-neutral-500">
            {status}
          </p>
          {downloadedHere && !progress && (
            <Button variant="ghost" disabled={busy} onClick={() => void change(() => api.removeH264Encoder())}>
              <Trash2 size={14} aria-hidden /> Remove the encoder
            </Button>
          )}
        </div>
        {progress && (
          <div className="flex items-end gap-2">
            <ProgressBar
              className="min-w-0 flex-1"
              label={`Downloading the encoder: ${downloaded(progress)}`}
              fraction={progress.total > 0 ? progress.received / progress.total : null}
            />
            <Button variant="ghost" onClick={() => void api.cancelH264Download()}>
              Cancel
            </Button>
          </div>
        )}
        {error && (
          <div role="alert" className="flex flex-wrap items-center gap-2 text-xs text-red-700 dark:text-red-300">
            <span>{error}</span>
            {encoder?.state === "missing" && (
              <Button variant="ghost" onClick={() => void download()}>
                Try again
              </Button>
            )}
          </div>
        )}
        {encoder && (
          <>
            <p className="text-xs text-neutral-600 dark:text-neutral-400">{encoder.attribution}</p>
            <details className="text-xs text-neutral-500">
              <summary className="cursor-pointer select-none">Cisco&apos;s licence for the encoder</summary>
              <pre className="mt-1 max-h-60 overflow-auto rounded border border-neutral-200 p-2 font-sans whitespace-pre-wrap dark:border-neutral-800">
                {encoder.licence}
              </pre>
            </details>
          </>
        )}
      </div>
      {asking && encoder && <DownloadConsent encoder={encoder} onDownload={() => void download()} onCancel={() => setAsking(false)} />}
    </div>
  );
}

/** Asks before Cisco's encoder is downloaded from Settings. */
function DownloadConsent({ encoder, onDownload, onCancel }: { encoder: H264Encoder; onDownload: () => void; onCancel: () => void }) {
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
  return (
    <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/40" onClick={(e) => e.stopPropagation()}>
      <div
        role="alertdialog"
        aria-modal="true"
        aria-label="Download Cisco's video encoder?"
        aria-describedby="h264-consent-settings"
        className="w-[30rem] max-w-[calc(100vw-2rem)] rounded-lg border border-neutral-200 bg-white p-5 shadow-xl dark:border-neutral-800 dark:bg-neutral-900"
        onKeyDown={(e) => e.stopPropagation()}
      >
        <h2 className="mb-2 text-lg font-semibold">Download Cisco&apos;s video encoder?</h2>
        <EncoderConsent encoder={encoder} id="h264-consent-settings" />
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
