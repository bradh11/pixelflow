import type { PreviewProp } from "../../api/types";
import { useApp } from "../../state/store";
import { showViewKey, useView3d } from "../../state/view3d";
import { PreviewCanvas } from "../PreviewCanvas";
import { useBackgroundImage, usePreviewProps3d } from "../layout/useLayoutData";
import { Layout3dView } from "./Layout3dView";
import { ModeSwitch } from "./ModeSwitch";

/**
 * The props in their live colors, flat (front view) or in 3D, with a switch between the two
 * (remembered). The 3D view is look-only here and shares the Layout screen's camera for the show.
 */
export function LivePreview({ props, frame }: { props: PreviewProp[]; frame: Uint8Array | null }) {
  const snapshot = useApp((s) => s.snapshot);
  const mode = useView3d((s) => s.playMode);
  const setMode = useView3d((s) => s.setPlayMode);
  const in3d = mode === "3d";
  const preview3d = usePreviewProps3d(in3d);
  const photo = useBackgroundImage(in3d ? snapshot?.show.background?.path : null);
  if (!snapshot) return null;
  return (
    <div className="flex h-full min-h-64 flex-col gap-2">
      <div className="flex justify-end">
        <ModeSwitch mode={mode} onChange={setMode} />
      </div>
      <div className="relative min-h-64 flex-1">
        {in3d ? (
          <Layout3dView preview={preview3d} show={snapshot.show} photo={photo} storageKey={showViewKey(snapshot.path, snapshot.show.name)} frame={frame} />
        ) : (
          <PreviewCanvas props={props} frame={frame} />
        )}
      </div>
    </div>
  );
}
