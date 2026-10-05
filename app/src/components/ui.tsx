import type { ComponentProps, InputHTMLAttributes, ReactNode } from "react";

type Variant = "primary" | "secondary" | "ghost" | "danger";

const VARIANTS: Record<Variant, string> = {
  primary: "bg-accent-600 text-white hover:bg-accent-500 disabled:bg-accent-600/40",
  secondary:
    "border border-neutral-300 bg-white hover:bg-neutral-100 dark:border-neutral-700 dark:bg-neutral-900 dark:hover:bg-neutral-800",
  ghost: "hover:bg-neutral-200/70 dark:hover:bg-neutral-800",
  danger: "text-red-600 hover:bg-red-50 dark:text-red-400 dark:hover:bg-red-950/60",
};

export function Button({
  variant = "secondary",
  className = "",
  ...props
}: ComponentProps<"button"> & { variant?: Variant }) {
  return (
    <button
      type="button"
      className={`inline-flex items-center justify-center gap-1.5 rounded-md px-3 py-1.5 text-sm font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-50 ${VARIANTS[variant]} ${className}`}
      {...props}
    />
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
  return <select className={`${CONTROL} ${className}`} {...props} />;
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
