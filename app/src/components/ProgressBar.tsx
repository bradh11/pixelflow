/**
 * How far long work has got: a thin bar, with its label and percent above it. Without a
 * `fraction` (the work can't tell), the bar sweeps instead. `slim` draws only a hairline bar
 * (the label is still its accessible name), to sit under something that already says what's
 * going on.
 */
export function ProgressBar({ label, fraction, slim = false, className = "" }: { label: string; fraction: number | null; slim?: boolean; className?: string }) {
  const percent = fraction === null ? null : Math.round(Math.min(1, Math.max(0, fraction)) * 100);
  return (
    <div className={`flex min-w-0 flex-col gap-1 ${className}`}>
      {!slim && (
        <div className="flex items-baseline justify-between gap-2 text-xs text-neutral-500" aria-hidden>
          <span className="truncate">{label}…</span>
          {percent !== null && <span className="shrink-0 tabular-nums">{percent}%</span>}
        </div>
      )}
      <div
        role="progressbar"
        aria-label={label}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={percent ?? undefined}
        aria-valuetext={percent === null ? `${label}…` : `${percent}%`}
        className={`relative overflow-hidden rounded-full bg-neutral-200 dark:bg-neutral-800 ${slim ? "h-0.5" : "h-1"}`}
      >
        {percent === null ? (
          <div className="pf-progress-sweep absolute inset-y-0 w-1/3 rounded-full bg-accent-500" />
        ) : (
          <div className="h-full rounded-full bg-accent-500 transition-[width] duration-150 ease-out" style={{ width: `${percent}%` }} />
        )}
      </div>
    </div>
  );
}
