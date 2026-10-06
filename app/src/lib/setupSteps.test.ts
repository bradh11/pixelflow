import { describe, expect, it } from "vitest";
import { emptyShow } from "../api/memory";
import { demoShow } from "../api/demo";
import type { Show } from "../api/types";
import { newController, newProp } from "./shows";
import { nextStep, setupSteps } from "./setupSteps";

const none = { tested: false, sequenced: false };

describe("setting up a show", () => {
  it("starts with every step to do, drawing the props first (as the sidebar has it)", () => {
    const steps = setupSteps(emptyShow("New"), none);
    expect(steps.map((s) => [s.id, s.done])).toEqual([
      ["props", false],
      ["controllers", false],
      ["wiring", false],
      ["test", false],
      ["sequence", false],
      ["play", false],
    ]);
    expect(steps.map((s) => s.screen)).toEqual(["layout", "devices", "wiring", "test", "sequence", "play"]);
    expect(steps.at(-1)?.label).toBe("Add it to the playlist");
    expect(nextStep(steps)?.id).toBe("props");
  });

  it("ticks controllers and props from the show, and says how many props aren't wired", () => {
    const show: Show = emptyShow("New");
    show.controllers.push(newController("Main", "10.0.0.2", "ddp", 4));
    show.props.push(newProp("arch", show), newProp("line", show));
    const steps = setupSteps(show, none);
    expect(steps.find((s) => s.id === "controllers")).toMatchObject({ done: true, detail: "1 controller" });
    expect(steps.find((s) => s.id === "props")).toMatchObject({ done: true, detail: "2 props" });
    expect(steps.find((s) => s.id === "wiring")).toMatchObject({ done: false, detail: "2 props not wired" });
    expect(nextStep(steps)?.id).toBe("wiring");
  });

  it("isn't wired until every pixel of every prop is", () => {
    // The demo's Porch Star isn't wired.
    const steps = setupSteps(demoShow(), none);
    expect(steps.find((s) => s.id === "wiring")).toMatchObject({ done: false, detail: "1 prop not wired" });
  });

  it("is wired when there are props and every one is wired", () => {
    const show = demoShow();
    show.props = show.props.filter((p) => p.name !== "Porch Star");
    expect(setupSteps(show, none).find((s) => s.id === "wiring")).toMatchObject({ done: true, detail: "All wired" });
  });

  it("ticks testing, a sequence, and playing from what's been done", () => {
    const show = demoShow();
    const steps = setupSteps(show, { tested: true, sequenced: true });
    expect(steps.find((s) => s.id === "test")?.done).toBe(true);
    expect(steps.find((s) => s.id === "sequence")?.done).toBe(true);
    // Playing: a sequence on the show's playlist.
    expect(steps.find((s) => s.id === "play")?.done).toBe(show.sequences.length > 0);
    show.sequences = [{ id: "s1", name: "Medley", path: "/Shows/Medley.fseq", audio: null, offsetMs: 0 }];
    expect(setupSteps(show, none).find((s) => s.id === "play")?.done).toBe(true);
  });

  it("has no next step when everything is done", () => {
    const show = demoShow();
    show.props = show.props.filter((p) => p.name !== "Porch Star");
    show.sequences = [{ id: "s1", name: "Medley", path: "/Shows/Medley.fseq", audio: null, offsetMs: 0 }];
    expect(nextStep(setupSteps(show, { tested: true, sequenced: true }))).toBeNull();
  });
});
