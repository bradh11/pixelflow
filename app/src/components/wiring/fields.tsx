import { useEffect, useState } from "react";
import { Input } from "../ui";

/**
 * A number box that may be left empty (meaning "not set": `placeholder` says what applies
 * then). Saves on Enter or leaving it; goes back if what's typed isn't valid.
 */
export function OptionalNumberField({
  label,
  value,
  onCommit,
  min,
  max,
  integer = false,
  placeholder,
  hint,
}: {
  label: string;
  value: number | null;
  onCommit: (value: number | null) => void;
  min: number;
  max: number;
  integer?: boolean;
  placeholder: string;
  hint?: string;
}) {
  const shown = value === null ? "" : String(value);
  const [draft, setDraft] = useState(shown);
  useEffect(() => setDraft(shown), [shown]);
  const commit = () => {
    if (draft.trim() === "") {
      if (value !== null) onCommit(null);
      return;
    }
    const n = Number(draft);
    if (!Number.isFinite(n) || n < min || n > max || (integer && !Number.isInteger(n))) {
      setDraft(shown);
      return;
    }
    if (n !== value) onCommit(n);
  };
  return (
    <label className="flex flex-col gap-1 text-xs" title={hint}>
      <span className="truncate text-neutral-500 dark:text-neutral-400">{label}</span>
      <Input
        inputMode="decimal"
        value={draft}
        placeholder={placeholder}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") (e.target as HTMLInputElement).blur();
          if (e.key === "Escape") {
            setDraft(shown);
            e.stopPropagation();
          }
        }}
        className="w-full tabular-nums"
      />
    </label>
  );
}
