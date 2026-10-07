import { RotateCcw } from "lucide-react";
import { useEffect, useState } from "react";
import type { HistoryEntry } from "../api/types";
import { Button, EmptyState, PageHeader } from "../components/ui";
import { useApp } from "../state/store";
import { toast } from "../state/toast";

/** How often the list is read again: backups are made in the background, every 30 seconds. */
export const HISTORY_REFRESH_MS = 5_000;

/** Backups of the show (not its saved file); restoring one can be undone. */
export function HistoryScreen() {
  const backend = useApp((s) => s.backend);
  const run = useApp((s) => s.run);
  const revision = useApp((s) => s.snapshot?.revision);
  // A save can move the backups (to the saved file's own list) without changing the revision.
  const path = useApp((s) => s.snapshot?.path);
  const dirty = useApp((s) => s.snapshot?.dirty);
  const [entries, setEntries] = useState<HistoryEntry[]>([]);

  useEffect(() => {
    if (!backend) return;
    let cancelled = false;
    const read = () =>
      void backend.listHistory().then(
        (list) => !cancelled && setEntries(list),
        () => !cancelled && setEntries([]),
      );
    read();
    const timer = setInterval(read, HISTORY_REFRESH_MS);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [backend, revision, path, dirty]);

  return (
    <div className="mx-auto max-w-4xl">
      <PageHeader
        title="History"
        description="Backups of your show, kept every 30 seconds while you work. They aren't your saved file: save (⌘S) to keep your changes. Restoring a backup can be undone. Unsaved sequences are offered back on the Sequence screen."
      />
      {entries.length === 0 ? (
        <EmptyState title="No backups yet">
          <p>They appear here after you make changes.</p>
          <div className="mt-3">
            <Button onClick={() => useApp.getState().setScreen("layout")}>Go to Layout</Button>
          </div>
        </EmptyState>
      ) : (
        <ul className="flex flex-col">
          {entries.map((entry) => (
            <li key={entry.id} className="flex items-center gap-3 border-t border-neutral-200 py-2 dark:border-neutral-800">
              <span className="text-sm">{new Date(entry.savedAtMs).toLocaleString()}</span>
              <span className="text-xs text-neutral-500">{(entry.sizeBytes / 1024).toFixed(1)} KB</span>
              <Button
                className="ml-auto"
                title="Put the show back as it was then (Undo takes it back)"
                onClick={async () => {
                  if (await run((b) => b.restoreHistory(entry.id))) toast(`Restored the show from ${new Date(entry.savedAtMs).toLocaleString()}`);
                }}
              >
                <RotateCcw size={14} /> Restore
              </Button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
