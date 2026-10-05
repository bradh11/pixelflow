import { useEffect, useState } from "react";
import type { PreviewProp } from "../api/types";
import { useApp } from "../state/store";
import { PreviewCanvas } from "./PreviewCanvas";

/** How often the preview picks up colors while a test pattern or sequence is running. */
const FRAME_MS = 100;
/** How often it checks whether something started, while nothing is running. */
const IDLE_MS = 1000;

/** The show's props drawn where they are in the layout, lit when something is playing. */
export function LayoutPreview() {
  const backend = useApp((s) => s.backend);
  const revision = useApp((s) => s.snapshot?.revision);
  const [props, setProps] = useState<PreviewProp[]>([]);
  const [frame, setFrame] = useState<Uint8Array | null>(null);

  useEffect(() => {
    if (!backend) return;
    // Only the latest request counts: an older answer arriving late is ignored.
    let latest = true;
    void backend.previewProps().then(
      (p) => latest && setProps(p),
      () => latest && setProps([]),
    );
    return () => {
      latest = false;
    };
  }, [backend, revision]);

  useEffect(() => {
    if (!backend) return;
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const poll = () => {
      backend.liveFrame().then(
        (f) => {
          if (cancelled) return;
          const lit = f.length > 0;
          setFrame(lit ? f : null);
          timer = setTimeout(poll, lit ? FRAME_MS : IDLE_MS);
        },
        () => {
          if (!cancelled) timer = setTimeout(poll, IDLE_MS);
        },
      );
    };
    timer = setTimeout(poll, FRAME_MS);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [backend]);

  if (props.length === 0) return null;
  return (
    <div className="mb-6 h-80">
      <PreviewCanvas props={props} frame={frame} />
    </div>
  );
}
