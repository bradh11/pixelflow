import { useCallback, useEffect, useRef, useState } from "react";
import type { PlayerStatus } from "../../api/types";
import { useApp } from "../../state/store";

const FPP_MS = 3000;

export interface BusyFpp {
  address: string;
  name: string;
  status: PlayerStatus;
}

/** FPPs found by discovery that are playing right now (they override PixelFlow's output). */
export function useBusyFpps(): {
  busy: BusyFpp[];
  /** Checks the FPPs again now (after the user stops one). */
  recheck: () => Promise<void>;
} {
  const backend = useApp((s) => s.backend);
  const discovery = useApp((s) => s.discovery);
  const [busy, setBusy] = useState<BusyFpp[]>([]);
  const checkRef = useRef<() => Promise<void>>(async () => {});
  useEffect(() => {
    const fpps = discovery?.devices.filter((d) => d.kind === "fpp" && d.responding) ?? [];
    if (!backend || fpps.length === 0) {
      setBusy([]);
      return;
    }
    let cancelled = false;
    const check = async () => {
      const results = await Promise.all(
        fpps.map(async (d) => {
          try {
            return { address: d.address, name: d.name, status: await backend.fppStatus(d.address) };
          } catch {
            return null;
          }
        }),
      );
      if (!cancelled) setBusy(results.filter((r) => r !== null && r.status.state === "playing") as BusyFpp[]);
    };
    checkRef.current = check;
    void check();
    const timer = setInterval(() => void check(), FPP_MS);
    return () => {
      cancelled = true;
      checkRef.current = async () => {};
      clearInterval(timer);
    };
  }, [backend, discovery]);
  const recheck = useCallback(() => checkRef.current(), []);
  return { busy, recheck };
}
