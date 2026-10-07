import { GitCompare, Upload } from "lucide-react";
import type { MouseEvent } from "react";
import { Button } from "../ui";

/** "Compare with this device" and "Send setup to this device…", for a controller in the show. */
export function SetupButtons({ name, onCompare, onSend, compact = false }: { name: string; onCompare: () => void; onSend: () => void; compact?: boolean }) {
  const stop = (run: () => void) => (e: MouseEvent) => {
    e.stopPropagation();
    run();
  };
  return (
    <>
      <Button variant={compact ? "ghost" : "secondary"} className={compact ? "px-2 py-1 text-xs" : ""} onClick={stop(onCompare)} aria-label={`Compare with this device: ${name}`} title="Read this controller and take its differences into your show (the controller isn't changed)">
        <GitCompare size={14} aria-hidden /> {compact ? "Compare" : "Compare with this device"}
      </Button>
      <Button variant={compact ? "ghost" : "secondary"} className={compact ? "px-2 py-1 text-xs" : ""} onClick={stop(onSend)} aria-label={`Send setup to this device: ${name}…`} title="See what sending your show's setup would change on this controller, then send it if you want">
        <Upload size={14} aria-hidden /> {compact ? "Send setup…" : "Send setup to this device…"}
      </Button>
    </>
  );
}
