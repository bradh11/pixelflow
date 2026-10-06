import { ChevronDown, ChevronRight, Trash2 } from "lucide-react";
import { memo, useRef, useState } from "react";
import { errorMessage } from "../api/backend";
import type { Prop } from "../api/types";
import { LayoutCanvas, type LayoutCanvasHandle } from "../components/layout/LayoutCanvas";
import { LayoutToolbar } from "../components/layout/LayoutToolbar";
import { PropertiesPanel } from "../components/layout/PropertiesPanel";
import { FALLBACK_ASPECT, imageAspect, useBackgroundImage, usePreviewProps, usePreviewProps3d } from "../components/layout/useLayoutData";
import { useLayoutKeys } from "../components/layout/useLayoutKeys";
import { Layout3dView } from "../components/layout3d/Layout3dView";
import { useLayout3dKeys } from "../components/layout3d/useLayout3dKeys";
import { Button, EmptyState, Input, PageHeader } from "../components/ui";
import { thousands } from "../lib/format";
import { updateEdits } from "../lib/layoutEdits";
import { AddPropMenu } from "../components/layout/AddPropMenu";
import { boxOfPoints, defaultBackground, unionBox } from "../lib/layoutMath";
import { shapeLabel } from "../lib/shows";
import { useLayoutEditor } from "../state/layoutEditor";
import { useApp } from "../state/store";
import { showViewKey, useView3d } from "../state/view3d";

function PropRow({ prop, pixels, selected }: { prop: Prop; pixels: number; selected: boolean }) {
  const apply = useApp((s) => s.apply);
  const toggle = useLayoutEditor((s) => s.toggle);
  const [name, setName] = useState(prop.name);
  const commit = () => {
    const trimmed = name.trim();
    if (trimmed && trimmed !== prop.name) void apply(updateEdits(prop.id, (p) => ({ ...p, name: trimmed })));
    else setName(prop.name);
  };
  return (
    <tr className={`border-t border-neutral-200 dark:border-neutral-800 ${selected ? "bg-accent-50 dark:bg-accent-600/15" : ""}`}>
      <td className="w-8 py-1.5 pl-1">
        <input type="checkbox" aria-label={`Select ${prop.name}`} checked={selected} onChange={() => toggle(prop.id)} className="accent-accent-500" />
      </td>
      <td className="py-1.5 pr-3">
        <Input
          aria-label={`Name of ${prop.name}`}
          value={name}
          onChange={(e) => setName(e.target.value)}
          onBlur={commit}
          onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
          className="w-full border-transparent bg-transparent hover:border-neutral-300 focus:border-neutral-400 dark:border-transparent dark:bg-transparent dark:hover:border-neutral-700 dark:focus:border-neutral-600"
        />
      </td>
      <td className="px-3 text-sm text-neutral-500">{shapeLabel(prop.shape)}</td>
      <td className="px-3 text-right text-sm tabular-nums">{thousands(pixels)}</td>
      <td className="px-3 text-sm text-neutral-500">{prop.colorOrder}</td>
      <td className="pl-3 text-right">
        <Button variant="danger" aria-label={`Delete ${prop.name}`} onClick={() => apply([{ type: "removeProp", id: prop.id }])}>
          <Trash2 size={16} />
        </Button>
      </td>
    </tr>
  );
}

/** Every prop as a table: names, pixel counts, and a checkbox to select it (for keyboard users too). */
const PropsList = memo(function PropsList() {
  const snapshot = useApp((s) => s.snapshot!);
  const selected = useLayoutEditor((s) => s.selected);
  const [open, setOpen] = useState(true);
  const pixels = new Map(snapshot.channelMap.props.map((p) => [p.prop, p.nodes]));
  const props = snapshot.show.props;
  return (
    <section className="mt-4">
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
        className="mb-2 inline-flex items-center gap-1 text-sm font-medium text-neutral-700 dark:text-neutral-300"
      >
        {open ? <ChevronDown size={16} aria-hidden /> : <ChevronRight size={16} aria-hidden />}
        Props list ({props.length})
      </button>
      {open &&
        (props.length === 0 ? (
          <EmptyState title="No props yet">Pick a drawing tool and drag on the canvas, or choose a prop type and Add prop.</EmptyState>
        ) : (
          <table className="w-full">
            <thead>
              <tr className="text-left text-xs tracking-wide text-neutral-500 uppercase">
                <th className="pb-2 font-medium">
                  <span className="sr-only">Selected</span>
                </th>
                <th className="pb-2 font-medium">Name</th>
                <th className="px-3 pb-2 font-medium">Type</th>
                <th className="px-3 pb-2 text-right font-medium">Pixels</th>
                <th className="px-3 pb-2 font-medium">Color order</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {props.map((prop) => (
                <PropRow key={`${prop.id}:${prop.name}`} prop={prop} pixels={pixels.get(prop.id) ?? 0} selected={selected.includes(prop.id)} />
              ))}
            </tbody>
          </table>
        ))}
    </section>
  );
});

/** Draw the display: props over a photo of the house, with a properties panel and a props list. */
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
          <>
            <AddPropMenu preview={preview.props} />
          </>
        }
      />
      <LayoutToolbar hasPhoto={!!show.background} onChoosePhoto={() => void choosePhoto()} />
      <div className="flex h-[max(26rem,calc(100vh-17rem))] gap-3">
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
                  ? "No props yet. Use Add prop above, or switch to 2D (V) to draw them."
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
      <PropsList />
    </div>
  );
}
