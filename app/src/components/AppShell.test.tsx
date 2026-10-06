import { describe, expect, it } from "vitest";
import { problemCount } from "./AppShell";

describe("the status bar's problem count", () => {
  it("says only what there is", () => {
    expect(problemCount(0, 0)).toBe("No problems");
    expect(problemCount(0, 3)).toBe("3 warnings");
    expect(problemCount(1, 0)).toBe("1 error");
    expect(problemCount(1, 2)).toBe("1 error, 2 warnings");
  });
});
