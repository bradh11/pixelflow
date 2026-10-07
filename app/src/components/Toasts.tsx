import { CheckCircle2, Info, X } from "lucide-react";
import { useToasts } from "../state/toast";

/**
 * Brief confirmations at the bottom right (clear of the timeline's scrollbar and the canvas's
 * middle); they go by themselves, but wait while pointed at or focused.
 */
export function Toasts() {
  const toasts = useToasts((s) => s.toasts);
  const { dismiss, pause, resume } = useToasts.getState();
  return (
    <div aria-live="polite" data-testid="toasts" className="pointer-events-none fixed right-4 bottom-10 z-50 flex flex-col items-end gap-2">
      {toasts.map((t) => (
        <div
          key={t.id}
          data-testid="toast"
          data-tone={t.tone}
          onPointerEnter={() => pause(t.id)}
          onPointerLeave={() => resume(t.id)}
          onFocus={() => pause(t.id)}
          onBlur={() => resume(t.id)}
          className="pointer-events-auto flex max-w-sm items-center gap-2 rounded-full border border-neutral-200 bg-white py-1.5 pr-1.5 pl-3 text-sm shadow-lg dark:border-neutral-700 dark:bg-neutral-800"
        >
          {t.tone === "success" ? (
            <CheckCircle2 size={15} className="shrink-0 text-emerald-600 dark:text-emerald-400" aria-hidden />
          ) : (
            <Info size={15} className="shrink-0 text-sky-600 dark:text-sky-400" aria-hidden />
          )}
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
