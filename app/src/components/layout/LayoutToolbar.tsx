import {
  AppWindow,
  Box,
  Globe,
  CandyCane,
  ChevronDown,
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
import { drawsProps, setLayoutMode } from "../layout3d/useLayout3dKeys";

interface ToolInfo {
  tool: Tool;
  label: string;
  hint: string;
  icon: LucideIcon;
}

/** The tools always on the bar. */
const TOOLS: ToolInfo[] = [
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
 * When a button's label shows. The bar keeps to one row: as it narrows, the toggles and Fit
 * show only their icons first, then every button does (the label is still the button's name,
 * and its hint shows on hover).
 */
export type LabelShown = "always" | "wide" | "medium";
const LABEL_CLASS: Record<LabelShown, string | undefined> = {
  always: undefined,
  wide: "sr-only @min-[1200px]:not-sr-only",
  medium: "sr-only @min-[1000px]:not-sr-only",
};

function ToolButton({
  pressed,
  label,
  hint,
  onClick,
  disabled,
  popup,
  expanded,
  labelShown = "medium",
  children,
}: {
  pressed?: boolean;
  label: string;
  hint: string;
  onClick: () => void;
  disabled?: boolean;
  /** Opens a menu (with a small arrow after the label). */
  popup?: boolean;
  expanded?: boolean;
  labelShown?: LabelShown;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-pressed={pressed}
      aria-haspopup={popup ? "menu" : undefined}
      aria-expanded={popup ? expanded : undefined}
      title={hint}
      // aria-disabled rather than disabled: the hint saying why still shows on hover.
      aria-disabled={disabled || undefined}
      onClick={disabled ? undefined : onClick}
      className={`inline-flex items-center gap-1.5 rounded-md px-2 py-1.5 text-sm transition-colors ${
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

/** Draw tools are 2D only (for now): what their buttons say in 3D. */
const DRAW_IN_2D = "Drawing works in the 2D view — switch with V";

/** Tools for drawing and arranging, plus snap, zoom, and the background photo. */
export function LayoutToolbar({ hasPhoto, onChoosePhoto }: { hasPhoto: boolean; onChoosePhoto: () => void }) {
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
      className="@container mb-3 flex flex-wrap items-center gap-0.5 rounded-lg border border-neutral-200 bg-white p-1 dark:border-neutral-800 dark:bg-neutral-900"
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
      <ToolButton pressed={snap} label="Snap to grid" hint="Line props up on a grid as you move and draw" labelShown="wide" onClick={() => setSnap(!snap)}>
        <Magnet size={16} aria-hidden />
      </ToolButton>
      <ToolButton
        pressed={smartGuides}
        label="Smart guides"
        hint="Line props up with others, space them evenly, and match sizes as you move, resize, and draw (hold Option/Alt to place freely)"
        labelShown="wide"
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
      <ToolButton label="Fit" hint="Show the whole display" labelShown="wide" onClick={() => (in3d ? camera({ kind: "fit" }) : setView(null))}>
        <Maximize size={16} aria-hidden />
      </ToolButton>
      <Divider />
      {hasPhoto ? (
        <ToolButton
          pressed={editPhoto}
          label="Edit photo"
          hint={in3d ? "Move the photo in the 2D view — switch with V" : "Drag the photo to move it, or its corners to resize it"}
          disabled={in3d}
          onClick={() => setEditPhoto(!editPhoto)}
        >
          <ImagePlus size={16} aria-hidden />
        </ToolButton>
      ) : (
        <ToolButton label="Add photo…" hint="Draw your display over a photo of your house" onClick={onChoosePhoto}>
          <ImagePlus size={16} aria-hidden />
        </ToolButton>
      )}
    </div>
  );
}
