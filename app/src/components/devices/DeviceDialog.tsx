import { type ReactNode, useEffect, useRef } from "react";
import type { ColorOrder, Prop, Show } from "../../api/types";
import { doubleReorder, mappingProblem } from "../../lib/deviceSetup";
import { thousands } from "../../lib/format";
import { nodeCount } from "../../lib/shows";
import { Select } from "../ui";

/**
 * A device dialog: a title over scrolling content and a row of buttons. Escape closes it unless
 * `busy` (while something is being sent). The first button with `data-autofocus` gets focus.
 */
export function DeviceDialog({
  title,
  subtitle,
  busy = false,
  onClose,
  footer,
  children,
}: {
  title: string;
  subtitle?: string;
  busy?: boolean;
  onClose: () => void;
  footer: ReactNode;
  children: ReactNode;
}) {
  const box = useRef<HTMLDivElement>(null);
  const busyRef = useRef(busy);
  busyRef.current = busy;

  useEffect(() => {
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    box.current?.querySelector<HTMLElement>("[data-autofocus]")?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || busyRef.current) return;
      e.preventDefault();
      e.stopPropagation();
      onClose();
    };
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      if (opener?.isConnected) opener.focus();
    };
  }, [onClose]);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4">
      <div
        ref={box}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        className="flex max-h-[85vh] w-full max-w-2xl flex-col rounded-xl border border-neutral-200 bg-white shadow-2xl dark:border-neutral-800 dark:bg-neutral-900"
      >
        <div className="border-b border-neutral-200 px-5 py-4 dark:border-neutral-800">
          <h2 className="text-lg font-semibold">{title}</h2>
          {subtitle && <p className="mt-0.5 text-sm text-neutral-500">{subtitle}</p>}
        </div>
        <div className="flex-1 overflow-auto px-5 py-4 text-sm">{children}</div>
        <div className="flex flex-wrap justify-end gap-2 border-t border-neutral-200 p-4 dark:border-neutral-800">{footer}</div>
      </div>
    </div>
  );
}

/** Where a prop is wired in the show, as "Main FPP port 1", or null. */
function wiredAt(show: Show, prop: Prop): string | null {
  for (const c of show.controllers) {
    const port = c.ports.find((p) => p.slots.some((s) => s.prop === prop.id));
    if (port) return `${c.name} port ${port.number}`;
  }
  return null;
}

/**
 * Picks what a device string becomes in the show: a new starter prop (the default), or a prop
 * already in the show (one imported from xLights, say).
 */
export function PropPicker({
  show,
  label,
  pixels,
  order,
  value,
  onChange,
}: {
  show: Show;
  label: string;
  pixels: number;
  /** The color order the controller applies to the string, when known. */
  order: ColorOrder | null;
  value: string;
  onChange: (id: string) => void;
}) {
  const props = [...show.props].sort((a, b) => a.name.localeCompare(b.name));
  const chosen = props.find((p) => p.id === value);
  const size = chosen ? nodeCount(chosen.shape) : null;
  const problem = chosen && order ? mappingProblem(chosen, order) : null;
  const twice = chosen && order && !problem ? doubleReorder(chosen, order) : null;
  return (
    <span className="mt-1 flex flex-wrap items-center gap-2 text-xs">
      <Select aria-label={label} value={value} onChange={(e) => onChange(e.target.value)} className="max-w-72 py-0.5 text-xs">
        <option value="">A new prop</option>
        {props.map((p) => {
          const at = wiredAt(show, p);
          return (
            <option key={p.id} value={p.id}>
              {p.name} · {thousands(nodeCount(p.shape))} px{at ? ` (on ${at})` : ""}
            </option>
          );
        })}
      </Select>
      {chosen && size !== pixels && !problem && (
        <span className="text-amber-700 dark:text-amber-400">
          {chosen.name} has {thousands(size ?? 0)} pixels; this string has {thousands(pixels)}.
        </span>
      )}
      {problem && (
        <span role="alert" className="text-red-700 dark:text-red-400">
          {chosen!.name} can't be wired here: {problem}
        </span>
      )}
      {twice && <span className="text-amber-700 dark:text-amber-400">{twice}</span>}
    </span>
  );
}
