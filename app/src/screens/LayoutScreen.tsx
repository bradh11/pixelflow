import { useRef } from "react";
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
import { PageHeader } from "../components/ui";
import { boxOfPoints, defaultBackground, unionBox } from "../lib/layoutMath";
import { useApp } from "../state/store";
import { showViewKey, useView3d } from "../state/view3d";

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
      <div className="flex h-[max(26rem,calc(100vh-16.5rem))] gap-3">
        <SidePanel />
        <div className="relative min-w-0 flex-1">
          {in3d ? (
            <Layout3dView ref={canvas} preview={preview3d} show={show} photo={photo} storageKey={showViewKey(snapshot.path, show.name)} editable />
          ) : (
            <LayoutCanvas ref={canvas} preview={preview} show={show} photo={photo} />
          )}
          {show.props.length === 0 && (
            <div className="pointer-events-none absolute inset-0 flex items-center justify-center p-6 text-center text-sm text-neutral-300">
              <p className="max-w-sm rounded-lg bg-black/60 px-4 py-3">
                {in3d
                  ? "Nothing here yet. Use Add prop above, or switch to 2D (V) to draw props."
                  : show.background
                    ? "Use Add prop, or pick a tool above, like Arch or Matrix, and drag on the photo where that prop is."
                    : "Use Add prop, or pick a tool above, like Arch or Matrix, and drag here to draw a prop. Add a photo of your house to draw right over it."}
              </p>
            </div>
          )}
        </div>
        <div className="w-72 shrink-0">
          <PropertiesPanel preview={preview.props} photoProblem={photo.problem} onRetryPhoto={photo.reload} onChoosePhoto={() => void choosePhoto()} />
        </div>
      </div>
    </div>
  );
}
