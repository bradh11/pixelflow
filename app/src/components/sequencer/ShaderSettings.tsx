import { FilePlus2, TriangleAlert } from "lucide-react";
import { useEffect, useState } from "react";
import { errorMessage } from "../../api/backend";
import type { ShaderInputs, ShaderValue } from "../../api/sequence";
import type { ShaderInfo, ShaderInputInfo } from "../../api/sequencer";
import { useSequencer } from "../../state/sequencer";
import { Button } from "../ui";
import { ColorPicker, type CurveControl, FIELD, NumberSetting } from "./effectControls";

/**
 * What the shader `file` is and takes, asked of the engine (which reads and compiles the file);
 * null until it answers. Asked again when a file a preview was waiting for arrives.
 */
function useShaderInfo(file: string): ShaderInfo | null {
  const api = useSequencer((s) => s.api);
  const arrived = useSequencer((s) => s.picturesArrived);
  const [known, setKnown] = useState<ShaderInfo | null>(null);
  useEffect(() => {
    if (!api) return;
    let live = true;
    void api.shaderInfo(file).then(
      (info) => live && setKnown(info),
      () => {},
    );
    return () => {
      live = false;
    };
  }, [api, file, arrived]);
  return known?.file === file ? known : null;
}

/** A step a slider moves by: about a two-hundredth of its range, as a round number. */
function stepFor(min: number, max: number): number {
  const rough = (max - min) / 200;
  if (!(rough > 0)) return 0.01;
  const power = 10 ** Math.floor(Math.log10(rough));
  return Number((rough / power >= 5 ? 5 * power : rough / power >= 2 ? 2 * power : power).toPrecision(1));
}

const toHex = (c: number[]) =>
  `#${c
    .slice(0, 3)
    .map((v) => Math.round(Math.min(1, Math.max(0, v)) * 255).toString(16).padStart(2, "0"))
    .join("")}`;

function fromHex(hex: string, alpha: number): [number, number, number, number] {
  const n = parseInt(hex.replace("#", ""), 16);
  const part = (shift: number) => Math.round((((Number.isFinite(n) ? n : 0xffffff) >> shift) & 255) / 0.255) / 1000;
  return [part(16), part(8), part(0), alpha];
}

/**
 * The shader a Shader effect draws: a list of PixelFlow's own and the show's shader files to
 * choose from, a button that adds one from the disk (the desktop app copies it into the show's
 * shaders folder), what the shader says of itself, and, when it can't be drawn, why.
 */
export function ShaderFileSetting({
  effectId,
  label,
  description,
  file,
  onChoose,
}: {
  effectId: string;
  label: string;
  description: string;
  file: string;
  /** Another shader was chosen: `file` is what the effect's setting holds for it. */
  onChoose: (file: string) => Promise<boolean>;
}) {
  const api = useSequencer((s) => s.api);
  const playheadMs = useSequencer((s) => s.playheadMs);
  const revision = useSequencer((s) => s.revision);
  const info = useShaderInfo(file);
  const [listed, setListed] = useState<ShaderInfo[]>([]);
  const [stopped, setStopped] = useState<string | null>(null);
  const [failed, setFailed] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    void api?.listShaders().then(
      (list) => live && setListed(list),
      () => {},
    );
    return () => {
      live = false;
    };
  }, [api, file]);
  // Whether the frame showing was stopped is known once it has been drawn: asked a moment after
  // the effect, the time, or the sequence changes.
  useEffect(() => {
    if (!api) return;
    let live = true;
    const ask = () =>
      void api.shaderStopped(effectId).then(
        (why) => live && setStopped(why),
        () => {},
      );
    ask();
    const later = setTimeout(ask, 400);
    return () => {
      live = false;
      clearTimeout(later);
    };
  }, [api, effectId, file, playheadMs, revision]);
  const add = async () => {
    if (!api) return;
    setFailed(null);
    try {
      const picked = await api.pickShader();
      if (picked) await onChoose(picked.file);
    } catch (e) {
      setFailed(errorMessage(e));
    }
  };
  const own = listed.filter((s) => s.builtIn);
  const shows = listed.filter((s) => !s.builtIn);
  const known = file === "" || listed.some((s) => s.file === file);
  const problem = info?.problem ?? null;
  return (
    <div className="flex flex-col gap-1.5 text-sm" title={description}>
      <label htmlFor="setting-shader-file" className="text-neutral-600 dark:text-neutral-400">
        {label}
      </label>
      <div className="flex items-center gap-1.5">
        <select id="setting-shader-file" className={`${FIELD} min-w-0 flex-1`} value={file} onChange={(e) => void onChoose(e.target.value)}>
          {file === "" && <option value="">Choose a shader…</option>}
          <optgroup label="PixelFlow's own">
            {own.map((s) => (
              <option key={s.file} value={s.file}>
                {s.name}
              </option>
            ))}
          </optgroup>
          {shows.length > 0 && (
            <optgroup label="This show's shader files">
              {shows.map((s) => (
                <option key={s.file} value={s.file}>
                  {s.name}
                  {s.problem ? " (can't be drawn)" : ""}
                </option>
              ))}
            </optgroup>
          )}
          {!known && <option value={file}>{info?.name ?? file}</option>}
        </select>
        <Button className="!px-2 !py-1" title="Add a shader file from your computer (.fs)" aria-label="Add a shader file" onClick={() => void add()}>
          <FilePlus2 size={14} aria-hidden />
        </Button>
      </div>
      {info && !problem && info.description && (
        <p className="text-xs text-neutral-500">
          {info.description}
          {info.credit ? <span className="text-neutral-400"> By {info.credit}.</span> : null}
        </p>
      )}
      {problem && (
        <p role="alert" className="flex items-start gap-1.5 rounded border border-amber-300 bg-amber-50 px-2 py-1.5 text-xs text-amber-900 dark:border-amber-800 dark:bg-amber-950/50 dark:text-amber-200">
          <TriangleAlert size={13} className="mt-0.5 shrink-0" aria-hidden />
          <span>
            <strong className="font-semibold">{info?.name ?? "This shader"} can't be drawn, so the effect shows nothing.</strong>{" "}
            {info?.line ? <span className="font-mono">{problem}</span> : sentence(problem)}
          </span>
        </p>
      )}
      {!problem && stopped && (
        <p role="alert" className="flex items-start gap-1.5 rounded border border-amber-300 bg-amber-50 px-2 py-1.5 text-xs text-amber-900 dark:border-amber-800 dark:bg-amber-950/50 dark:text-amber-200">
          <TriangleAlert size={13} className="mt-0.5 shrink-0" aria-hidden />
          <span>{stopped}</span>
        </p>
      )}
      {failed && <span className="text-xs text-amber-700 dark:text-amber-400">{failed}</span>}
    </div>
  );
}

/** `text` as a sentence: a capital to start and a full stop to end. */
function sentence(text: string): string {
  const start = text.charAt(0).toUpperCase() + text.slice(1);
  return /[.!?]$/.test(start) ? start : `${start}.`;
}

/**
 * The inputs the effect's shader declares, each with the control its kind calls for: a slider
 * with the file's own range (which can change over the effect, like any number setting), a
 * checkbox, a list, two sliders for a point, a color. An input the effect hasn't set shows the
 * file's own value, and setting it back there takes it out of the effect again.
 */
export function ShaderInputsSetting({
  effectId,
  file,
  inputs,
  onChange,
  animate,
}: {
  effectId: string;
  file: string;
  inputs: ShaderInputs;
  /** One input set (`value` undefined: back to the file's own), with the gesture a slider's pull is. */
  onChange: (name: string, value: ShaderValue | undefined, gesture?: string) => Promise<boolean>;
  /** The curve on the setting `key` (`inputs.<name>`, or `.x` and `.y` of a point). */
  animate: (key: string) => CurveControl;
}) {
  const info = useShaderInfo(file);
  if (!info || info.problem || info.inputs.length === 0) return null;
  return (
    <div className="flex flex-col gap-2.5" aria-label={`${info.name}'s inputs`} role="group">
      {info.inputs.map((input) => (
        <ShaderInput
          key={`${effectId}:${file}:${input.name}`}
          input={input}
          value={inputs[input.name]}
          onChange={(value, gesture) => onChange(input.name, value, gesture)}
          animate={animate}
        />
      ))}
    </div>
  );
}

function same(a: ShaderValue, b: ShaderValue): boolean {
  return Array.isArray(a) && Array.isArray(b) ? a.length === b.length && a.every((v, k) => v === b[k]) : a === b;
}

function ShaderInput({
  input,
  value,
  onChange,
  animate,
}: {
  input: ShaderInputInfo;
  value: ShaderValue | undefined;
  onChange: (value: ShaderValue | undefined, gesture?: string) => Promise<boolean>;
  animate: (key: string) => CurveControl;
}) {
  // Back at the file's own value, the effect no longer sets it.
  const set = (next: ShaderValue, gesture?: string) => onChange(same(next, input.default) ? undefined : next, gesture);
  const key = `inputs.${input.name}`;
  if (input.type === "number") {
    const setting = { key, label: input.label, description: "", type: "number" as const, min: input.min, max: input.max, step: stepFor(input.min, input.max), default: input.default };
    return <NumberSetting setting={setting} value={typeof value === "number" ? value : input.default} onChange={(v, gesture) => set(v as number, gesture)} animate={animate(key)} />;
  }
  if (input.type === "bool") {
    return (
      <label className="flex items-center gap-2 text-sm">
        <input type="checkbox" className="accent-violet-600" checked={typeof value === "boolean" ? value : input.default} onChange={(e) => void set(e.target.checked)} />
        {input.label}
      </label>
    );
  }
  if (input.type === "choice") {
    const id = `setting-${key}`;
    return (
      <label htmlFor={id} className="flex flex-col gap-1 text-sm">
        <span className="text-neutral-600 dark:text-neutral-400">{input.label}</span>
        <select id={id} className={FIELD} value={typeof value === "number" ? value : input.default} onChange={(e) => void set(Number(e.target.value))}>
          {input.options.map((o) => (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ))}
        </select>
      </label>
    );
  }
  if (input.type === "point") {
    const at = Array.isArray(value) && value.length === 2 ? value : input.default;
    // A coordinate a curve hasn't set is the file's own.
    const now = (k: 0 | 1) => (Number.isFinite(at[k]) ? at[k] : input.default[k]);
    const axis = (k: 0 | 1, name: string) => {
      const part = `${key}.${k === 0 ? "x" : "y"}`;
      const setting = { key: part, label: `${input.label}: ${name}`, description: "", type: "number" as const, min: input.min[k], max: input.max[k], step: stepFor(input.min[k], input.max[k]), default: input.default[k] };
      return (
        <NumberSetting
          setting={setting}
          value={now(k)}
          onChange={(v, gesture) => set(k === 0 ? [v as number, now(1)] : [now(0), v as number], gesture)}
          animate={animate(part)}
        />
      );
    };
    return (
      <div className="flex flex-col gap-2">
        {axis(0, "across")}
        {axis(1, "up")}
      </div>
    );
  }
  const color = Array.isArray(value) && value.length === 4 ? value : input.default;
  return (
    <div className="flex items-center justify-between gap-2 text-sm">
      <span className="text-neutral-600 dark:text-neutral-400">{input.label}</span>
      <ColorPicker index={0} label={input.label} color={toHex(color)} onPick={(hex, gesture) => set(fromHex(hex, color[3]), gesture)} />
    </div>
  );
}
