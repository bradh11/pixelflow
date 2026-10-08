import { ChevronDown, ChevronRight, House, Music } from "lucide-react";
import { type ComponentProps, type InputHTMLAttributes, type ReactNode, useId, useState } from "react";

type Variant = "primary" | "secondary" | "ghost" | "danger";

const VARIANTS: Record<Variant, string> = {
  primary: "bg-accent-600 text-white hover:bg-accent-500 disabled:bg-accent-600/40",
  secondary:
    "border border-neutral-300 bg-white hover:bg-neutral-100 dark:border-neutral-700 dark:bg-neutral-900 dark:hover:bg-neutral-800",
  ghost: "hover:bg-neutral-200/70 dark:hover:bg-neutral-800",
  danger: "text-red-600 hover:bg-red-50 dark:text-red-400 dark:hover:bg-red-950/60",
};

type ButtonProps = ComponentProps<"button"> & { variant?: Variant; "data-tip"?: string; "data-tip-key"?: string };

export function Button({ variant = "secondary", className = "", ...props }: ButtonProps) {
  // A button named only by its aria-label (an icon) shows that name as its tooltip.
  const tip = props["data-tip"] ?? (props.title === undefined ? props["aria-label"] : undefined);
  return (
    <button
      type="button"
      className={`inline-flex items-center justify-center gap-1.5 rounded-md px-3 py-1.5 text-sm font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-50 ${VARIANTS[variant]} ${className}`}
      {...props}
      data-tip={tip}
    />
  );
}

/**
 * A button showing only an icon: `label` is its name, and shows as its tooltip (or `hint`, when
 * that says more), with the `shortcut` that does the same.
 */
export function IconButton({
  label,
  hint,
  shortcut,
  className = "rounded-md p-2 text-neutral-600 hover:bg-neutral-200/70 disabled:opacity-30 disabled:hover:bg-transparent dark:text-neutral-300 dark:hover:bg-neutral-800",
  ...props
}: Omit<ComponentProps<"button">, "title"> & { label: string; hint?: string; shortcut?: string }) {
  return <button type="button" aria-label={label} data-tip={hint ?? label} data-tip-key={shortcut} className={className} {...props} />;
}

const MORE_KEY = "pixelflow.more";

function loadMore(): Record<string, boolean> {
  try {
    const saved = JSON.parse(localStorage.getItem(MORE_KEY) ?? "{}") as unknown;
    return saved && typeof saved === "object" ? (saved as Record<string, boolean>) : {};
  } catch {
    return {};
  }
}

/** Whether the "More" disclosure called `id` was left open on this computer. */
export function moreOpen(id: string): boolean {
  return loadMore()[id] === true;
}

function saveMore(id: string, open: boolean) {
  try {
    localStorage.setItem(MORE_KEY, JSON.stringify({ ...loadMore(), [id]: open }));
  } catch {
    // Storage unavailable: it stays as it is until the form closes.
  }
}

/**
 * Fields most people leave alone, folded under a "More" button. Whether it's open is remembered
 * on this computer by `id`, so a form opens the way it was last left.
 */
/** `forceOpen`: start open whatever was remembered, because something inside is set and should be seen. */
export function More({ id, label = "More", forceOpen = false, children }: { id: string; label?: string; forceOpen?: boolean; children: ReactNode }) {
  const [open, setOpen] = useState(() => forceOpen || moreOpen(id));
  const region = useId();
  return (
    <div className="mt-3">
      <button
        type="button"
        aria-expanded={open}
        aria-controls={region}
        onClick={() => {
          saveMore(id, !open);
          setOpen(!open);
        }}
        className="flex items-center gap-1 rounded text-xs font-medium text-neutral-600 hover:text-neutral-900 dark:text-neutral-400 dark:hover:text-neutral-100"
      >
        {open ? <ChevronDown size={14} aria-hidden /> : <ChevronRight size={14} aria-hidden />}
        {label}
      </button>
      <div id={region} role="group" aria-label={label} hidden={!open} className="mt-2">
        {open && children}
      </div>
    </div>
  );
}

export function Card({ children, className = "" }: { children: ReactNode; className?: string }) {
  return (
    <div
      className={`rounded-lg border border-neutral-200 bg-white p-4 dark:border-neutral-800 dark:bg-neutral-900 ${className}`}
    >
      {children}
    </div>
  );
}

export function EmptyState({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <div className="flex flex-col items-center justify-center gap-2 rounded-lg border border-dashed border-neutral-300 p-10 text-center dark:border-neutral-700">
      <p className="font-medium">{title}</p>
      {children && <div className="max-w-md text-sm text-neutral-500 dark:text-neutral-400">{children}</div>}
    </div>
  );
}

export function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="flex flex-col gap-1 text-sm">
      <span className="text-neutral-600 dark:text-neutral-400">{label}</span>
      {children}
    </label>
  );
}

const CONTROL =
  "rounded-md border border-neutral-300 bg-white px-2 py-1.5 text-sm dark:border-neutral-700 dark:bg-neutral-950";

export function Input({ className = "", ...props }: InputHTMLAttributes<HTMLInputElement>) {
  return <input className={`${CONTROL} ${className}`} {...props} />;
}

export function Select({ className = "", ...props }: ComponentProps<"select">) {
  return <select className={`pf-select ${CONTROL} ${className}`} {...props} />;
}

const UNSAVED = {
  show: { label: "Show not saved", Icon: House, style: "border-amber-300 bg-amber-50 text-amber-800 dark:border-amber-800 dark:bg-amber-950/50 dark:text-amber-300" },
  sequence: { label: "Sequence not saved", Icon: Music, style: "border-violet-300 bg-violet-50 text-violet-800 dark:border-violet-800 dark:bg-violet-950/50 dark:text-violet-300" },
};

/**
 * "Show not saved" or "Sequence not saved": the two documents are saved separately, so each says
 * so in its own color and icon (amber with a house for the show, violet with a note for a sequence).
 */
export function UnsavedBadge({ doc }: { doc: "show" | "sequence" }) {
  const { label, Icon, style } = UNSAVED[doc];
  return (
    <span role="note" className={`inline-flex shrink-0 items-center gap-1 rounded-full border px-2 py-0.5 text-xs font-medium whitespace-nowrap ${style}`}>
      <Icon size={12} aria-hidden />
      {label}
    </span>
  );
}

/**
 * A work screen's one header row: its name, its `tools`, and its primary actions (`children`) at
 * the end. (Overview screens use PageHeader, with a description.)
 */
export function ScreenHeader({ title, tools, children }: { title: string; tools?: ReactNode; children?: ReactNode }) {
  return (
    <div className="@container mb-3 shrink-0">
      <div className="flex min-h-9 flex-wrap items-center gap-x-3 gap-y-2">
        <h1 className="shrink-0 text-base font-semibold">{title}</h1>
        {/* The tools share the row when there's room, else take a row of their own. */}
        {tools && <div className="flex min-w-0 flex-1 @max-[1099px]:order-last @max-[1099px]:basis-full">{tools}</div>}
        <div className="ml-auto flex shrink-0 items-center gap-2">{children}</div>
      </div>
    </div>
  );
}

export function PageHeader({ title, description, actions }: { title: string; description?: string; actions?: ReactNode }) {
  return (
    <div className="mb-6 flex items-start justify-between gap-4">
      <div>
        <h1 className="text-xl font-semibold">{title}</h1>
        {description && <p className="mt-1 text-sm text-neutral-500 dark:text-neutral-400">{description}</p>}
      </div>
      {actions && <div className="flex shrink-0 gap-2">{actions}</div>}
    </div>
  );
}
