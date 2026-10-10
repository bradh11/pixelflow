import { Sparkles } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useView3d } from "../../state/view3d";

const words = (percent: number) => (percent === 0 ? "None" : `${percent}%`);

/**
 * How much lit pixels glow in the previews, for beside a preview's 2D | 3D switch: a small button
 * that opens a slider from None to 100%. It's how this viewer likes to look at the show (every
 * preview follows it, and it's remembered on this computer), not part of the show.
 *
 * `align`: the side of the button the slider's panel lines up with. `iconOnly`: no word on the
 * button, for a narrow place.
 */
export function GlowControl({ align = "left", iconOnly = false }: { align?: "left" | "right"; iconOnly?: boolean }) {
  const percent = useView3d((s) => Math.round(s.glow * 100));
  const setGlow = useView3d((s) => s.setGlow);
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    box.current?.querySelector<HTMLElement>("input")?.focus();
    // Ahead of the screen's own keys, so Escape only closes the panel.
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      setOpen(false);
      box.current?.querySelector<HTMLElement>("button")?.focus();
    };
    const onDown = (e: PointerEvent) => {
      if (!box.current?.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("pointerdown", onDown);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("pointerdown", onDown);
    };
  }, [open]);
  return (
    <div ref={box} className="relative inline-flex">
      <button
        type="button"
        aria-haspopup="dialog"
        aria-expanded={open}
        title={`Glow around lit pixels: ${words(percent)}`}
        onClick={() => setOpen(!open)}
        className={`inline-flex items-center gap-1 rounded-md px-1.5 py-1 text-xs hover:bg-neutral-200/70 dark:hover:bg-neutral-800 ${
          percent > 0 ? "text-accent-600 dark:text-accent-400" : "text-neutral-600 dark:text-neutral-300"
        }`}
      >
        <Sparkles size={14} aria-hidden />
        <span className={iconOnly ? "sr-only" : undefined}>Glow</span>
      </button>
      {open && (
        <div
          role="dialog"
          aria-label="Glow"
          className={`absolute top-full z-30 mt-1 w-56 rounded-lg border border-neutral-200 bg-white p-3 text-neutral-800 shadow-xl dark:border-neutral-800 dark:bg-neutral-900 dark:text-neutral-100 ${
            align === "right" ? "right-0" : "left-0"
          }`}
        >
          <label className="flex flex-col gap-1 text-xs">
            <span className="flex justify-between text-neutral-500 dark:text-neutral-400">
              <span>Glow</span>
              <span className="tabular-nums">{words(percent)}</span>
            </span>
            <input
              type="range"
              min={0}
              max={100}
              step={5}
              value={percent}
              aria-label="Glow"
              aria-valuetext={words(percent)}
              onChange={(e) => setGlow(Number(e.target.value) / 100)}
              className="accent-accent-500"
            />
            <span className="text-neutral-500">None for bare bulbs, more for lights behind diffusers.</span>
          </label>
        </div>
      )}
    </div>
  );
}
