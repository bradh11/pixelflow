import type { ReactNode } from "react";

/** A titled part of a device page: a compact heading row (with optional `actions`) over its content. */
export function Section({ title, actions, children, className = "" }: { title: string; actions?: ReactNode; children: ReactNode; className?: string }) {
  return (
    <section aria-label={title} className={`flex min-w-0 flex-col gap-2 rounded-lg border border-neutral-200 bg-white p-3 text-sm dark:border-neutral-800 dark:bg-neutral-900 ${className}`}>
      <div className="flex min-h-7 items-center justify-between gap-2">
        <h2 className="text-xs font-semibold tracking-wide text-neutral-500 uppercase dark:text-neutral-400">{title}</h2>
        {actions && <div className="flex items-center gap-2">{actions}</div>}
      </div>
      {children}
    </section>
  );
}

/** A small round status light. */
export function Dot({ tone }: { tone: "ok" | "bad" | "idle" }) {
  const color = tone === "ok" ? "bg-emerald-500" : tone === "bad" ? "bg-red-500" : "bg-neutral-400";
  return <span aria-hidden className={`inline-block size-2 shrink-0 rounded-full ${color}`} />;
}
