import { ChevronDown, Plus, Shapes } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { PreviewProp } from "../../api/types";
import { PROP_KINDS } from "../../lib/shows";
import { addPropInView } from "../../state/addProp";
import { Button } from "../ui";
import { MORE_TOOLS, TOOLS } from "./LayoutToolbar";

const ICONS = new Map([...TOOLS, ...MORE_TOOLS].map((t) => [t.tool, t.icon]));

/**
 * "Add prop ▾": pick a kind and it lands in the middle of the canvas, selected. (To draw one where
 * it goes instead, pick its tool on the toolbar.)
 */
export function AddPropMenu({ preview }: { preview: PreviewProp[] }) {
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (!open) return;
    box.current?.querySelector<HTMLButtonElement>("[role=menuitem]")?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        setOpen(false);
        trigger.current?.focus();
      }
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        const items = [...(box.current?.querySelectorAll<HTMLButtonElement>("[role=menuitem]") ?? [])];
        const at = items.indexOf(document.activeElement as HTMLButtonElement);
        items[(at + (e.key === "ArrowDown" ? 1 : -1) + items.length) % items.length]?.focus();
        e.preventDefault();
        e.stopPropagation();
      }
    };
    const onDown = (e: PointerEvent) => {
      if (!box.current?.contains(e.target as Node)) setOpen(false);
    };
    // Ahead of the layout keys, so Escape and the arrows only work the menu.
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("pointerdown", onDown);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("pointerdown", onDown);
    };
  }, [open]);
  return (
    <div
      ref={box}
      className="relative"
      onBlur={(e) => {
        if (open && !box.current?.contains(e.relatedTarget as Node | null)) setOpen(false);
      }}
    >
      <Button ref={trigger} variant="primary" aria-haspopup="menu" aria-expanded={open} title="Add a prop in the middle of the canvas" onClick={() => setOpen(!open)}>
        <Plus size={16} aria-hidden /> Add prop <ChevronDown size={14} aria-hidden />
      </Button>
      {open && (
        <div
          role="menu"
          aria-label="Add prop"
          className="absolute top-full right-0 z-40 mt-1 flex w-64 flex-col rounded-lg border border-neutral-200 bg-white p-1 text-sm text-neutral-800 shadow-xl dark:border-neutral-800 dark:bg-neutral-900 dark:text-neutral-100"
        >
          <div className="grid grid-cols-2 gap-0.5">
            {PROP_KINDS.map(({ kind, label }) => {
              const Icon = ICONS.get(kind) ?? Shapes;
              return (
                <button
                  key={kind}
                  type="button"
                  role="menuitem"
                  className="flex items-center gap-2 rounded px-2 py-1.5 text-left hover:bg-neutral-100 focus:bg-neutral-100 dark:hover:bg-neutral-800 dark:focus:bg-neutral-800"
                  onClick={() => {
                    setOpen(false);
                    void addPropInView(kind, preview);
                  }}
                >
                  <Icon size={16} aria-hidden className="shrink-0 text-neutral-500" />
                  <span className="truncate">{label}</span>
                </button>
              );
            })}
          </div>
          <p className="mt-1 border-t border-neutral-200 px-2 pt-1.5 pb-1 text-xs text-neutral-500 dark:border-neutral-800">
            Lands in the middle of the canvas. To draw a prop right where it goes, pick its tool on the toolbar and drag.
          </p>
        </div>
      )}
    </div>
  );
}
