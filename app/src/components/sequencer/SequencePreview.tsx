import { Maximize2, PanelRight, PanelTop } from "lucide-react";
import { useMemo } from "react";
import type { Sequence } from "../../api/sequence";
import { backgroundBox, boxOfPoints, unionBox } from "../../lib/layoutMath";
import { usePreviewWindow } from "../../state/previewWindow";
import { useSequencer } from "../../state/sequencer";
import { useApp } from "../../state/store";
import { useView3d } from "../../state/view3d";
import { useBackgroundImage, usePreviewProps } from "../layout/useLayoutData";
import { GlowControl } from "../layout3d/GlowControl";
import { ModeSwitch } from "../layout3d/ModeSwitch";
import { PopOutButton } from "../preview/PopOut";
import { PreviewStage } from "../preview/PreviewStage";
import { IconButton } from "../ui";
import { useSequenceFrame } from "./useSequenceFrame";

/** The display's width over its height (props and photo), for sizing the preview to it. */
export function useDisplayAspect(): number {
  const show = useApp((s) => s.snapshot?.show);
  const preview = usePreviewProps();
  const photo = useBackgroundImage(show?.background?.path);
  return useMemo(() => {
    const bg = show?.background ?? null;
    const box = unionBox([...preview.props.map((p) => boxOfPoints(p.points)), bg ? backgroundBox(bg, photo.aspect) : null]);
    if (!box) return 16 / 9;
    const w = box.maxX - box.minX;
    const h = box.maxY - box.minY;
    return w > 0 && h > 0 ? Math.min(4, Math.max(0.5, w / h)) : 16 / 9;
  }, [show?.background, preview.props, photo.aspect]);
}

/** "Selected row only": the preview shows just the pixels of the row being worked on. */
export function RowOnlyToggle() {
  const rowOnly = usePreviewWindow((s) => s.rowOnly);
  const setRowOnly = usePreviewWindow((s) => s.setRowOnly);
  return (
    <label className="flex items-center gap-1.5 text-xs whitespace-nowrap text-neutral-600 dark:text-neutral-300" title="Show only the pixels of the row being worked on">
      <input type="checkbox" checked={rowOnly} onChange={(e) => setRowOnly(e.target.checked)} />
      Selected row only
    </label>
  );
}

/**
 * The show as it looks at the playhead, flat like the Layout screen or in 3D (view only, with the
 * show's camera from the Layout and Play screens): rendered by the engine for the current moment
 * while editing, and live while playing. "Selected row only" shows just the pixels of the row
 * being worked on (only a submodel's own pixels, for a row on a submodel).
 */
export function SequencePreview({
  doc,
  expanded,
  onExpand,
  place,
  onPlace,
}: {
  doc: Sequence;
  expanded?: boolean;
  onExpand?: (expanded: boolean) => void;
  /** Where the preview sits, and how to move it (only where it can go beside the timeline). */
  place?: "side" | "top";
  onPlace?: (place: "side" | "top") => void;
}) {
  const activeRow = useSequencer((s) => s.activeRow);
  const mode = useView3d((s) => s.sequenceMode);
  const setMode = useView3d((s) => s.setSequenceMode);
  const onlyRow = usePreviewWindow((s) => s.rowOnly);
  const aspect = useDisplayAspect();
  const frame = useSequenceFrame(doc.durationMs, doc);
  const only = useMemo(() => (onlyRow ? (doc.rows.find((r) => r.id === activeRow)?.target ?? null) : null), [onlyRow, doc.rows, activeRow]);
  const quiet = "rounded-md p-1 text-neutral-600 hover:bg-neutral-200/70 dark:text-neutral-300 dark:hover:bg-neutral-800";

  return (
    <section aria-label="Preview" className="flex h-full w-full flex-col gap-1.5">
      <div className="flex shrink-0 flex-wrap items-center gap-x-3 gap-y-1">
        <span className="inline-flex items-center gap-0.5">
          <ModeSwitch mode={mode} onChange={setMode} />
          {/* Beside the timeline the column is narrow, at the window's edge. */}
          <GlowControl iconOnly={place === "side"} align={place === "side" ? "right" : "left"} />
        </span>
        <RowOnlyToggle />
        <span className="ml-auto inline-flex items-center gap-0.5">
          {onExpand && (
            <button
              type="button"
              aria-pressed={expanded}
              aria-label="Bigger preview"
              title={expanded ? "Give the timeline its room back" : "Give the preview most of the screen"}
              onClick={() => onExpand(!expanded)}
              className={`inline-flex items-center gap-1 rounded-md px-2 py-1 text-xs text-neutral-600 hover:bg-neutral-200/70 dark:text-neutral-300 dark:hover:bg-neutral-800 ${
                expanded ? "bg-accent-50 text-accent-600 dark:bg-accent-600/15 dark:text-accent-400" : ""
              }`}
            >
              {/* Beside the timeline the column is narrow: the icon alone, named on hover. */}
              <Maximize2 size={13} aria-hidden /> <span className={place === "side" ? "sr-only" : undefined}>Bigger preview</span>
            </button>
          )}
          {onPlace && (
            <IconButton label={place === "side" ? "Preview above the timeline" : "Preview beside the timeline"} className={quiet} onClick={() => onPlace(place === "side" ? "top" : "side")}>
              {place === "side" ? <PanelTop size={14} aria-hidden /> : <PanelRight size={14} aria-hidden />}
            </IconButton>
          )}
          <PopOutButton className={quiet} />
        </span>
      </div>
      <div
        className={`relative min-h-0 ${place === "side" ? "w-full" : "flex-1"}`}
        // Beside the timeline: as tall as the display needs at the column's width, within reason.
        style={place === "side" ? { aspectRatio: `${aspect} / 1`, maxHeight: expanded ? "75vh" : "50vh", minHeight: 120 } : undefined}
      >
        <PreviewStage frame={frame} mode={mode} only={only} />
      </div>
    </section>
  );
}
