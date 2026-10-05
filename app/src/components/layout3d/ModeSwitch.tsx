import type { LayoutMode } from "../../state/view3d";

/** A 2D | 3D switch. */
export function ModeSwitch({ mode, onChange, hint }: { mode: LayoutMode; onChange: (mode: LayoutMode) => void; hint?: string }) {
  return (
    <div role="group" aria-label="View" className="inline-flex rounded-md bg-neutral-100 p-0.5 dark:bg-neutral-800">
      {(["2d", "3d"] as const).map((m) => (
        <button
          key={m}
          type="button"
          aria-pressed={mode === m}
          title={hint ? `${m.toUpperCase()} view (${hint})` : `${m.toUpperCase()} view`}
          onClick={() => onChange(m)}
          className={`rounded px-2 py-1 text-sm font-medium ${
            mode === m ? "bg-white text-neutral-900 shadow-sm dark:bg-neutral-600 dark:text-white" : "text-neutral-600 dark:text-neutral-400"
          }`}
        >
          {m.toUpperCase()}
        </button>
      ))}
    </div>
  );
}
