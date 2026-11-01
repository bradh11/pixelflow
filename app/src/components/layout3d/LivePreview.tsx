import { useView3d } from "../../state/view3d";
import { PreviewStage } from "../preview/PreviewStage";
import { GlowControl } from "./GlowControl";
import { ModeSwitch } from "./ModeSwitch";

/**
 * The props in their live colors, flat (front view) or in 3D as the Play screen was last left.
 * The 3D view is look-only here and shares the Layout screen's camera for the show. `playing`:
 * the sequence sending them is playing (not paused).
 */
export function LivePreview({ frame, playing = false }: { frame: Uint8Array | null; playing?: boolean }) {
  const mode = useView3d((s) => s.playMode);
  return <PreviewStage frame={frame} mode={mode} playing={playing} label="Preview of the lights" />;
}

/**
 * How the Play screen's preview looks: the viewer's glow, and the switch between 2D and 3D
 * (remembered). `drop`: which way the glow's panel opens.
 */
export function LiveViewControls({ drop = "down" }: { drop?: "down" | "up" }) {
  const mode = useView3d((s) => s.playMode);
  const setMode = useView3d((s) => s.setPlayMode);
  return (
    <span className="inline-flex shrink-0 items-center gap-1">
      <GlowControl align="right" drop={drop} iconOnly />
      <ModeSwitch mode={mode} onChange={setMode} />
    </span>
  );
}
