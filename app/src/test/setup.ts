import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";
import { useApp } from "../state/store";

// jsdom lacks these browser APIs; the command palette (cmdk) uses them.
globalThis.ResizeObserver ??= class {
  observe() {}
  unobserve() {}
  disconnect() {}
};
Element.prototype.scrollIntoView ??= function scrollIntoView() {};

afterEach(() => {
  cleanup();
  localStorage.clear();
  useApp.setState(useApp.getInitialState(), true);
});
