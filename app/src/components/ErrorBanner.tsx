import { X } from "lucide-react";
import { useApp } from "../state/store";

/** Shows the last error in plain language until dismissed. */
export function ErrorBanner() {
  const error = useApp((s) => s.error);
  const dismiss = useApp((s) => s.dismissError);
  if (!error) return null;
  return (
    <div
      role="alert"
      className="fixed top-14 right-4 z-50 flex max-w-md items-start gap-3 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-800 shadow-lg dark:border-red-900 dark:bg-red-950 dark:text-red-200"
    >
      <p className="flex-1">{error}</p>
      <button type="button" aria-label="Dismiss" data-tip="Dismiss" onClick={dismiss} className="rounded p-0.5 hover:bg-red-100 dark:hover:bg-red-900">
        <X size={16} />
      </button>
    </div>
  );
}
