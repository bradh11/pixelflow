import { Maximize, Sparkles, Square } from "lucide-react";
import { useShallow } from "zustand/react/shallow";
import { PRESETS } from "../../lib/layout3d";
import { useView3d } from "../../state/view3d";

const button = "rounded px-2 py-1 text-xs text-neutral-200 hover:bg-white/10 aria-pressed:bg-white/15 aria-pressed:text-white";

/**
 * The 3D view's camera views and look, over its top-left corner (always dark, like the scene).
 * `keys`: the screen handles the camera keys (1–5, F), so the tooltips name them.
 */
export function View3dControls({ keys = false }: { keys?: boolean }) {
  const { camera, bloom, setBloom, ground, setGround } = useView3d(
    useShallow((s) => ({ camera: s.camera, bloom: s.bloom, setBloom: s.setBloom, ground: s.ground, setGround: s.setGround })),
  );
  return (
    <div role="toolbar" aria-label="3D view" className="absolute top-2 left-2 flex flex-wrap items-center gap-0.5 rounded-md bg-black/55 p-0.5 backdrop-blur-sm">
      {PRESETS.map(({ preset, label, key }) => (
        <button key={preset} type="button" className={button} title={keys ? `${label} view (${key})` : `${label} view`} onClick={() => camera({ kind: "preset", preset })}>
          {label}
        </button>
      ))}
      <span aria-hidden className="mx-0.5 h-4 w-px bg-white/20" />
      <button type="button" className={`${button} inline-flex items-center gap-1`} title={keys ? "Show the whole display (F)" : "Show the whole display"} onClick={() => camera({ kind: "fit" })}>
        <Maximize size={12} aria-hidden /> Fit
      </button>
      <span aria-hidden className="mx-0.5 h-4 w-px bg-white/20" />
      <button type="button" aria-pressed={bloom} className={`${button} inline-flex items-center gap-1`} title="Glow around lit pixels" onClick={() => setBloom(!bloom)}>
        <Sparkles size={12} aria-hidden /> Glow
      </button>
      <button type="button" aria-pressed={ground} className={`${button} inline-flex items-center gap-1`} title="Show the ground" onClick={() => setGround(!ground)}>
        <Square size={12} aria-hidden /> Ground
      </button>
    </div>
  );
}
