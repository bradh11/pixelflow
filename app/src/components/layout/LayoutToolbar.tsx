import {
  AppWindow,
  Box,
  Globe,
  CandyCane,
  ChevronDown,
  CircleHelp,
  PanelRight,
  Circle,
  CircleDot,
  Droplets,
  Grid3x3,
  ImagePlus,
  LayoutGrid,
  LoaderPinwheel,
  Magnet,
  Maximize,
  MousePointer2,
  Rainbow,
  Ruler,
  Shapes,
  Slash,
  Spline,
  Star,
  TreePine,
  ZoomIn,
  ZoomOut,
  type LucideIcon,
} from "lucide-react";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import { DEFAULT_VIEW, MAX_ZOOM, MIN_ZOOM } from "../../lib/layoutMath";
import { type Tool, useLayoutEditor } from "../../state/layoutEditor";
import { useView3d } from "../../state/view3d";
import { ModeSwitch } from "../layout3d/ModeSwitch";
import { usePropertiesOpen } from "./PropertiesDock";
import { drawsProps, setLayoutMode } from "../layout3d/useLayout3dKeys";

export interface ToolInfo {
  tool: Tool;
  label: string;
  hint: string;
  icon: LucideIcon;
}

/** The tools always on the bar. */
export const TOOLS: ToolInfo[] = [
  { tool: "select", label: "Select", hint: "Select, move, resize, and turn props", icon: MousePointer2 },
  { tool: "line", label: "Line", hint: "Drag from one end of a light string to the other", icon: Slash },
  {
    tool: "polyLine",
    label: "Poly Line",
    hint: "Click each point of a line that bends; double-click or press Enter to finish. Start or end on another line's end to join it",
    icon: Spline,
  },
  { tool: "arch", label: "Arch", hint: "Drag from one foot of the arch to the other", icon: Rainbow },
  { tool: "matrix", label: "Matrix", hint: "Drag a box where the matrix goes", icon: Grid3x3 },
  { tool: "tree", label: "Tree", hint: "Drag a box from the tree's base to its top", icon: TreePine },
];

/** The rest of the shapes, under "More shapes". */
export const MORE_TOOLS: ToolInfo[] = [
  { tool: "circle", label: "Circle", hint: "Drag a box around the circle", icon: Circle },
  { tool: "star", label: "Star", hint: "Drag a box around the star", icon: Star },
  { tool: "candyCanes", label: "Candy canes", hint: "Drag from where the first cane stands to where the last one does", icon: CandyCane },
  { tool: "icicles", label: "Icicles", hint: "Drag along the line the icicles hang from", icon: Droplets },
  { tool: "windowFrame", label: "Window frame", hint: "Drag a box around the window", icon: AppWindow },
  { tool: "wreath", label: "Wreath", hint: "Drag a box around the wreath", icon: CircleDot },
  { tool: "spinner", label: "Spinner", hint: "Drag a box around the spinner", icon: LoaderPinwheel },
  { tool: "sphere", label: "Sphere", hint: "Drag a box around the sphere", icon: Globe },
  { tool: "cube", label: "Cube", hint: "Drag a box around the front of the cube", icon: Box },
  { tool: "customGrid", label: "Custom grid", hint: "Drag a box where the grid goes, then number its squares in the properties panel", icon: LayoutGrid },
];

/**
 * When a button's label shows. The bar keeps to one row: as it narrows, labels give way a group
 * at a time, least needed first: the snap and guides toggles, then Fit and the photo, then More
 * shapes, and last the drawing tools. (The label is still the button's name, and its hint shows
 * on hover.)
 */
export type LabelShown = "always" | "toggles" | "view" | "shapes" | "tools";
/** The bar width (px) from which each group's labels show. */
export const LABEL_FROM: Record<Exclude<LabelShown, "always">, number> = { toggles: 1160, view: 1000, shapes: 900, tools: 800 };
// Spelled out in full so the class names are found when the styles are built.
export const LABEL_CLASS: Record<LabelShown, string | undefined> = {
  always: undefined,
  toggles: "sr-only @min-[1160px]:not-sr-only",
  view: "sr-only @min-[1000px]:not-sr-only",
  shapes: "sr-only @min-[900px]:not-sr-only",
  tools: "sr-only @min-[800px]:not-sr-only",
};

function ToolButton({
  pressed,
  label,
  hint,
  onClick,
  disabled,
  popup,
  expanded,
  labelShown = "tools",
  children,
}: {
  pressed?: boolean;
  label: string;
  hint: string;
  onClick: () => void;
  disabled?: boolean;
  /** Opens a menu or a small panel (with a small arrow after the label). */
  popup?: boolean | "dialog";
  expanded?: boolean;
  labelShown?: LabelShown;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-pressed={pressed}
      aria-haspopup={popup === "dialog" ? "dialog" : popup ? "menu" : undefined}
      aria-expanded={popup ? expanded : undefined}
      title={hint}
      // aria-disabled rather than disabled: the hint saying why still shows on hover.
      aria-disabled={disabled || undefined}
      onClick={disabled ? undefined : onClick}
      className={`inline-flex items-center gap-1 rounded-md px-1.5 py-1.5 text-sm transition-colors ${
        disabled
          ? "cursor-not-allowed text-neutral-700 opacity-40 dark:text-neutral-300"
          : pressed
            ? "bg-accent-600 text-white"
            : "text-neutral-700 hover:bg-neutral-200/70 dark:text-neutral-300 dark:hover:bg-neutral-800"
      }`}
    >
      {children}
      <span className={LABEL_CLASS[labelShown]}>{label}</span>
      {popup && <ChevronDown size={14} aria-hidden />}
    </button>
  );
}

const Divider = () => <span aria-hidden className="mx-1 h-6 w-px bg-neutral-300 dark:bg-neutral-700" />;

/**
 * "More shapes": a menu of the less common shapes. Its button shows the shape picked from it
 * while that tool is on.
 */
function MoreShapes({ tool, setTool, in3d }: { tool: Tool; setTool: (t: Tool) => void; in3d: boolean }) {
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  const picked = MORE_TOOLS.find((t) => t.tool === tool);
  useEffect(() => {
    if (!open) return;
    box.current?.querySelector<HTMLButtonElement>("[role=menuitem]")?.focus();
    // Ahead of the layout keys, so Escape only closes the menu.
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        setOpen(false);
      }
    };
    const onDown = (e: PointerEvent) => {
      if (!box.current?.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("pointerdown", onDown);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("pointerdown", onDown);
    };
  }, [open]);
  const Icon = picked?.icon ?? Shapes;
  return (
    <div ref={box} className="relative">
      <ToolButton
        pressed={!!picked}
        label={picked ? picked.label : "More shapes"}
        hint={in3d ? DRAW_IN_2D : "More kinds of props: circles, stars, candy canes, icicles, window frames, wreaths, spinners, spheres, cubes, custom grids"}
        disabled={in3d}
        onClick={() => setOpen(!open)}
        popup
        expanded={open}
        labelShown="shapes"
      >
        <Icon size={16} aria-hidden />
      </ToolButton>
      {open && (
        <div
          role="menu"
          aria-label="More shapes"
          className="absolute top-full left-0 z-30 mt-1 flex w-56 flex-col rounded-lg border border-neutral-200 bg-white p-1 text-sm text-neutral-800 shadow-xl dark:border-neutral-800 dark:bg-neutral-900 dark:text-neutral-100"
        >
          {MORE_TOOLS.map(({ tool: t, label, hint, icon: ItemIcon }) => (
            <button
              key={t}
              type="button"
              role="menuitem"
              title={hint}
              className={`flex items-center gap-2 rounded px-2 py-1.5 text-left hover:bg-neutral-100 dark:hover:bg-neutral-800 ${
                t === tool ? "text-accent-600 dark:text-accent-400" : ""
              }`}
              onClick={() => {
                setOpen(false);
                setTool(t);
              }}
            >
              <ItemIcon size={16} aria-hidden />
              {label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

/**
 * A tool bar button that opens a small panel under it (the photo's settings, the tips). Escape,
 * or a click outside, closes it.
 */
function PopoverButton({
  label,
  hint,
  icon,
  labelShown,
  children,
}: {
  label: string;
  hint: string;
  icon: ReactNode;
  labelShown: LabelShown;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    box.current?.querySelector<HTMLElement>("[role=dialog] button, [role=dialog] input")?.focus();
    // Ahead of the layout keys, so Escape only closes the panel.
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      setOpen(false);
      box.current?.querySelector<HTMLElement>("button")?.focus();
    };
    const onDown = (e: PointerEvent) => {
      if (!box.current?.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("pointerdown", onDown);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("pointerdown", onDown);
    };
  }, [open]);
  return (
    <div ref={box} className="relative">
      <ToolButton label={label} hint={hint} onClick={() => setOpen(!open)} popup="dialog" expanded={open} labelShown={labelShown}>
        {icon}
      </ToolButton>
      {open && (
        <div
          role="dialog"
          aria-label={label}
          className="absolute top-full right-0 z-30 mt-1 w-72 rounded-lg border border-neutral-200 bg-white p-3 text-sm text-neutral-800 shadow-xl dark:border-neutral-800 dark:bg-neutral-900 dark:text-neutral-100"
        >
          {children}
        </div>
      )}
    </div>
  );
}

/** Opens or puts away the properties panel (it opens by itself while a prop is selected). */
function PropertiesToggle() {
  const { open, hasSelection, toggle } = usePropertiesOpen();
  return (
    <span className="relative">
      <ToolButton
        pressed={open}
        label="Properties"
        hint={open ? "Put the properties panel away" : hasSelection ? "Show the selected prop's properties" : "Keep the properties panel open"}
        labelShown="view"
        onClick={toggle}
      >
        <PanelRight size={16} aria-hidden />
      </ToolButton>
      {hasSelection && !open && <span aria-hidden className="absolute top-1 right-1 h-1.5 w-1.5 rounded-full bg-accent-500" />}
    </span>
  );
}

/** Draw tools are 2D only (for now): what their buttons say in 3D. */
const DRAW_IN_2D = "Drawing works in the 2D view — switch with V";

/** Tools for drawing and arranging, plus snap, zoom, and the background photo. */
/**
 * `photo`: the background photo's settings, shown from the Photo button; `tips`: how to get
 * around, shown from the Tips button.
 */
export function LayoutToolbar({ photo, tips }: { photo: ReactNode; tips: string[] }) {
  // Not the view: panning and zooming don't need the tool bar redrawn.
  const { tool, setTool, snap, setSnap, editPhoto, setEditPhoto, setView } = useLayoutEditor(
    useShallow((s) => ({
      tool: s.tool,
      setTool: s.setTool,
      snap: s.snap,
      setSnap: s.setSnap,
      editPhoto: s.editPhoto,
      setEditPhoto: s.setEditPhoto,
      setView: s.setView,
    })),
  );
  const smartGuides = useLayoutEditor((s) => s.smartGuides);
  const mode = useView3d((s) => s.mode);
  const in3d = mode === "3d";
  const camera = useView3d((s) => s.camera);
  const zoom = (factor: number) => {
    if (in3d) return camera({ kind: "zoom", factor });
    const v = useLayoutEditor.getState().view ?? DEFAULT_VIEW;
    setView({ ...v, zoom: Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, v.zoom * factor)) });
  };
  return (
    <div
      role="toolbar"
      aria-label="Layout tools"
      className="@container flex min-w-0 flex-1 flex-wrap items-center gap-0.5 rounded-lg border border-neutral-200 bg-white p-1 dark:border-neutral-800 dark:bg-neutral-900"
    >
      <span className="mr-1">
        <ModeSwitch mode={mode} onChange={setLayoutMode} hint="V switches" />
      </span>
      {TOOLS.map(({ tool: t, label, hint, icon: Icon }) => {
        const off = in3d && drawsProps(t);
        return (
          <ToolButton key={t} pressed={tool === t && !editPhoto} label={label} hint={off ? DRAW_IN_2D : hint} disabled={off} onClick={() => setTool(t)}>
            <Icon size={16} aria-hidden />
          </ToolButton>
        );
      })}
      <MoreShapes tool={editPhoto ? "select" : tool} setTool={setTool} in3d={in3d} />
      <Divider />
      <ToolButton pressed={snap} label="Snap to grid" hint="Line props up on a grid as you move and draw" labelShown="toggles" onClick={() => setSnap(!snap)}>
        <Magnet size={16} aria-hidden />
      </ToolButton>
      <ToolButton
        pressed={smartGuides}
        label="Smart guides"
        hint="Line props up with others, space them evenly, and match sizes as you move, resize, and draw (hold Option/Alt to place freely)"
        labelShown="toggles"
        onClick={() => useLayoutEditor.getState().setSmartGuides(!smartGuides)}
      >
        <Ruler size={16} aria-hidden />
      </ToolButton>
      <Divider />
      <button type="button" aria-label="Zoom out" title="Zoom out" onClick={() => zoom(1 / 1.25)} className="rounded-md p-1.5 hover:bg-neutral-200/70 dark:hover:bg-neutral-800">
        <ZoomOut size={16} aria-hidden />
      </button>
      <button type="button" aria-label="Zoom in" title="Zoom in" onClick={() => zoom(1.25)} className="rounded-md p-1.5 hover:bg-neutral-200/70 dark:hover:bg-neutral-800">
        <ZoomIn size={16} aria-hidden />
      </button>
      <ToolButton label="Fit" hint="Show the whole display" labelShown="view" onClick={() => (in3d ? camera({ kind: "fit" }) : setView(null))}>
        <Maximize size={16} aria-hidden />
      </ToolButton>
      <Divider />
      <PopoverButton
        label={in3d ? "Photo and model" : "Photo"}
        hint={
          in3d
            ? "The photo of your house and its 3D model behind the props"
            : "The photo of your house behind the props: add, move or resize, dim, replace, or remove it"
        }
        icon={<ImagePlus size={16} aria-hidden />}
        labelShown="view"
      >
        {photo}
      </PopoverButton>
      {editPhoto && (
        <ToolButton pressed label="Done moving photo" hint="Stop moving the photo" labelShown="always" onClick={() => setEditPhoto(false)}>
          <ImagePlus size={16} aria-hidden />
        </ToolButton>
      )}
      <PropertiesToggle />
      <PopoverButton label="Tips" hint="How to draw, select, and get around" icon={<CircleHelp size={16} aria-hidden />} labelShown="view">
        <h3 className="mb-2 text-xs font-semibold tracking-wide text-neutral-500 uppercase">Tips</h3>
        <ul className="list-disc space-y-1 pl-4 text-neutral-600 dark:text-neutral-400">
          {tips.map((tip) => (
            <li key={tip}>{tip}</li>
          ))}
        </ul>
      </PopoverButton>
    </div>
  );
}
