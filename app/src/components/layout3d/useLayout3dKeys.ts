import { useEffect } from "react";
import { PRESETS } from "../../lib/layout3d";
import { useLayoutEditor } from "../../state/layoutEditor";
import { useApp } from "../../state/store";
import { useView3d } from "../../state/view3d";
import { typing } from "../layout/useLayoutKeys";

/** Tools that draw props: 2D only (for now). */
export const drawsProps = (tool: string) => tool !== "select" && tool !== "pan";

/** Switches the Layout screen between 2D and 3D; drawing tools put down for 3D. */
export function setLayoutMode(mode: "2d" | "3d") {
  if (mode === "3d") {
    const editor = useLayoutEditor.getState();
    if (drawsProps(editor.tool) || editor.editPhoto) editor.setTool("select");
  }
  useView3d.getState().setMode(mode);
}

/**
 * Layout screen keys for the view: V switches 2D and 3D; in 3D, F fits everything in and 1–5
 * pick the Front, Top, Left, Right, and Street views. Typing in a field is left alone.
 */
export function useLayout3dKeys() {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const app = useApp.getState();
      if (app.paletteOpen || app.pendingReplace || e.defaultPrevented || e.metaKey || e.ctrlKey || e.altKey) return;
      if (typing(e.target) || !app.snapshot) return;
      const view = useView3d.getState();
      const key = e.key.toLowerCase();
      if (key === "v") {
        e.preventDefault();
        setLayoutMode(view.mode === "3d" ? "2d" : "3d");
        return;
      }
      if (view.mode !== "3d") return;
      if (key === "f") {
        e.preventDefault();
        view.camera({ kind: "fit" });
        return;
      }
      const preset = PRESETS.find((p) => p.key === e.key);
      if (preset) {
        e.preventDefault();
        view.camera({ kind: "preset", preset: preset.preset });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
}
