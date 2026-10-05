import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";
import { usePaletteDrag } from "../components/sequencer/EffectPalette";
import { useLayoutEditor } from "../state/layoutEditor";
import { useSequencer } from "../state/sequencer";
import { useApp } from "../state/store";
import { useView3d } from "../state/view3d";
import { useWiring } from "../state/wiring";

// jsdom lacks these browser APIs; the command palette (cmdk) uses them.
globalThis.ResizeObserver ??= class {
  observe() {}
  unobserve() {}
  disconnect() {}
};
Element.prototype.scrollIntoView ??= function scrollIntoView() {};
// jsdom has no 2D canvas; the preview draws nothing in tests.
HTMLCanvasElement.prototype.getContext = (() => null) as typeof HTMLCanvasElement.prototype.getContext;

afterEach(() => {
  cleanup();
  localStorage.clear();
  useApp.setState(useApp.getInitialState(), true);
  useLayoutEditor.setState(useLayoutEditor.getInitialState(), true);
  useView3d.setState(useView3d.getInitialState(), true);
  useSequencer.setState(useSequencer.getInitialState(), true);
  usePaletteDrag.setState(usePaletteDrag.getInitialState(), true);
  useWiring.setState(useWiring.getInitialState(), true);
});
