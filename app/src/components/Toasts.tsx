import { CheckCircle2, X } from "lucide-react";
import { useToasts } from "../state/toast";

/** Brief confirmations at the bottom of the window; they go by themselves. */
export function Toasts() {
  const toasts = useToasts((s) => s.toasts);
  const dismiss = useToasts((s) => s.dismiss);
  return (
    <div aria-live="polite" data-testid="toasts" className="pointer-events-none fixed bottom-10 left-1/2 z-50 flex -translate-x-1/2 flex-col items-center gap-2">
      {toasts.map((t) => (
        <div
          key={t.id}
          data-testid="toast"
          className="pointer-events-auto flex items-center gap-2 rounded-full border border-neutral-200 bg-white py-1.5 pr-1.5 pl-3 text-sm shadow-lg dark:border-neutral-700 dark:bg-neutral-800"
        >
          <CheckCircle2 size={15} className="shrink-0 text-emerald-600 dark:text-emerald-400" aria-hidden />
          <span>{t.text}</span>
          {t.action && (
            <button
              type="button"
              className="rounded-full px-2 py-0.5 font-medium text-accent-600 hover:bg-neutral-100 dark:text-accent-400 dark:hover:bg-neutral-700"
              onClick={() => {
                dismiss(t.id);
                void t.action!.run();
              }}
            >
              {t.action.label}
            </button>
          )}
          <button type="button" aria-label="Dismiss" title="Dismiss" className="rounded-full p-1 text-neutral-400 hover:bg-neutral-100 dark:hover:bg-neutral-700" onClick={() => dismiss(t.id)}>
            <X size={12} />
          </button>
        </div>
      ))}
    </div>
  );
}
