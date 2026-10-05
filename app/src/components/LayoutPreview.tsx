import { useEffect, useState } from "react";
import type { PreviewProp } from "../api/types";
import { useApp } from "../state/store";
import { PreviewCanvas } from "./PreviewCanvas";

/** How often the preview picks up colors from a running test pattern or sequence. */
const FRAME_MS = 100;

/** The show's props drawn where they are in the layout, lit when something is playing. */
export function LayoutPreview() {
  const backend = useApp((s) => s.backend);
  const revision = useApp((s) => s.snapshot?.revision);
  const [props, setProps] = useState<PreviewProp[]>([]);
  const [frame, setFrame] = useState<Uint8Array | null>(null);

  useEffect(() => {
    if (!backend) return;
    void backend.previewProps().then(setProps, () => setProps([]));
  }, [backend, revision]);

  useEffect(() => {
    if (!backend) return;
    let cancelled = false;
    let pending = false;
    const timer = setInterval(() => {
      if (pending) return;
      pending = true;
      backend.liveFrame().then(
        (f) => {
          pending = false;
          if (!cancelled) setFrame(f.length ? f : null);
        },
        () => {
          pending = false;
        },
      );
    }, FRAME_MS);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [backend]);

  if (props.length === 0) return null;
  return (
    <div className="mb-6 h-80">
      <PreviewCanvas props={props} frame={frame} />
    </div>
  );
}
