import { ImagePlus, SlidersHorizontal, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { errorMessage } from "../api/backend";
import { AddPropMenu } from "../components/layout/AddPropMenu";
import { LayoutCanvas, type LayoutCanvasHandle } from "../components/layout/LayoutCanvas";
import { LayoutToolbar } from "../components/layout/LayoutToolbar";
import { PropertiesPanel } from "../components/layout/PropertiesPanel";
import { SidePanel } from "../components/layout/SidePanel";
import { FALLBACK_ASPECT, imageAspect, useBackgroundImage, usePreviewProps, usePreviewProps3d } from "../components/layout/useLayoutData";
import { useLayoutKeys } from "../components/layout/useLayoutKeys";
import { Layout3dView } from "../components/layout3d/Layout3dView";
import { useLayout3dKeys } from "../components/layout3d/useLayout3dKeys";
import { Button, IconButton, PageHeader } from "../components/ui";
import { useElementWidth } from "../lib/useWidth";
import { boxOfPoints, defaultBackground, unionBox } from "../lib/layoutMath";
import { useApp } from "../state/store";
import { showViewKey, useView3d } from "../state/view3d";

/** The least width the canvas keeps before the panels beside it give way. */
const MIN_CANVAS = 480;
const LIST_WIDTH = 256;
const RAIL_WIDTH = 36;
const PROPERTIES_WIDTH = 288;
const GAPS = 24;

export interface LayoutArrangement {
  /** "floating": put away beside the canvas, and shown over it when asked for. */
  list: "docked" | "floating";
  properties: "docked" | "floating";
}

/**
 * How the props list and the properties panel sit beside a canvas row `row` px wide (null: not
 * measured, so both stay docked). The list gives way first, then the properties.
 */
export function layoutArrangement(row: number | null): LayoutArrangement {
  if (row === null) return { list: "docked", properties: "docked" };
  return {
    list: row - LIST_WIDTH - PROPERTIES_WIDTH - GAPS >= MIN_CANVAS ? "docked" : "floating",
    properties: row - RAIL_WIDTH - PROPERTIES_WIDTH - GAPS >= MIN_CANVAS ? "docked" : "floating",
  };
}

/** Draw the display: props over a photo of the house, with the props and groups list and a properties panel. */
export function LayoutScreen() {
  const snapshot = useApp((s) => s.snapshot);
  const apply = useApp((s) => s.apply);
  const backend = useApp((s) => s.backend);
  const preview = usePreviewProps();
  const photo = useBackgroundImage(snapshot?.show.background?.path);
  const in3d = useView3d((s) => s.mode === "3d");
  const preview3d = usePreviewProps3d(in3d);
  const canvas = useRef<LayoutCanvasHandle>(null);
  const row = useRef<HTMLDivElement>(null);
  const arrangement = layoutArrangement(useElementWidth(row));
  const [propertiesShown, setPropertiesShown] = useState(false);
  const floatingProperties = arrangement.properties === "floating";
  useEffect(() => {
    if (!floatingProperties || !propertiesShown) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !e.defaultPrevented) setPropertiesShown(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [floatingProperties, propertiesShown]);
  useLayoutKeys(canvas);
  useLayout3dKeys();
  if (!snapshot) return null;
  const show = snapshot.show;

  const choosePhoto = async () => {
    if (!backend) return;
    const path = await backend.pickImagePath();
    if (!path) return;
    let aspect = FALLBACK_ASPECT;
    try {
      aspect = (await imageAspect(await backend.readImage(path), path)) ?? FALLBACK_ASPECT;
    } catch (e) {
      useApp.setState({ error: errorMessage(e) });
      return;
    }
    const props = unionBox(preview.props.map((p) => boxOfPoints(p.points)));
    const same = path === useApp.getState().snapshot?.show.background?.path;
    await apply((latest) => [
      { type: "setBackground", background: latest.background ? { ...latest.background, path } : defaultBackground(path, props, aspect) },
    ]);
    // The same file chosen again (it may have changed on disk): show it afresh.
    if (same) photo.reload();
  };

  return (
    <div className="flex min-h-full flex-col">
      <PageHeader
        title="Layout"
        description="Draw your display over a photo of your house, then wire each prop to a controller port."
        actions={
          <AddPropMenu preview={preview.props} />
        }
      />
      <LayoutToolbar hasPhoto={!!show.background} onChoosePhoto={() => void choosePhoto()} />
      <div ref={row} data-layout-row className="relative flex h-[max(26rem,calc(100vh-16.5rem))] gap-3">
        <SidePanel floating={arrangement.list === "floating"} />
        <div className="relative min-w-0 flex-1">
          {in3d ? (
            <Layout3dView ref={canvas} preview={preview3d} show={show} photo={photo} storageKey={showViewKey(snapshot.path, show.name)} editable />
          ) : (
            <LayoutCanvas ref={canvas} preview={preview} show={show} photo={photo} />
          )}
          {show.props.length === 0 && (
            <div className="pointer-events-none absolute inset-0 flex items-center justify-center p-6 text-center text-sm text-neutral-300">
              <div className="flex max-w-sm flex-col items-center gap-2 rounded-lg bg-black/60 px-4 py-3">
                <p>
                  {in3d
                    ? "Nothing here yet. Use Add prop above, or switch to 2D (V) to draw props."
                    : show.background
                      ? "Use Add prop, or pick a tool above, like Arch or Matrix, and drag on the photo where that prop is."
                      : "Use Add prop, or pick a tool above, like Arch or Matrix, and drag here to draw a prop. Draw right over a photo of your house:"}
                </p>
                {!in3d && !show.background && (
                  <Button variant="primary" className="pointer-events-auto" onClick={() => void choosePhoto()}>
                    <ImagePlus size={16} aria-hidden /> Add a photo of your house…
                  </Button>
                )}
              </div>
            </div>
          )}
        </div>
        {floatingProperties ? (
          <>
            <div className="flex w-9 shrink-0 flex-col items-center rounded-lg border border-neutral-200 bg-white py-1 dark:border-neutral-800 dark:bg-neutral-900">
              <IconButton
                label="Show properties"
                hint="Show the selected prop's properties (or the photo settings)"
                aria-expanded={propertiesShown}
                className="rounded p-1.5 text-neutral-600 hover:bg-neutral-200/70 dark:text-neutral-300 dark:hover:bg-neutral-800"
                onClick={() => setPropertiesShown(!propertiesShown)}
              >
                <SlidersHorizontal size={16} aria-hidden />
              </IconButton>
            </div>
            {propertiesShown && (
              <div className="absolute inset-y-0 right-12 z-20 w-72 rounded-lg shadow-2xl">
                <PropertiesPanel preview={preview.props} photoProblem={photo.problem} onRetryPhoto={photo.reload} onChoosePhoto={() => void choosePhoto()} />
                <IconButton
                  label="Put properties away"
                  className="absolute top-1.5 right-1.5 rounded p-1 text-neutral-500 hover:bg-neutral-200/70 dark:hover:bg-neutral-800"
                  onClick={() => setPropertiesShown(false)}
                >
                  <X size={14} aria-hidden />
                </IconButton>
              </div>
            )}
          </>
        ) : (
          <div className="w-72 shrink-0">
            <PropertiesPanel preview={preview.props} photoProblem={photo.problem} onRetryPhoto={photo.reload} onChoosePhoto={() => void choosePhoto()} />
          </div>
        )}
      </div>
    </div>
  );
}
