import { describe, expect, it } from "vitest";
import { demoShow } from "./demo";
import { sampleShow, withNumberedIds } from "./sampleShow";

/** The demo show as the sample file keeps it: no photo (it lives only in the browser demo). */
function demoAsSample(): string {
  const show = demoShow();
  delete show.background;
  return withNumberedIds(JSON.stringify(show, null, 2));
}

describe("the sample show", () => {
  it("is the ?demo show", () => {
    expect(withNumberedIds(JSON.stringify(sampleShow(), null, 2))).toBe(demoAsSample());
  });

  it("comes out as a fresh copy every time", () => {
    const first = sampleShow();
    first.name = "Changed";
    expect(sampleShow().name).toBe("Demo House");
  });
});
