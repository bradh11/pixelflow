import {
  Circle,
  Grid3x3,
  ImagePlus,
  Magnet,
  Maximize,
  MousePointer2,
  Rainbow,
  Ruler,
  Slash,
  Star,
  TreePine,
  ZoomIn,
  ZoomOut,
  type LucideIcon,
} from "lucide-react";
import type { ReactNode } from "react";
import { useShallow } from "zustand/react/shallow";
import { DEFAULT_VIEW, MAX_ZOOM, MIN_ZOOM } from "../../lib/layoutMath";
import { type Tool, useLayoutEditor } from "../../state/layoutEditor";
import { useView3d } from "../../state/view3d";
import { ModeSwitch } from "../layout3d/ModeSwitch";
import { drawsProps, setLayoutMode } from "../layout3d/useLayout3dKeys";

const TOOLS: { tool: Tool; label: string; hint: string; icon: LucideIcon }[] = [
  { tool: "select", label: "Select", hint: "Select, move, resize, and turn props", icon: MousePointer2 },
  { tool: "line", label: "Line", hint: "Drag from one end of a light string to the other", icon: Slash },
  { tool: "arch", label: "Arch", hint: "Drag from one foot of the arch to the other", icon: Rainbow },
  { tool: "matrix", label: "Matrix", hint: "Drag a box where the matrix goes", icon: Grid3x3 },
  { tool: "tree", label: "Tree", hint: "Drag a box from the tree's base to its top", icon: TreePine },
  { tool: "circle", label: "Circle", hint: "Drag a box around the circle or wreath", icon: Circle },
  { tool: "star", label: "Star", hint: "Drag a box around the star", icon: Star },
];

function ToolButton({
  pressed,
  label,
  hint,
  onClick,
  disabled,
  children,
}: {
  pressed?: boolean;
  label: string;
  hint: string;
  onClick: () => void;
  disabled?: boolean;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-pressed={pressed}
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
      <span>{label}</span>
    </button>
  );
}

const Divider = () => <span aria-hidden className="mx-1 h-6 w-px bg-neutral-300 dark:bg-neutral-700" />;

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
      className="mb-3 flex flex-wrap items-center gap-0.5 rounded-lg border border-neutral-200 bg-white p-1 dark:border-neutral-800 dark:bg-neutral-900"
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
      <Divider />
      <ToolButton pressed={snap} label="Snap to grid" hint="Line props up on a grid as you move and draw" onClick={() => setSnap(!snap)}>
        <Magnet size={16} aria-hidden />
      </ToolButton>
      <ToolButton
        pressed={smartGuides}
        label="Smart guides"
        hint="Line props up with others, space them evenly, and match sizes as you move, resize, and draw (hold Option/Alt to place freely)"
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
      <ToolButton label="Fit" hint="Show the whole display" onClick={() => (in3d ? camera({ kind: "fit" }) : setView(null))}>
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
