import { Bold, Italic } from "lucide-react";
import { useEffect, useId, useMemo, useState } from "react";
import type { SequencerApi } from "../../api/sequencer";
import { useSequencer } from "../../state/sequencer";
import { FIELD, MIXED } from "./effectControls";

/** What the font list calls PixelFlow's own pixel font (the effect stores it as ""). */
export const PIXEL_FONT = "Pixel font";

/** The most fonts the list shows at once: typing narrows it. */
const MOST_SHOWN = 300;

const asked = new WeakMap<SequencerApi, Promise<string[]>>();

/** The computer's font families, sorted, asked for once (null until they've come). */
export function useFonts(): string[] | null {
  const api = useSequencer((s) => s.api);
  const [fonts, setFonts] = useState<string[] | null>(null);
  useEffect(() => {
    if (!api) return;
    let live = true;
    let list = asked.get(api);
    if (!list) {
      list = api.listFonts().catch(() => []);
      asked.set(api, list);
    }
    void list.then((found) => live && setFonts(found));
    return () => {
      live = false;
    };
  }, [api]);
  return fonts;
}

/** The family as the computer spells it, when it has `name` (any case). */
export function installedFont(fonts: string[] | null, name: string): string | undefined {
  const wanted = name.trim().toLowerCase();
  return fonts?.find((f) => f.toLowerCase() === wanted);
}

/** A CSS font-family for a family's name: the list shows each name in its own lettering. */
function family(name: string): string {
  return `${JSON.stringify(name)}, sans-serif`;
}

/** Bold or italic, beside the font. */
function StyleToggle({ label, on, disabled, onChange, children }: { label: string; on: boolean; disabled: boolean; onChange: (on: boolean) => void; children: React.ReactNode }) {
  return (
    <button
      type="button"
      aria-label={label}
      aria-pressed={on}
      disabled={disabled}
      title={disabled ? `${label} is for a computer font` : label}
      onClick={() => onChange(!on)}
      className={`rounded-md border p-1.5 disabled:cursor-not-allowed disabled:opacity-40 ${
        on
          ? "border-accent-600 bg-accent-600/10 text-accent-700 dark:text-accent-300"
          : "border-neutral-300 bg-white text-neutral-600 hover:bg-neutral-100 dark:border-neutral-700 dark:bg-neutral-950 dark:text-neutral-300 dark:hover:bg-neutral-800"
      }`}
    >
      {children}
    </button>
  );
}

/**
 * The Text effect's font: a list of the computer's font families to search, with PixelFlow's
 * own pixel font first, and (with `styles`) bold and italic beside it. A font this computer
 * doesn't have is shown as it is named, with a line saying the pixel font is drawn instead.
 */
export function FontSetting({
  label,
  description,
  value,
  onChange,
  mixed = false,
  styles,
}: {
  label: string;
  description: string;
  value: string;
  onChange: (value: string) => void;
  mixed?: boolean;
  styles?: { bold: boolean; italic: boolean; onBold: (on: boolean) => void; onItalic: (on: boolean) => void };
}) {
  const fonts = useFonts();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const id = useId();
  const listId = `${id}-fonts`;
  const current = mixed ? "" : value.trim();
  const known = installedFont(fonts, current);
  const missing = !mixed && current !== "" && fonts !== null && known === undefined;
  const choices = useMemo(() => {
    const q = query.trim().toLowerCase();
    const all = [{ value: "", label: PIXEL_FONT }, ...(fonts ?? []).map((f) => ({ value: f, label: f }))];
    return (q ? all.filter((c) => c.label.toLowerCase().includes(q)) : all).slice(0, MOST_SHOWN);
  }, [fonts, query]);
  useEffect(() => {
    if (open) document.getElementById(`${listId}-${active}`)?.scrollIntoView?.({ block: "nearest" });
  }, [open, active, listId]);

  const choose = (font: string | undefined) => {
    setOpen(false);
    setQuery("");
    if (font !== undefined && (mixed || font !== current)) onChange(font);
  };
  const shown = mixed ? "" : (known ?? (current || PIXEL_FONT));
  const pixel = !mixed && current === "";

  return (
    <div className="flex flex-col gap-1 text-sm" title={description}>
      <label htmlFor={`${id}-input`} className="text-neutral-600 dark:text-neutral-400">
        {label}
      </label>
      <div className="flex items-center gap-1">
        <input
          id={`${id}-input`}
          role="combobox"
          aria-expanded={open}
          aria-controls={listId}
          aria-autocomplete="list"
          aria-activedescendant={open && choices[active] ? `${listId}-${active}` : undefined}
          autoComplete="off"
          spellCheck={false}
          className={`${FIELD} min-w-0 flex-1`}
          style={!open && !pixel && known ? { fontFamily: family(known) } : undefined}
          value={open ? query : shown}
          placeholder={open ? "Find a font" : mixed ? MIXED : undefined}
          onFocus={() => {
            setOpen(true);
            setQuery("");
            setActive(Math.max(0, choices.findIndex((c) => c.value.toLowerCase() === current.toLowerCase())));
          }}
          onBlur={() => {
            setOpen(false);
            setQuery("");
          }}
          onChange={(e) => {
            setQuery(e.target.value);
            setActive(0);
            setOpen(true);
          }}
          onKeyDown={(e) => {
            if (e.key === "ArrowDown") setActive((a) => Math.min(choices.length - 1, a + 1));
            else if (e.key === "ArrowUp") setActive((a) => Math.max(0, a - 1));
            else if (e.key === "Enter" && open) choose(choices[active]?.value);
            else if (e.key === "Escape" && open) {
              setOpen(false);
              setQuery("");
            } else return;
            e.preventDefault();
            e.stopPropagation();
          }}
        />
        {styles && (
          <>
            <StyleToggle label="Bold" on={styles.bold} disabled={pixel} onChange={styles.onBold}>
              <Bold size={14} aria-hidden />
            </StyleToggle>
            <StyleToggle label="Italic" on={styles.italic} disabled={pixel} onChange={styles.onItalic}>
              <Italic size={14} aria-hidden />
            </StyleToggle>
          </>
        )}
      </div>
      {open && (
        <ul
          id={listId}
          role="listbox"
          aria-label="Fonts"
          className="max-h-48 overflow-auto rounded-md border border-neutral-200 bg-white py-0.5 dark:border-neutral-700 dark:bg-neutral-900"
        >
          {fonts === null && <li className="px-2 py-1 text-neutral-500">Finding this computer's fonts…</li>}
          {fonts !== null && choices.length === 0 && <li className="px-2 py-1 text-neutral-500">No font on this computer is called that.</li>}
          {choices.map((c, i) => (
            <li
              key={c.value || "\u0000pixel"}
              id={`${listId}-${i}`}
              role="option"
              aria-selected={!mixed && c.value.toLowerCase() === current.toLowerCase()}
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => choose(c.value)}
              onMouseEnter={() => setActive(i)}
              style={c.value ? { fontFamily: family(c.value) } : undefined}
              className={`cursor-pointer truncate px-2 py-0.5 ${i === active ? "bg-accent-600/10" : ""} ${c.value ? "" : "font-mono"}`}
            >
              {c.label}
            </li>
          ))}
        </ul>
      )}
      {missing && (
        <span className="text-xs text-amber-700 dark:text-amber-400">
          {current} isn't on this computer: shown in PixelFlow's pixel font.
        </span>
      )}
    </div>
  );
}
