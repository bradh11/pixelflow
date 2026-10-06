import { describe, expect, it } from "vitest";
import { MemoryBackend, emptyShow } from "../api/memory";
import { currentSetupKey, useSetup } from "./setup";
import { useApp } from "./store";

const tested = () => useSetup.getState().tested.includes(currentSetupKey()!);
const dismissed = () => useSetup.getState().dismissed.includes(currentSetupKey()!);

describe("what the setup checklist remembers", () => {
  it("keeps each new show to itself, though they all start as Untitled Show", async () => {
    await useApp.getState().connect(new MemoryBackend(emptyShow("Untitled Show")));
    useSetup.getState().markTested(currentSetupKey());
    useSetup.getState().setDismissed(currentSetupKey(), true);
    expect(tested()).toBe(true);
    await useApp.getState().newShow();
    expect(useApp.getState().snapshot?.show.name).toBe("Untitled Show");
    expect(tested()).toBe(false);
    expect(dismissed()).toBe(false);
  });

  it("carries a new show's record over when it's first saved", async () => {
    await useApp.getState().connect(new MemoryBackend(emptyShow("Untitled Show")));
    useSetup.getState().markTested(currentSetupKey());
    useSetup.getState().setDismissed(currentSetupKey(), true);
    await useApp.getState().run((b) => b.saveShowAs("/Shows/Home.pixelflow.json"));
    expect(currentSetupKey()).toBe("/Shows/Home.pixelflow.json");
    expect(tested()).toBe(true);
    expect(dismissed()).toBe(true);
  });
});
