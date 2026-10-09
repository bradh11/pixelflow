import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach, beforeEach } from "vitest";
import { usePaletteDrag } from "../components/sequencer/EffectPalette";
import { useAssistant } from "../state/assistant";
import { useLayoutEditor } from "../state/layoutEditor";
import { useSequencer } from "../state/sequencer";
import { useApp } from "../state/store";
import { useConfirm } from "../state/confirm";
import { useSetup } from "../state/setup";
import { useUndoLabels } from "../state/undoLabels";
import { usePropertiesPanel } from "../components/layout/PropertiesDock";
import { useListWidth } from "../components/layout/SidePanel";
import { useToasts } from "../state/toast";
import { useView3d } from "../state/view3d";
import { useWiring } from "../state/wiring";
import { useShortcutSheet } from "../state/shortcutSheet";
import { useContextMenu } from "../state/contextMenu";
import { playClock, usePreviewSync } from "../state/previewSync";
import { useLyricTools } from "../state/lyricTools";

// jsdom lacks these browser APIs; the command palette (cmdk) uses them.
globalThis.ResizeObserver ??= class {
  observe() {}
  unobserve() {}
  disconnect() {}
};
Element.prototype.scrollIntoView ??= function scrollIntoView() {};
// jsdom has no 2D canvas; the preview draws nothing in tests.
HTMLCanvasElement.prototype.getContext = (() => null) as typeof HTMLCanvasElement.prototype.getContext;

// A wide desktop window unless a test narrows it (jsdom starts at 1024).
beforeEach(() => {
  window.innerWidth = 1920;
});

afterEach(() => {
  cleanup();
  localStorage.clear();
  useApp.setState(useApp.getInitialState(), true);
  useLayoutEditor.setState(useLayoutEditor.getInitialState(), true);
  useView3d.setState(useView3d.getInitialState(), true);
  useSequencer.setState(useSequencer.getInitialState(), true);
  usePaletteDrag.setState(usePaletteDrag.getInitialState(), true);
  useWiring.setState(useWiring.getInitialState(), true);
  useAssistant.setState(useAssistant.getInitialState(), true);
  useToasts.setState(useToasts.getInitialState(), true);
  useConfirm.setState(useConfirm.getInitialState(), true);
  useSetup.setState({ tested: [], dismissed: [] });
  useUndoLabels.setState(useUndoLabels.getInitialState(), true);
  usePropertiesPanel.setState({ pref: "auto", hinted: false });
  useListWidth.setState({ width: 256 });
  useShortcutSheet.setState({ open: false });
  useContextMenu.setState({ menu: null });
  usePreviewSync.setState({ offsetMs: 0 });
  useLyricTools.setState({ vocalsShown: false, tap: null });
  playClock.set(null);
});
