import { Box } from "lucide-react";
import { useEffect, useState } from "react";
import { errorMessage } from "../../api/backend";
import type { FileRole, HouseModel, PreviewProp, Show } from "../../api/types";
import { fileName, shownPath } from "../../lib/format";
import { type Box3, fitModelPlacement, v3 } from "../../lib/layout3d";
import { boxOfPoints, unionBox } from "../../lib/layoutMath";
import { useApp } from "../../state/store";
import { useView3d } from "../../state/view3d";
import { Button } from "../ui";
import { NumberField, Section } from "../layout/PropertiesPanel";
import { measureModel } from "./scene";
import { MissingFileNotice, useMissingFile } from "../MissingFiles";

const HOUSE_MODEL: FileRole = { kind: "houseModel" };

/** What a new house model is sized to: the photo's width, or else the props'. */
function fitTarget(show: Show, preview: PreviewProp[]): Box3 | null {
  const bg = show.background;
  if (bg) return { min: v3(bg.x, 0, 0), max: v3(bg.x + bg.width, 0, 0) };
  const box = unionBox(preview.map((p) => boxOfPoints(p.points)));
  return box ? { min: v3(box.minX, box.minY, 0), max: v3(box.maxX, box.maxY, 0) } : null;
}

/** The model at `path`, turned as `keep` says, sized and placed to stand behind the display (one undo step). */
async function placedModel(path: string, preview: PreviewProp[], keep: Partial<HouseModel> = {}): Promise<HouseModel> {
  const { backend, snapshot } = useApp.getState();
  const rotationDeg = keep.rotationDeg ?? v3(0, 0, 0);
  // The model the 3D view already shows is measured already; another one is read and measured here.
  const loaded = useView3d.getState().loadedModel;
  const natural = loaded?.path === path ? loaded.natural : backend ? await measureModel(await backend.readHouseModel(path), path) : null;
  const fit = natural ? fitModelPlacement(natural, fitTarget(snapshot!.show, preview), rotationDeg) : { position: v3(0, 0, 0), scale: 1 };
  return { path, opacity: 1, ...keep, rotationDeg, position: fit.position, scale: fit.scale };
}

/** The house model in the 3D view: add one, place it, or take it away. */
export function HouseModelPanel({ preview }: { preview: PreviewProp[] }) {
  const apply = useApp((s) => s.apply);
  const backend = useApp((s) => s.backend);
  const model = useApp((s) => s.snapshot?.show.houseModel ?? null);
  const missing = useMissingFile(HOUSE_MODEL);
  const [busy, setBusy] = useState(false);
  const [strength, setStrength] = useState<number | null>(null);
  useEffect(() => setStrength(null), [model?.opacity]);

  const set = (houseModel: HouseModel | null) => apply([{ type: "setHouseModel", houseModel }]);
  /** Changes the model as it is when the edit is sent. */
  const update = (change: (m: HouseModel) => HouseModel) =>
    void apply((show) => (show.houseModel ? [{ type: "setHouseModel", houseModel: change(show.houseModel) }] : []));

  const run = async (work: () => Promise<unknown>) => {
    setBusy(true);
    try {
      await work();
    } catch (e) {
      useApp.setState({ error: errorMessage(e) });
    } finally {
      setBusy(false);
    }
  };
  const choose = () =>
    run(async () => {
      const path = await backend?.pickHouseModelPath();
      if (path) await set(await placedModel(path, preview, model ? { rotationDeg: model.rotationDeg, opacity: model.opacity } : {}));
    });
  const fit = () => run(async () => model && (await set(await placedModel(model.path, preview, { rotationDeg: model.rotationDeg, opacity: model.opacity }))));

  if (!model) {
    return (
      <Section title="House model">
        <p className="mb-2 text-sm text-neutral-500">Add a 3D model of your house (GLB, glTF, or OBJ) to place props on it in depth.</p>
        <Button onClick={() => void choose()} disabled={busy}>
          <Box size={16} aria-hidden /> {busy ? "Loading model…" : "Add house model…"}
        </Button>
      </Section>
    );
  }
  const shownStrength = Math.round((strength ?? model.opacity) * 100);
  const commitStrength = () => {
    if (strength === null || strength === model.opacity) return;
    const opacity = strength;
    update((m) => ({ ...m, opacity }));
  };
  const v = (key: "position" | "rotationDeg", axis: "x" | "y" | "z") => (n: number) => update((m) => ({ ...m, [key]: { ...m[key], [axis]: n } }));
  return (
    <Section title="House model">
      <p className="mb-2 truncate text-sm" title={shownPath(model.path)}>
        {fileName(model.path)}
      </p>
      {missing && (
        <div className="mb-2">
          <MissingFileNotice missing={missing} />
        </div>
      )}
      <div className="grid grid-cols-3 gap-2">
        <NumberField label="Model X" value={model.position.x} onCommit={v("position", "x")} />
        <NumberField label="Model Y" value={model.position.y} onCommit={v("position", "y")} />
        <NumberField label="Model Z" value={model.position.z} onCommit={v("position", "z")} />
        <NumberField label="Model tilt (X°)" hint="Stand up a model made lying down: try -90" value={model.rotationDeg.x} onCommit={v("rotationDeg", "x")} />
        <NumberField label="Model turn (Y°)" value={model.rotationDeg.y} onCommit={v("rotationDeg", "y")} />
        <NumberField label="Model scale" min={0.0001} nonZero value={model.scale} onCommit={(scale) => update((m) => ({ ...m, scale }))} />
      </div>
      <label className="mt-2 flex flex-col gap-1 text-xs">
        <span className="text-neutral-500 dark:text-neutral-400">Model strength: {shownStrength}%</span>
        <input
          type="range"
          min={0}
          max={100}
          value={shownStrength}
          aria-label="Model strength"
          onChange={(e) => setStrength(Number(e.target.value) / 100)}
          onPointerUp={commitStrength}
          onKeyUp={commitStrength}
          onBlur={commitStrength}
          className="accent-accent-500"
        />
      </label>
      <div className="mt-3 flex flex-wrap gap-2">
        <Button title="Size the model to the photo (or the props) and stand it behind them" onClick={() => void fit()} disabled={busy}>
          Fit to display
        </Button>
        <Button onClick={() => void choose()} disabled={busy}>
          Replace…
        </Button>
        <Button variant="danger" onClick={() => void set(null)} disabled={busy}>
          Remove model
        </Button>
      </div>
    </Section>
  );
}
