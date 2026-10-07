import { CheckCircle2, FileQuestion, Search, X } from "lucide-react";
import { useEffect, useRef } from "react";
import type { FileRole, MissingFile } from "../api/types";
import { fileName, plural, shownPath } from "../lib/format";
import { folderOf, sameFile } from "../lib/showFiles";
import { missingNoticeKey, useApp } from "../state/store";
import { Button } from "./ui";

const NO_MISSING: MissingFile[] = [];

/** The show's files that aren't where it says they are. */
export function useMissingFiles(): MissingFile[] {
  return useApp((s) => s.snapshot?.missingFiles ?? NO_MISSING);
}

/** The missing file for `file`, if it is missing. */
export function useMissingFile(file: FileRole | null): MissingFile | undefined {
  const missing = useMissingFiles();
  return file ? missing.find((m) => sameFile(m.file, file)) : undefined;
}

/** Where a file was, for people to read. */
function wasIn(path: string): string {
  const folder = folderOf(path);
  return folder ? `It was in ${shownPath(folder)}.` : "";
}

/**
 * One missing file, said plainly ("Christmas Medley 2017.mp3 isn't where it was."), with Find
 * again (a search of the show's folder) and Locate… (the user shows where it is).
 */
export function MissingFileNotice({
  missing,
  onFind,
  onLocate,
  showOwner = false,
}: {
  missing: MissingFile;
  /** Defaults to the show's search and dialog. */
  onFind?: () => void;
  onLocate?: () => void;
  /** Say what the file belongs to ("Music for Medley"). */
  showOwner?: boolean;
}) {
  const findMissingFiles = useApp((s) => s.findMissingFiles);
  const locateFile = useApp((s) => s.locateFile);
  const busy = useApp((s) => s.busy);
  const find = onFind ?? (() => void findMissingFiles(missing.file));
  const locate = onLocate ?? (() => void locateFile(missing.file));
  return (
    <div
      role="group"
      aria-label={missing.message}
      className="rounded-md border border-amber-300 bg-amber-50 p-2.5 text-sm text-amber-900 dark:border-amber-800/70 dark:bg-amber-950/40 dark:text-amber-200"
    >
      <p className="flex items-start gap-2">
        <FileQuestion size={15} className="mt-0.5 shrink-0" aria-hidden />
        <span className="min-w-0">
          <span className="font-medium break-words">{missing.name}</span> isn't where it was.
          {showOwner && <span className="block text-xs text-amber-800/80 dark:text-amber-300/80">{missing.owner}</span>}
          <span className="block truncate text-xs text-amber-800/80 dark:text-amber-300/80" title={shownPath(missing.wasAt)}>
            {wasIn(missing.wasAt)}
          </span>
        </span>
      </p>
      <div className="mt-2 flex flex-wrap gap-2 pl-6">
        <Button aria-label={`Find ${missing.name} again`} disabled={busy} onClick={find}>
          <Search size={14} aria-hidden /> Find again
        </Button>
        <Button aria-label={`Locate ${missing.name}`} disabled={busy} onClick={locate}>
          Locate…
        </Button>
      </div>
    </div>
  );
}

/**
 * Shown under the top bar while the open show has files that aren't where they were (a moved
 * or synced show folder, usually): offers to find them all at once.
 */
/**
 * Whether the "files aren't where they were" banner is up and names `name`: a screen then holds
 * its own note about that file back, so the same alarm doesn't show twice.
 */
export function useMissingBannerNames(name: string | null): boolean {
  const missing = useMissingFiles();
  const dismissed = useApp((s) => s.missingNoticeDismissed === missingNoticeKey(s.snapshot));
  return name !== null && !dismissed && missing.some((m) => m.name === name);
}

export function MissingFilesBanner() {
  const missing = useMissingFiles();
  const snapshot = useApp((s) => s.snapshot);
  const dismissed = useApp((s) => s.missingNoticeDismissed === missingNoticeKey(s.snapshot));
  const findMissingFiles = useApp((s) => s.findMissingFiles);
  const dismiss = useApp((s) => s.dismissMissingNotice);
  const busy = useApp((s) => s.busy);
  if (missing.length === 0 || dismissed || !snapshot) return null;
  const names = missing.slice(0, 3).map((m) => m.name);
  const more = missing.length - names.length;
  return (
    <section
      aria-label="Missing files"
      className="flex flex-wrap items-center gap-3 border-b border-amber-200 bg-amber-50 px-4 py-2 text-sm text-amber-900 dark:border-amber-900/70 dark:bg-amber-950/30 dark:text-amber-200"
    >
      <FileQuestion size={16} className="shrink-0" aria-hidden />
      <p className="min-w-0 flex-1">
        {missing.length === 1 ? (
          <>
            <span className="font-medium">{names[0]}</span> isn't where it was.
          </>
        ) : (
          <>
            {plural(missing.length, "file")} aren't where they were: <span className="font-medium">{names.join(", ")}</span>
            {more > 0 && ` and ${more} more`}.
          </>
        )}{" "}
        {missing.length === 1
          ? snapshot.path
            ? "PixelFlow can look for it in the show's folder."
            : "Save the show, or locate it from the problems list."
          : snapshot.path
            ? "PixelFlow can look for them in the show's folder."
            : "Save the show, or locate them one by one."}
      </p>
      {snapshot.path && (
        <Button variant="primary" disabled={busy} onClick={() => void findMissingFiles()}>
          <Search size={14} aria-hidden /> Find all missing files
        </Button>
      )}
      <Button variant="ghost" onClick={dismiss}>
        Not now
      </Button>
    </section>
  );
}

/** What the last search for missing files found (all one undo step), until dismissed. */
export function FilesReport() {
  const report = useApp((s) => s.filesReport);
  const dismiss = useApp((s) => s.dismissFilesReport);
  const locateFile = useApp((s) => s.locateFile);
  const doneRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!report) return;
    doneRef.current?.focus();
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && dismiss();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [report, dismiss]);

  if (!report) return null;
  const { found, stillMissing } = report;
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4">
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="files-report-title"
        className="flex max-h-[85vh] w-full max-w-xl flex-col rounded-xl border border-neutral-200 bg-white shadow-2xl dark:border-neutral-800 dark:bg-neutral-900"
      >
        <div className="flex items-start justify-between gap-3 border-b border-neutral-200 p-5 dark:border-neutral-800">
          <h2 id="files-report-title" className="flex items-center gap-2 text-lg font-semibold">
            {found.length > 0 ? (
              <>
                <CheckCircle2 size={18} className="text-green-600" aria-hidden /> Found {plural(found.length, "file")}
              </>
            ) : (
              <>
                <FileQuestion size={18} className="text-amber-600" aria-hidden /> No files found
              </>
            )}
          </h2>
          <button type="button" aria-label="Close" data-tip="Close" onClick={dismiss} className="rounded p-1 hover:bg-neutral-100 dark:hover:bg-neutral-800">
            <X size={16} />
          </button>
        </div>
        <div className="flex-1 overflow-auto p-5 text-sm">
          {found.length > 0 && (
            <>
              <ul className="flex flex-col gap-2">
                {found.map((f) => (
                  <li key={`${f.file.kind}:${"id" in f.file ? f.file.id : ""}`} className="flex items-start gap-2">
                    <CheckCircle2 size={14} className="mt-0.5 shrink-0 text-green-600 dark:text-green-400" aria-hidden />
                    <span className="min-w-0">
                      <span className="font-medium">{f.name}</span> is now in{" "}
                      <span className="break-all text-neutral-600 dark:text-neutral-300">{shownPath(folderOf(f.to))}</span>
                      {fileName(f.to) !== f.name && <> as {fileName(f.to)}</>}
                      {f.also.length > 0 && (
                        <span className="block text-xs text-amber-700 dark:text-amber-400">
                          Also found in {f.also.map((p) => shownPath(folderOf(p))).join(", ")}. If that's the right one, use Locate… to
                          choose it.
                        </span>
                      )}
                    </span>
                  </li>
                ))}
              </ul>
              <p className="mt-3 text-neutral-500">The show now uses these. Undo puts the old places back; save the show to keep them.</p>
            </>
          )}
          {report.gaveUp && (
            <p className="mt-3 text-amber-700 dark:text-amber-400">
              PixelFlow stopped looking before it had checked every folder. Use Locate… for anything still missing.
            </p>
          )}
          {stillMissing.length > 0 && (
            <div className={found.length > 0 ? "mt-5" : ""}>
              <p className="mb-2 text-neutral-500">
                {found.length > 0 ? "Still missing" : "PixelFlow looked in the show's folder and the folders inside it, but couldn't find:"}
              </p>
              <ul className="flex flex-col gap-2">
                {stillMissing.map((m) => (
                  <li key={`${m.file.kind}:${"id" in m.file ? m.file.id : ""}`} className="flex items-center gap-3">
                    <span className="min-w-0 flex-1">
                      <span className="font-medium">{m.name}</span>
                      <span className="block text-xs text-neutral-500">{m.owner}</span>
                    </span>
                    <Button aria-label={`Locate ${m.name}`} onClick={() => void locateFile(m.file)}>
                      Locate…
                    </Button>
                  </li>
                ))}
              </ul>
            </div>
          )}
        </div>
        <div className="flex justify-end border-t border-neutral-200 p-4 dark:border-neutral-800">
          <Button ref={doneRef} variant="primary" onClick={dismiss}>
            Done
          </Button>
        </div>
      </div>
    </div>
  );
}
