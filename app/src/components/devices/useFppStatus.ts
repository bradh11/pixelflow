import { useCallback, useEffect, useState } from "react";
import { errorMessage } from "../../api/backend";
import type { PlayerStatus } from "../../api/types";
import { useApp } from "../../state/store";

/** How often the status is read while the page is on screen. */
export const STATUS_POLL_MS = 1000;
/** How long to wait before trying again after the FPP didn't answer. */
export const STATUS_RETRY_MS = 5000;

const hidden = () => document.visibilityState === "hidden";

/**
 * What an FPP is playing, read about once a second while the page is mounted and the window is
 * visible (only `/api/fppd/status`, which changes nothing). Reading stops while the window is
 * hidden and starts again when it's back; `refresh` reads again now.
 */
export function useFppStatus(address: string) {
  const backend = useApp((s) => s.backend);
  const [status, setStatus] = useState<PlayerStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [turn, setTurn] = useState(0);

  useEffect(() => {
    if (!backend) return;
    let alive = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let reading = false;
    const read = async () => {
      timer = undefined;
      if (!alive || hidden() || reading) return;
      reading = true;
      let wait = STATUS_POLL_MS;
      try {
        const next = await backend.fppStatus(address);
        if (alive) {
          setStatus(next);
          setError(null);
        }
      } catch (e) {
        wait = STATUS_RETRY_MS;
        if (alive) {
          // Don't keep showing (or offering to stop) a state that can't be confirmed.
          setStatus(null);
          setError(errorMessage(e));
        }
      }
      reading = false;
      if (alive && !hidden()) timer = setTimeout(read, wait);
    };
    const onVisibility = () => {
      if (hidden()) {
        clearTimeout(timer);
        timer = undefined;
      } else if (timer === undefined) {
        void read();
      }
    };
    document.addEventListener("visibilitychange", onVisibility);
    void read();
    return () => {
      alive = false;
      clearTimeout(timer);
      document.removeEventListener("visibilitychange", onVisibility);
    };
  }, [backend, address, turn]);

  const refresh = useCallback(() => setTurn((t) => t + 1), []);
  return { status, error, refresh };
}
