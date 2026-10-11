import { GitCompare, Upload } from "lucide-react";
import type { MouseEvent } from "react";
import { FALCON_READ_ONLY } from "../../lib/labels";
import { Button } from "../ui";

/**
 * "Compare with this controller" and "Send setup to this controller…", for a controller in the
 * show. `readOnlyFalcon`: PixelFlow can read a Falcon's setup but not write it, so Send is off,
 * with the reason on hover.
 */
export function SetupButtons({
  name,
  onCompare,
  onSend,
  compact = false,
  readOnlyFalcon = false,
}: {
  name: string;
  onCompare: () => void;
  onSend: () => void;
  compact?: boolean;
  readOnlyFalcon?: boolean;
}) {
  const stop = (run: () => void) => (e: MouseEvent) => {
    e.stopPropagation();
    run();
  };
  return (
    <>
      <Button variant={compact ? "ghost" : "secondary"} className={compact ? "px-2 py-1 text-xs" : ""} onClick={stop(onCompare)} aria-label={`Compare with this controller: ${name}`} title="Read this controller and take its differences into your show (the controller isn't changed)">
        <GitCompare size={14} aria-hidden /> {compact ? "Compare" : "Compare with this controller"}
      </Button>
      <Button
        variant={compact ? "ghost" : "secondary"}
        className={compact ? "px-2 py-1 text-xs" : ""}
        onClick={stop(onSend)}
        disabled={readOnlyFalcon}
        aria-label={`Send setup to this controller: ${name}…`}
        title={readOnlyFalcon ? FALCON_READ_ONLY : "See what sending your show's setup would change on this controller, then send it if you want"}
      >
        <Upload size={14} aria-hidden /> {compact ? "Send setup…" : "Send setup to this controller…"}
      </Button>
    </>
  );
}
