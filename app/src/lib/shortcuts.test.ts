import { describe, expect, it } from "vitest";
import { SHORTCUTS, SHORTCUT_GROUPS, ariaKeysFor, comboLabel, hintFor, keysLabel, searchShortcuts, shortcutOf } from "./shortcuts";

describe("the shortcut registry", () => {
  it("names each shortcut once, in a known group, with keys and a label", () => {
    const ids = SHORTCUTS.map((s) => s.id);
    expect(new Set(ids).size).toBe(ids.length);
    for (const s of SHORTCUTS) {
      expect(SHORTCUT_GROUPS).toContain(s.group);
      expect(s.keys.trim()).not.toBe("");
      expect(s.label.trim()).not.toBe("");
    }
  });

  it("doesn't give one key two meanings on the same screen", () => {
    const seen = new Map<string, string>();
    for (const s of SHORTCUTS) {
      for (const combo of s.keys.split(" ")) {
        const key = `${s.group}:${combo}`;
        expect(seen.get(key), `${combo} in ${s.group}`).toBeUndefined();
        seen.set(key, s.id);
      }
    }
  });

  it("labels keys the way a Mac keyboard does", () => {
    expect(comboLabel("Meta+Shift+S")).toBe("⇧⌘S");
    expect(comboLabel("Meta+K")).toBe("⌘K");
    expect(comboLabel("Shift+F10")).toBe("⇧F10");
    expect(comboLabel("ArrowLeft")).toBe("←");
    expect(comboLabel("Control+X")).toBe("Ctrl+X");
    expect(comboLabel("?")).toBe("?");
    // Already written for display: left as it is.
    expect(comboLabel("⌘Z")).toBe("⌘Z");
  });

  it("gives hints, aria keys and sheet labels from the same entry", () => {
    expect(hintFor("save")).toBe("⌘S");
    expect(hintFor("redo")).toBe("⇧⌘Z");
    expect(hintFor("seq-loop")).toBe("L");
    expect(ariaKeysFor("seq-play")).toBe("Space");
    expect(keysLabel(shortcutOf("layout-delete"))).toBe("⌫ / Delete");
    expect(keysLabel(shortcutOf("layout3d-views"))).toBe("1–5");
  });

  it("searches labels, groups and keys", () => {
    expect(searchShortcuts("").length).toBe(SHORTCUTS.length);
    expect(searchShortcuts("tap timing").map((s) => s.id)).toEqual(["seq-tap"]);
    expect(searchShortcuts("sequence copy").map((s) => s.id)).toEqual(["seq-copy"]);
    expect(searchShortcuts("⌘K").map((s) => s.id)).toEqual(["palette"]);
    expect(searchShortcuts("nothing like this")).toEqual([]);
  });
});
