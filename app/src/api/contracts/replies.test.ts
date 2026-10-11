// What the shell sends must fit the window's types. The files in replies/ are written by the
// shell's own tests from real commands (app/src-tauri/src/tests/contracts); each value there names
// the TypeScript type it must fit, and the compiler is asked whether it does.

import { describe, expect, it } from "vitest";
import { AssistantError, assistantFailure } from "../assistant";
import { errorMessage } from "../backend";
import catalogJson from "../effectCatalog.json";
import { type Typed, listJson, readJson, typeFacts, typeProblems } from "./support";

/** A value from the shell: a command's reply, an event's payload, or what a command was rejected with. */
interface Fixture extends Typed {
  /** The event this is the payload of. */
  event?: string;
  /** The command that was rejected with this. */
  rejectedBy?: string;
  /** `value` lists every member of this union (a type expression), as the shell sends them. */
  allOf?: string;
  /** The field that tells the variants of `allOf` apart, when it's a union of objects. */
  tag?: string;
  /** `allOf` is a union of objects told apart by which keys they have. */
  byKeys?: boolean;
}

/** The catalog as far as these checks read it. */
const catalog = catalogJson as unknown as { kind: string; settings: { key: string; type: string; options?: { value: string }[] }[] }[];

const files = listJson("replies");
const fixtures = files.flatMap((file) => readJson<Fixture[]>(`replies/${file}`).map((f) => ({ ...f, name: `${file}: ${f.name}` })));

/** Fields the window's types declare that no fixture has, and why (see the test that reads this). */
const NEVER_SENT: string[] = [];

// The compiler takes a few seconds the first time (it reads the standard library).
const SLOW = 120_000;

function show(problems: { name: string; message: string }[]): string {
  return problems.map((p) => `${p.name}\n  ${p.message}`).join("\n\n");
}

describe("what the shell sends", () => {
  it("fits the window's types", { timeout: SLOW }, () => {
    expect(fixtures.length).toBeGreaterThan(100);
    expect(show(typeProblems(fixtures))).toBe("");
  });

  it("has every member of the window's unions, and no other", { timeout: SLOW }, () => {
    const unions = fixtures.filter((f) => f.allOf !== undefined);
    expect(unions.length).toBeGreaterThan(40);
    const tagOf = (f: Fixture) => `(${f.allOf})[${JSON.stringify(f.tag)}]`;
    const facts = typeFacts(unions.map((f) => (f.tag ? tagOf(f) : f.allOf!)));
    const differences: string[] = [];
    for (const f of unions) {
      const values = f.value as unknown[];
      if (f.byKeys) {
        const sent = [...new Set(values.map((v) => Object.keys(v as object).sort().join(",")))].sort();
        const known = facts.memberKeys(f.allOf!).map((keys) => keys.join(",")).sort();
        if (sent.join(" | ") !== known.join(" | ")) differences.push(`${f.allOf}: the shell sends objects with the keys ${sent.join(" | ")}; the type has ${known.join(" | ")}`);
        continue;
      }
      const sent = [...new Set(values.map((v) => (f.tag ? (v as Record<string, string>)[f.tag] : (v as string))))].sort();
      const known = facts.literals(f.tag ? tagOf(f) : f.allOf!);
      const missing = sent.filter((s) => !known.includes(s));
      const extra = known.filter((k) => !sent.includes(k));
      if (missing.length > 0) differences.push(`${f.allOf}: the shell also sends ${missing.join(", ")}`);
      if (extra.length > 0) differences.push(`${f.allOf}: the shell never sends ${extra.join(", ")}`);
    }
    expect(differences.join("\n")).toBe("");
  });

  it("sends every field the window's types declare", { timeout: SLOW }, () => {
    // A field the window's type has but the shell never sends is one the shell may have renamed
    // (an optional one would go unnoticed otherwise), or one no fixture shows yet.
    const keys = new Set<string>();
    const collect = (value: unknown) => {
      if (Array.isArray(value)) value.forEach(collect);
      else if (value !== null && typeof value === "object") {
        for (const [key, inner] of Object.entries(value)) {
          keys.add(key);
          collect(inner);
        }
      }
    };
    fixtures.forEach((f) => collect(f.value));
    const declared = typeFacts(fixtures.map((f) => f.ts)).declared();
    expect(declared.length).toBeGreaterThan(300);
    const unsent = declared.filter((d) => !keys.has(d.slice(d.lastIndexOf(".") + 1)));
    expect(unsent).toEqual(NEVER_SENT);
  });

  it("sends each event with the payload its listener expects", { timeout: SLOW }, () => {
    const events = fixtures.filter((f) => f.event !== undefined);
    const facts = typeFacts(events.map((f) => f.ts));
    const listeners = facts.listeners();
    expect(listeners.length).toBeGreaterThan(8);
    const problems: string[] = [];
    for (const listener of listeners) {
      const sent = events.filter((f) => f.event === listener.event);
      if (sent.length === 0) problems.push(`${listener.file} listens for "${listener.event}", which the shell never sends`);
      // A listener that takes no payload (it only wants to know that it happened) fits any.
      for (const f of sent) {
        if (listener.payload !== null && !listener.sameAs(f.ts)) {
          problems.push(`${listener.file} takes "${listener.event}" to carry ${listener.payload}, but the shell's fixture says ${f.ts} (${f.name})`);
        }
      }
    }
    for (const event of new Set(events.map((f) => f.event!))) {
      if (!listeners.some((l) => l.event === event)) problems.push(`the shell sends "${event}", which nothing in src/api listens for`);
    }
    expect(problems.join("\n")).toBe("");
  });

  it("rejects with a sentence to show, or the assistant's message and details", () => {
    const rejections = fixtures.filter((f) => f.rejectedBy !== undefined);
    expect(rejections.length).toBeGreaterThan(10);
    for (const f of rejections) {
      if (typeof f.value === "string") {
        expect(errorMessage(f.value), f.name).toBe(f.value);
        expect(f.value.length, f.name).toBeGreaterThan(10);
      } else {
        const failure = assistantFailure(f.value);
        expect(failure, f.name).toBeInstanceOf(AssistantError);
        expect((failure as AssistantError).message, f.name).toBe((f.value as { message: string }).message);
        expect((failure as AssistantError).details, f.name).toBe((f.value as { details: string | null }).details);
      }
    }
  });
});

describe("the effect catalog (the engine's own, see effectCatalog.json)", () => {
  it("fits the window's types, with every kind of setting", { timeout: SLOW }, () => {
    expect(show(typeProblems([{ name: "effectCatalog.json", ts: "sequence.EffectInfo[]", value: catalogJson }]))).toBe("");
    const facts = typeFacts(["sequence.EffectSetting['type']"]);
    const kinds = [...new Set(catalog.flatMap((effect) => effect.settings.map((s) => s.type)))].sort();
    expect(kinds).toEqual(facts.literals("sequence.EffectSetting['type']"));
  });

  it("offers exactly the choices the window's effect settings allow", { timeout: SLOW }, () => {
    // Each choice setting, as "<kind>.<key>", with the type its params give it.
    const choices = catalog.flatMap((effect) =>
      effect.settings.filter((s) => s.type === "choice").map((s) => ({ effect: effect.kind, key: s.key, options: (s.options ?? []).map((o) => o.value).sort() })),
    );
    expect(choices.length).toBeGreaterThan(30);
    const query = (c: { effect: string; key: string }) => `NonNullable<Extract<sequence.EffectParams, { kind: ${JSON.stringify(c.effect)} }>[${JSON.stringify(c.key)}]>`;
    const facts = typeFacts(choices.map(query));
    const differences = choices.filter((c) => facts.literals(query(c)).join(",") !== c.options.join(",")).map((c) => `${c.effect}.${c.key}: the engine offers ${c.options.join(", ")}; the type allows ${facts.literals(query(c)).join(", ")}`);
    expect(differences.join("\n")).toBe("");
  });

  it("has exactly the settings the window's effect params declare", { timeout: SLOW }, () => {
    const query = (kind: string) => `Extract<sequence.EffectParams, { kind: ${JSON.stringify(kind)} }>`;
    const facts = typeFacts(catalog.map((effect) => query(effect.kind)));
    const differences: string[] = [];
    for (const effect of catalog) {
      const declared = facts.properties(query(effect.kind)).map((p) => p.replace(/\?$/, "")).filter((p) => p !== "kind");
      const settings = effect.settings.map((s) => s.key).sort();
      if (declared.join(",") !== settings.join(",")) differences.push(`${effect.kind}: the engine has ${settings.join(", ")}; the type declares ${declared.join(", ")}`);
    }
    expect(differences.join("\n")).toBe("");
  });
});

describe("the check itself", () => {
  it("refuses a renamed field, a field the type doesn't have, a missing one, a wrong tag, and null where none is allowed", { timeout: SLOW }, () => {
    const entry = { id: "h1", savedAtMs: 1, sizeBytes: 2 };
    const wrong: Typed[] = [
      { name: "right", ts: "types.HistoryEntry", value: entry },
      { name: "renamed", ts: "types.HistoryEntry", value: { id: "h1", savedAt: 1, sizeBytes: 2 } },
      { name: "extra", ts: "types.HistoryEntry", value: { ...entry, kind: "autosave" } },
      { name: "missing", ts: "types.HistoryEntry", value: { id: "h1", savedAtMs: 1 } },
      { name: "wrong tag", ts: "types.TargetSpec", value: { type: "everything" } },
      { name: "wrong literal", ts: "types.PlaybackStatus['state']", value: "stopped" },
      { name: "null", ts: "types.HistoryEntry", value: { ...entry, sizeBytes: null } },
      { name: "extra in a union's variant", ts: "types.TargetSpec", value: { type: "prop", id: "p", port: 1 } },
      { name: "nested", ts: "types.HistoryEntry[]", value: [entry, { ...entry, id: 7 }] },
    ];
    const refused = new Set(typeProblems(wrong).map((p) => p.name));
    expect([...refused].sort()).toEqual(wrong.slice(1).map((w) => w.name).sort());
  });

  it("reads unions and listeners from the window's own source", { timeout: SLOW }, () => {
    const facts = typeFacts(["types.CodeBase", "types.FppSendProgress", "types.FppDownloadProgress", "sequence.SequenceTarget"]);
    expect(facts.literals("types.CodeBase")).toEqual(["four", "two"]);
    expect(facts.memberKeys("sequence.SequenceTarget")).toEqual([["prop"], ["group"], ["region"]]);
    const send = facts.listeners().find((l) => l.event === "fpp-send-progress")!;
    expect(send.payload).toBe("FppSendProgress");
    expect(send.sameAs("types.FppSendProgress")).toBe(true);
    expect(send.sameAs("types.FppDownloadProgress")).toBe(false);
  });
});
