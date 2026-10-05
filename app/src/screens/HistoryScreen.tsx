import { RotateCcw } from "lucide-react";
import { useEffect, useState } from "react";
import type { HistoryEntry } from "../api/types";
import { Button, EmptyState, PageHeader } from "../components/ui";
import { useApp } from "../state/store";

/** Autosaved versions of the show; restoring one can be undone. */
export function HistoryScreen() {
  const backend = useApp((s) => s.backend);
  const run = useApp((s) => s.run);
  const revision = useApp((s) => s.snapshot?.revision);
  const [entries, setEntries] = useState<HistoryEntry[]>([]);

  useEffect(() => {
    if (!backend) return;
    void backend.listHistory().then(setEntries, () => setEntries([]));
  }, [backend, revision]);

  return (
    <div className="mx-auto max-w-4xl">
      <PageHeader title="History" description="PixelFlow autosaves a copy of your show every 30 seconds while you work. Restoring a copy can be undone." />
      {entries.length === 0 ? (
        <EmptyState title="No autosaved versions yet">They appear here after you make changes.</EmptyState>
      ) : (
        <ul className="flex flex-col">
          {entries.map((entry) => (
            <li key={entry.id} className="flex items-center gap-3 border-t border-neutral-200 py-2 dark:border-neutral-800">
              <span className="text-sm">{new Date(entry.savedAtMs).toLocaleString()}</span>
              <span className="text-xs text-neutral-500">{(entry.sizeBytes / 1024).toFixed(1)} KB</span>
              <Button className="ml-auto" onClick={() => run((b) => b.restoreHistory(entry.id))}>
                <RotateCcw size={14} /> Restore
              </Button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
