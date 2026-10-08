import {
  AlertTriangle,
  AppWindow,
  Box,
  CandyCane,
  Check,
  ChevronDown,
  ChevronRight,
  ChevronsDown,
  Circle,
  CircleDot,
  Disc3,
  Download,
  FolderOpen,
  Globe,
  Grid3x3,
  Layers,
  Lightbulb,
  Minus,
  Music,
  Plus,
  Rainbow,
  Search,
  Shapes,
  Snowflake,
  Star,
  TreePine,
  Upload,
  WandSparkles,
  X,
  Eraser,
  type LucideIcon,
} from "lucide-react";
import { useEffect, useId, useMemo, useRef, useState } from "react";
import { errorMessage } from "../api/backend";
import type { VendorInspection, VendorItem, VendorMapping, VendorPropType, VendorSuggestion, VendorTarget } from "../api/types";
import { fileName, plural, shownPath, thousands } from "../lib/format";
import { autoMapping, itemTree, mappingStats, targetsOf, withLoaded } from "../lib/vendorMapping";
import { useSequencer } from "../state/sequencer";
import { useApp } from "../state/store";
import { toast } from "../state/toast";
import { Button } from "./ui";

const TYPE_ICONS: Record<VendorPropType, LucideIcon> = {
  tree: TreePine,
  arch: Rainbow,
  matrix: Grid3x3,
  canes: CandyCane,
  line: Minus,
  window: AppWindow,
  star: Star,
  circle: Circle,
  wreath: CircleDot,
  spinner: Disc3,
  sphere: Globe,
  cube: Box,
  icicles: ChevronsDown,
  snowflake: Snowflake,
  flood: Lightbulb,
  other: Shapes,
};

const TYPE_NAMES: Record<VendorPropType, string> = {
  tree: "tree",
  arch: "arch",
  matrix: "matrix",
  canes: "candy canes",
  line: "line",
  window: "window frame",
  star: "star",
  circle: "circle",
  wreath: "wreath",
  spinner: "spinner",
  sphere: "sphere",
  cube: "cube",
  icicles: "icicles",
  snowflake: "snowflake",
  flood: "flood",
  other: "prop",
};

function TypeIcon({ type, group }: { type: VendorPropType; group: boolean }) {
  const Icon = group ? Layers : TYPE_ICONS[type];
  return <Icon size={14} aria-hidden className="shrink-0 text-neutral-500 dark:text-neutral-400" />;
}

/** "tree · 800 lights", "group of arches", "submodel". */
function describe(kind: VendorItem["kind"], type: VendorPropType, pixels: number): string {
  const what = kind === "group" ? (type === "other" ? "group" : `group · ${TYPE_NAMES[type]}`) : kind === "model" ? TYPE_NAMES[type] : kind;
  return pixels > 0 ? `${what} · ${thousands(pixels)} lights` : what;
}

/** Picks the props, groups, and submodels an item's effects go to: a search box over the show's. */
function TargetPicker({
  item,
  targets,
  chosen,
  hint,
  onToggle,
  onDone,
}: {
  item: VendorItem;
  targets: VendorTarget[];
  chosen: string[];
  hint: VendorSuggestion | undefined;
  onToggle: (target: string) => void;
  onDone: () => void;
}) {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const listId = useId();
  useEffect(() => {
    input.current?.focus();
    input.current?.parentElement?.parentElement?.scrollIntoView?.({ block: "nearest" });
  }, []);
  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    const order = (t: VendorTarget) => (hint?.targets.includes(t.name) ? 0 : t.type === item.type && item.type !== "other" ? 1 : 2);
    return targets
      .filter((t) => !q || t.name.toLowerCase().includes(q))
      .sort((a, b) => order(a) - order(b) || ["group", "model", "submodel"].indexOf(a.kind) - ["group", "model", "submodel"].indexOf(b.kind));
  }, [targets, query, hint, item.type]);
  const pick = (t: VendorTarget | undefined) => t && onToggle(t.name);
  return (
    <div className="mt-2 rounded-md border border-neutral-200 bg-neutral-50 p-2 dark:border-neutral-700 dark:bg-neutral-950/40">
      <div className="flex items-center gap-2 rounded-md border border-neutral-300 bg-white px-2 dark:border-neutral-700 dark:bg-neutral-900">
        <Search size={14} aria-hidden className="text-neutral-400" />
        <input
          ref={input}
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setActive(0);
          }}
          onKeyDown={(e) => {
            if (e.key === "ArrowDown") setActive((a) => Math.min(shown.length - 1, a + 1));
            else if (e.key === "ArrowUp") setActive((a) => Math.max(0, a - 1));
            else if (e.key === "Enter") pick(shown[active]);
            else if (e.key === "Escape") onDone();
            else return;
            e.preventDefault();
            e.stopPropagation();
          }}
          aria-label={`Find a prop or group for ${item.label}`}
          aria-controls={listId}
          placeholder="Find a prop or group"
          className="min-w-0 flex-1 bg-transparent py-1 text-sm outline-none"
        />
        <Button variant="ghost" className="px-2 py-0.5 text-xs" onClick={onDone}>
          Done
        </Button>
      </div>
      <ul id={listId} role="listbox" aria-multiselectable="true" aria-label={`Props for ${item.label}`} className="mt-1 max-h-56 overflow-auto">
        {shown.length === 0 && <li className="px-2 py-1.5 text-neutral-500">Nothing in your show is called that.</li>}
        {shown.map((t, i) => {
          const selected = chosen.includes(t.name);
          return (
            <li
              key={t.name}
              role="option"
              aria-selected={selected}
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => onToggle(t.name)}
              onMouseEnter={() => setActive(i)}
              className={`flex cursor-pointer items-center gap-2 rounded px-2 py-1 ${i === active ? "bg-accent-600/10" : ""}`}
            >
              <span className="w-3.5 shrink-0">{selected && <Check size={14} aria-hidden className="text-accent-600" />}</span>
              <TypeIcon type={t.type} group={t.kind === "group"} />
              <span className="min-w-0 flex-1 break-words">
                {t.parent && <span className="text-neutral-500">{t.parent} / </span>}
                {t.label}
                {hint?.targets.includes(t.name) && <span className="ml-2 text-xs text-accent-600">suggested</span>}
              </span>
              <span className="shrink-0 text-xs text-neutral-500">{describe(t.kind, t.type, t.pixels)}</span>
            </li>
          );
        })}
      </ul>
    </div>
  );
}

/** One vendor item and where its effects go. */
function ItemRow({
  item,
  depth,
  mapping,
  targets,
  hint,
  busy,
  expandable,
  expanded,
  picking,
  onExpand,
  onPick,
  onToggle,
  onSet,
}: {
  item: VendorItem;
  depth: number;
  mapping: VendorMapping;
  targets: VendorTarget[];
  hint: VendorSuggestion | undefined;
  busy: boolean;
  expandable: number;
  expanded: boolean;
  picking: boolean;
  onExpand: () => void;
  onPick: (open: boolean) => void;
  onToggle: (target: string) => void;
  onSet: (targets: string[]) => void;
}) {
  const chosen = targetsOf(mapping, item.name);
  const mapped = chosen.length > 0;
  const loud = !mapped && busy;
  const offer = !mapped && hint && hint.targets.length > 0 ? hint : undefined;
  return (
    <li
      aria-label={item.label}
      className={`border-b border-neutral-100 px-3 py-2 dark:border-neutral-800 ${loud ? "border-l-4 border-l-amber-500 bg-amber-50/70 dark:bg-amber-950/20" : "border-l-4 border-l-transparent"}`}
    >
      <div className="grid grid-cols-1 gap-2 md:grid-cols-[minmax(0,1fr)_minmax(0,1.1fr)] md:gap-4">
        <div className="flex min-w-0 items-start gap-1.5" style={{ paddingLeft: depth * 20 }}>
          {expandable > 0 ? (
            <button
              type="button"
              onClick={onExpand}
              aria-expanded={expanded}
              aria-label={`${expanded ? "Hide" : "Show"} the parts of ${item.label}`}
              className="mt-0.5 rounded text-neutral-500 hover:text-neutral-900 dark:hover:text-neutral-100"
            >
              {expanded ? <ChevronDown size={14} aria-hidden /> : <ChevronRight size={14} aria-hidden />}
            </button>
          ) : (
            <span className="w-3.5 shrink-0" />
          )}
          <span className="mt-0.5">
            <TypeIcon type={item.type} group={item.kind === "group"} />
          </span>
          <div className="min-w-0 flex-1">
            <div className="break-words font-medium">{item.label}</div>
            <div className="text-xs text-neutral-500">
              {describe(item.kind, item.type, item.pixels)}
              {expandable > 0 && !expanded && ` · ${plural(expandable, "part")} with effects`}
            </div>
          </div>
          <span
            className={`shrink-0 rounded-full px-2 py-0.5 text-xs tabular-nums ${loud ? "bg-amber-200 font-semibold text-amber-900 dark:bg-amber-900/60 dark:text-amber-200" : "bg-neutral-100 text-neutral-600 dark:bg-neutral-800 dark:text-neutral-300"}`}
          >
            {plural(item.effects, "effect")}
          </span>
        </div>
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-1.5">
            {chosen.map((t) => {
              const target = targets.find((x) => x.name === t);
              return (
                <span key={t} className="inline-flex max-w-full items-center gap-1 rounded-md border border-neutral-200 bg-white px-1.5 py-0.5 dark:border-neutral-700 dark:bg-neutral-900">
                  {target && <TypeIcon type={target.type} group={target.kind === "group"} />}
                  <span className="break-words">{t}</span>
                  <button type="button" aria-label={`Remove ${t} from ${item.label}`} onClick={() => onToggle(t)} className="rounded text-neutral-400 hover:text-neutral-900 dark:hover:text-neutral-100">
                    <X size={12} aria-hidden />
                  </button>
                </span>
              );
            })}
            {!mapped && (
              <span className={`inline-flex items-center gap-1 text-xs ${loud ? "font-medium text-amber-800 dark:text-amber-300" : "text-neutral-500"}`}>
                {loud && <AlertTriangle size={12} aria-hidden />} Not mapped
              </span>
            )}
            {offer && (
              <Button variant="ghost" className="px-1.5 py-0.5 text-xs text-accent-700 dark:text-accent-400" onClick={() => onSet([...offer.targets])}>
                Use {offer.targets.join(", ")}?
              </Button>
            )}
            <Button variant="ghost" className="px-1.5 py-0.5 text-xs" aria-label={`Map ${item.label}`} aria-expanded={picking} onClick={() => onPick(!picking)}>
              <Plus size={12} aria-hidden /> {mapped ? "Add" : "Choose"}
            </Button>
          </div>
          {picking && <TargetPicker item={item} targets={targets} chosen={chosen} hint={offer ?? hint} onToggle={onToggle} onDone={() => onPick(false)} />}
        </div>
      </div>
    </li>
  );
}

/**
 * Maps a vendor's sequence, made for their props, onto the show's, as xLights' Import Effects
 * does: each of their models, groups, submodels, and strands with effects goes to one or more of
 * the show's props or groups, or is skipped. Opens with the suggested mapping; Import brings the
 * sequence in with the mapping chosen and remembers it for the vendor's next song.
 */
export function VendorImportDialog() {
  const pending = useApp((s) => s.vendorImport);
  if (!pending) return null;
  // A new package or sequence starts the dialog afresh.
  return <MappingDialog key={`${pending.path}\n${pending.inspection.sequence}`} path={pending.path} inspection={pending.inspection} />;
}

function MappingDialog({ path, inspection }: { path: string; inspection: VendorInspection }) {
  const backend = useApp((s) => s.backend);
  const busy = useApp((s) => s.busy);
  const [mapping, setMapping] = useState<VendorMapping>(inspection.mapping);
  const [filter, setFilter] = useState("");
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set());
  const [picking, setPicking] = useState<string | null>(null);
  const [musicFolder, setMusicFolder] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const dialog = useRef<HTMLDivElement>(null);
  const close = () => useApp.getState().closeVendorImport();

  useEffect(() => {
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    dialog.current?.querySelector<HTMLElement>("[data-autofocus]")?.focus();
    return () => {
      if (opener?.isConnected) opener.focus();
    };
  }, []);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || e.defaultPrevented) return;
      e.preventDefault();
      if (picking) setPicking(null);
      else close();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [picking]);

  const stats = mappingStats(inspection.items, mapping);
  const hints = useMemo(() => new Map(inspection.suggestions.map((s) => [s.item, s])), [inspection.suggestions]);
  const tree = useMemo(() => itemTree(inspection.items), [inspection.items]);
  // "Many" effects: enough that leaving them out is noticeable.
  const many = Math.max(5, stats.effects * 0.05);
  const shownTree = useMemo(() => {
    const q = filter.trim().toLowerCase();
    if (!q) return tree;
    return tree
      .map(({ item, children }) => ({ item, children: children.filter((c) => c.label.toLowerCase().includes(q)) }))
      .filter(({ item, children }) => item.label.toLowerCase().includes(q) || children.length > 0);
  }, [tree, filter]);

  const setTargets = (item: string, targets: string[]) =>
    setMapping((m) => {
      const items = { ...m.items };
      if (targets.length > 0) items[item] = targets;
      else delete items[item];
      return { items };
    });
  const toggle = (item: string, target: string) => {
    const now = targetsOf(mapping, item);
    setTargets(item, now.includes(target) ? now.filter((t) => t !== target) : [...now, target]);
  };

  const api = useSequencer.getState().api;
  const loadXmap = async () => {
    if (!api) return;
    setError(null);
    try {
      const picked = await api.pickXmapPath();
      if (!picked) return;
      const read = await api.readXmap(picked);
      const merged = withLoaded(inspection.items, mapping, read.mapping);
      setMapping(merged.mapping);
      const skipped = [
        merged.unused > 0 ? `${plural(merged.unused, "mapping")} for models this sequence doesn't have` : "",
        read.nodesSkipped > 0 ? `${plural(read.nodesSkipped, "single-node mapping")} (PixelFlow doesn't import those)` : "",
      ].filter(Boolean);
      setNotice(`Loaded ${plural(merged.used, "mapping")} from ${fileName(picked)}${skipped.length ? `; left out ${skipped.join(" and ")}` : ""}.`);
    } catch (e) {
      setError(errorMessage(e));
    }
  };
  const saveXmap = async () => {
    if (!api) return;
    setError(null);
    try {
      const picked = await api.pickXmapSavePath(`${inspection.song}.xmap`);
      if (!picked) return;
      await api.writeXmap(picked, mapping);
      toast(`Saved the mapping as ${fileName(picked)}`);
    } catch (e) {
      setError(errorMessage(e));
    }
  };
  const chooseMusicFolder = async () => {
    if (!backend) return;
    try {
      const picked = await backend.pickDownloadFolder();
      if (picked) setMusicFolder(picked);
    } catch (e) {
      setError(errorMessage(e));
    }
  };
  const importNow = async () => {
    setError(null);
    // Items left out are remembered as skipped, so the vendor's next song skips them too.
    const items: Record<string, string[]> = {};
    for (const i of inspection.items) items[i.name] = targetsOf(mapping, i.name);
    await useApp.getState().finishVendorImport({ sequence: inspection.sequence, mapping: { items }, key: inspection.key, musicFolder });
  };

  const musicTo = inspection.musicFolder ?? (musicFolder ? `${musicFolder}/music` : null);
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4">
      <div
        ref={dialog}
        role="dialog"
        aria-modal="true"
        aria-labelledby="vendor-import-title"
        className="flex h-[min(88vh,52rem)] w-[62rem] max-w-full flex-col rounded-lg border border-neutral-200 bg-white text-sm shadow-xl dark:border-neutral-800 dark:bg-neutral-900"
        onKeyDown={(e) => e.stopPropagation()}
      >
        <div className="border-b border-neutral-200 px-5 pb-3 pt-4 dark:border-neutral-800">
          <div className="flex flex-wrap items-start justify-between gap-x-4 gap-y-2">
            <div className="min-w-0">
              <h2 id="vendor-import-title" className="break-words text-lg font-semibold">
                Map {inspection.song} onto your props
              </h2>
              <p className="mt-0.5 break-words text-neutral-600 dark:text-neutral-400">
                {fileName(path)} was made for someone else&apos;s props. Choose where each of their models&apos; effects go.
                {!inspection.hasLayout && " (It has no layout of their props, so kinds are guessed from names.)"}
              </p>
            </div>
            {inspection.sequences.length > 1 && (
              <label className="flex items-center gap-2">
                <span className="text-neutral-600 dark:text-neutral-400">Sequence</span>
                <select
                  value={inspection.sequence}
                  onChange={(e) => void useApp.getState().switchVendorSequence(e.target.value)}
                  className="max-w-72 rounded-md border border-neutral-300 bg-white px-2 py-1 dark:border-neutral-700 dark:bg-neutral-900"
                >
                  {inspection.sequences.map((s) => (
                    <option key={s} value={s}>
                      {fileName(s)}
                    </option>
                  ))}
                </select>
              </label>
            )}
          </div>
          <div className="mt-3 flex flex-wrap items-center gap-x-3 gap-y-2">
            <div className="min-w-48 flex-1">
              <p aria-live="polite" className="font-medium">
                {`${stats.mapped} of ${stats.total} mapped (${stats.percent}% of effects)`}
              </p>
              <div className="mt-1 h-1.5 overflow-hidden rounded-full bg-neutral-200 dark:bg-neutral-800" aria-hidden>
                <div className="h-full rounded-full bg-accent-600 transition-all" style={{ width: `${stats.percent}%` }} />
              </div>
            </div>
            <div className="flex flex-wrap gap-1.5">
              <Button onClick={() => (setMapping(autoMapping(inspection)), setNotice(null))} data-tip="Map by name, alias, kind of prop, and size">
                <WandSparkles size={14} aria-hidden /> Auto-map
              </Button>
              <Button onClick={() => (setMapping({ items: {} }), setNotice(null))}>
                <Eraser size={14} aria-hidden /> Clear
              </Button>
              <Button onClick={() => void loadXmap()} data-tip="Use a mapping saved in xLights">
                <Upload size={14} aria-hidden /> Load .xmap…
              </Button>
              <Button onClick={() => void saveXmap()} data-tip="Save this mapping for xLights or another import">
                <Download size={14} aria-hidden /> Save .xmap…
              </Button>
            </div>
          </div>
          {notice && <p className="mt-2 text-neutral-600 dark:text-neutral-400">{notice}</p>}
        </div>

        <div className="grid grid-cols-1 items-center gap-2 border-b border-l-4 border-neutral-200 border-l-transparent px-3 py-2 md:grid-cols-[minmax(0,1fr)_minmax(0,1.1fr)] md:gap-4 dark:border-neutral-800">
          <div className="flex min-w-0 items-center gap-2">
            <Search size={14} aria-hidden className="shrink-0 text-neutral-400" />
            <input
              data-autofocus
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              aria-label="Find a vendor model"
              placeholder="Find one of their models (busiest first)"
              className="min-w-0 flex-1 bg-transparent py-1 outline-none"
            />
          </div>
          <span className="hidden text-xs font-medium text-neutral-500 md:block">Goes to, in your show</span>
        </div>

        <ul aria-label="Their models" className="min-h-0 flex-1 overflow-auto">
          {shownTree.length === 0 && <li className="px-5 py-6 text-neutral-500">None of their models is called that.</li>}
          {shownTree.map(({ item, children }) => {
            const open = expanded.has(item.name) || (filter.trim() !== "" && children.length > 0);
            const row = (i: VendorItem, depth: number, expandable: number) => (
              <ItemRow
                key={i.name}
                item={i}
                depth={depth}
                mapping={mapping}
                targets={inspection.targets}
                hint={hints.get(i.name)}
                busy={i.effects >= many}
                expandable={expandable}
                expanded={open}
                picking={picking === i.name}
                onExpand={() =>
                  setExpanded((s) => {
                    const next = new Set(s);
                    if (next.has(i.name)) next.delete(i.name);
                    else next.add(i.name);
                    return next;
                  })
                }
                onPick={(o) => setPicking(o ? i.name : null)}
                onToggle={(t) => toggle(i.name, t)}
                onSet={(t) => setTargets(i.name, t)}
              />
            );
            return [row(item, 0, children.length), ...(open ? children.map((c) => row(c, 1, 0)) : [])];
          })}
        </ul>

        <div className="flex flex-wrap items-center justify-between gap-3 border-t border-neutral-200 px-5 py-3 dark:border-neutral-800">
          <div className="min-w-0 flex-1 text-neutral-600 dark:text-neutral-400">
            {inspection.music && (
              <p className="flex flex-wrap items-center gap-x-2 gap-y-1">
                <Music size={14} aria-hidden />
                {musicTo ? (
                  <span className="break-words">
                    {inspection.music} goes in {shownPath(musicTo)}
                  </span>
                ) : (
                  <>
                    <span className="break-words">Your show isn&apos;t saved yet, so choose a folder for {inspection.music}:</span>
                    <Button className="px-2 py-0.5 text-xs" onClick={() => void chooseMusicFolder()}>
                      <FolderOpen size={12} aria-hidden /> Choose folder…
                    </Button>
                  </>
                )}
              </p>
            )}
            {error && (
              <p role="alert" className="mt-1 text-red-600 dark:text-red-400">
                {error}
              </p>
            )}
          </div>
          <div className="flex gap-2">
            <Button onClick={close}>Cancel</Button>
            <Button variant="primary" disabled={busy || stats.mapped === 0} onClick={() => void importNow()}>
              Import {plural(stats.mappedEffects, "effect")}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}
