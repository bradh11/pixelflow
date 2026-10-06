import { Check, X } from "lucide-react";
import { useEffect, useMemo, useRef } from "react";
import type { PreviewProp } from "../../api/types";
import { useAssistant } from "../../state/assistant";
import { PreviewCanvas } from "../PreviewCanvas";
import { Button } from "../ui";

/** Highlight for props the draft adds or changes, and a dim color for the rest. */
const CHANGED: [number, number, number] = [255, 176, 32];
const UNCHANGED: [number, number, number] = [55, 55, 66];

/** A frame that colors changed props and dims the others, for the preview canvas. */
export function highlightFrame(props: PreviewProp[], changed: string[]): Uint8Array {
  const marked = new Set(changed);
  const size = Math.max(0, ...props.map((p) => p.frameOffset + (p.points.length / 2) * p.channelsPerPixel));
  const frame = new Uint8Array(size);
  for (const p of props) {
    const [r, g, b] = marked.has(p.prop) ? CHANGED : UNCHANGED;
    for (let n = 0; n < p.points.length / 2; n++) {
      const at = p.frameOffset + n * p.channelsPerPixel;
      frame[at] = r;
      frame[at + 1] = g;
      frame[at + 2] = b;
    }
  }
  return frame;
}

/**
 * The assistant's draft drawn like the layout, before anything is applied: added and changed props
 * glow amber, everything else is dimmed. Apply and Discard work from here too.
 */
export function DraftPreview() {
  const preview = useAssistant((s) => s.preview);
  const proposal = useAssistant((s) => s.proposal);
  const busy = useAssistant((s) => s.busy);
  const { hidePreview, apply, discard } = useAssistant.getState();
  const closeRef = useRef<HTMLButtonElement>(null);
  const frame = useMemo(
    () => (preview && proposal ? highlightFrame(preview.props, proposal.changedProps) : null),
    [preview, proposal],
  );

  useEffect(() => {
    if (!preview) return;
    closeRef.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") hidePreview();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [preview, hidePreview]);

  if (!preview || !proposal) return null;
  return (
    <div className="fixed inset-0 z-40 flex items-center justify-center bg-black/50 p-6">
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="draft-preview-title"
        className="flex h-full max-h-[48rem] w-full max-w-5xl flex-col rounded-xl border border-neutral-200 bg-white shadow-2xl dark:border-neutral-800 dark:bg-neutral-900"
      >
        <div className="flex items-center gap-3 border-b border-neutral-200 px-4 py-3 dark:border-neutral-800">
          <div className="min-w-0">
            <h2 id="draft-preview-title" className="font-semibold">
              Preview: not applied yet
            </h2>
            <p className="truncate text-sm text-neutral-500 dark:text-neutral-400">{proposal.summary}</p>
          </div>
          <Button ref={closeRef} variant="ghost" className="ml-auto" aria-label="Close preview" onClick={hidePreview}>
            <X size={16} aria-hidden />
          </Button>
        </div>
        <div className="min-h-0 flex-1 p-3">
          <div className="h-full overflow-hidden rounded-lg">
            <p className="sr-only">
              The layout with the draft applied. {proposal.changedProps.length} added or changed props are highlighted.
            </p>
            <PreviewCanvas props={preview.props} frame={frame} />
          </div>
        </div>
        <div className="flex items-center gap-4 border-t border-neutral-200 px-4 py-3 text-sm dark:border-neutral-800">
          <span className="flex items-center gap-1.5">
            <span className="h-3 w-3 rounded-full" style={{ background: `rgb(${CHANGED.join(",")})` }} aria-hidden /> Added or changed
          </span>
          <span className="flex items-center gap-1.5 text-neutral-500">
            <span className="h-3 w-3 rounded-full" style={{ background: `rgb(${UNCHANGED.join(",")})` }} aria-hidden /> Unchanged
          </span>
          <div className="ml-auto flex gap-2">
            <Button variant="danger" disabled={busy} onClick={() => void discard()}>
              Discard
            </Button>
            <Button variant="primary" disabled={busy} onClick={() => void apply()}>
              <Check size={14} aria-hidden /> Apply
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}
