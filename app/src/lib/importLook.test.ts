import { describe, expect, it } from "vitest";
import type { ByVerdict, LookGroup } from "../api/types";
import { lookHeadline, lookShares, looksByVerdict } from "./importLook";

const ms = (matches: number, close: number, different: number, noEquivalent: number): ByVerdict => ({ matches, close, different, noEquivalent });

describe("how an imported sequence comes across", () => {
  // The same cases as the engine's `the_headline_shares_out_the_sequence_by_how_its_effects_look`.
  it("shares the sequence out by how long its effects of each verdict run", () => {
    expect(lookHeadline(ms(82_000, 12_000, 6_000, 0))).toBe(
      "82% of this sequence comes across the same · 12% looks close · 6% looks different",
    );
    expect(lookHeadline(ms(50_000, 0, 0, 50_000))).toBe("50% of this sequence comes across the same · 50% has no PixelFlow effect yet");
    expect(lookHeadline(ms(4_000, 0, 0, 0))).toBe("100% of this sequence comes across the same");
  });

  it("still says when nothing matches, and doesn't round next to nothing away or up to all", () => {
    expect(lookHeadline(ms(0, 999_000, 1_000, 0))).toBe(
      "0% of this sequence comes across the same · 99% looks close · under 1% looks different",
    );
    expect(lookShares(ms(0, 999_000, 1_000, 0))).toEqual({ matches: null, close: "99%", different: "under 1%", noEquivalent: null });
  });

  it("has nothing to say of a sequence with no effects", () => {
    expect(lookHeadline(ms(0, 0, 0, 0))).toBeNull();
  });

  it("groups what differs by verdict, worst first", () => {
    const group = (verdict: LookGroup["verdict"], effect: string): LookGroup => ({ verdict, effect, effects: 1, ms: 1000, reasons: [] });
    const sections = looksByVerdict([group("noEquivalent", "Shader"), group("different", "Fireworks"), group("close", "Bars"), group("close", "Text")]);
    expect(sections.map((s) => [s.verdict, s.groups.map((g) => g.effect)])).toEqual([
      ["noEquivalent", ["Shader"]],
      ["different", ["Fireworks"]],
      ["close", ["Bars", "Text"]],
    ]);
    expect(looksByVerdict([])).toEqual([]);
  });
});
