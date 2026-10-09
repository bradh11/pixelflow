import { useEffect, useState } from "react";
import type { AudioProgress, AudioTask } from "../api/types";
import { useApp } from "./store";

/**
 * How far `task` has got on a music file (on `path`, when given), as the backend reports it;
 * null when nothing of the kind is under way. The last report (fraction 1) ends it, done or not.
 */
export function useAudioProgress(task: AudioTask, path?: string | null): AudioProgress | null {
  const backend = useApp((s) => s.backend);
  const [progress, setProgress] = useState<AudioProgress | null>(null);
  useEffect(() => {
    setProgress(null);
    if (!backend) return;
    let live = true;
    let stop: (() => void) | null = null;
    void backend
      .onAudioProgress((p) => {
        if (!live || p.task !== task || (path != null && p.path !== path)) return;
        setProgress(p.fraction >= 1 ? null : p);
      })
      .then((unlisten) => {
        if (live) stop = unlisten;
        else unlisten();
      });
    return () => {
      live = false;
      stop?.();
    };
  }, [backend, task, path]);
  return progress;
}
