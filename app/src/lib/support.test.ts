import ts from "typescript";
import { describe, expect, it } from "vitest";
import accounts from "../../support.json";
import { supportLinks } from "./support";

const NONE = { githubSponsors: "", koFi: "", buyMeACoffee: "", payPal: "" };

/** The repository's root, from this file's own address (the app has no Node typings). */
const ROOT = (() => {
  const dir = decodeURIComponent(import.meta.url.replace(/^file:\/\//, "").replace(/[^/]*$/, ""));
  return `${/^\/[A-Za-z]:\//.test(dir) ? dir.slice(1) : dir}../../../`;
})();

/** `.github/FUNDING.yml` as GitHub reads it: each service's values, written on its own line. */
function funding(): Record<string, string[]> {
  const text = ts.sys.readFile(`${ROOT}.github/FUNDING.yml`);
  if (text === undefined) throw new Error(".github/FUNDING.yml is missing");
  const found: Record<string, string[]> = {};
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.replace(/(^|\s)#.*$/, "").trim();
    const entry = /^([a-z_]+):\s*(.*)$/.exec(line);
    if (!entry) {
      // A list written over several lines would be skipped without a word.
      expect(line, "FUNDING.yml: write each service's value on its own line").toBe("");
      continue;
    }
    found[entry[1]] = entry[2]
      .replace(/^\[|\]$/g, "")
      .split(",")
      .map((value) => value.trim().replace(/^["']|["']$/g, ""))
      .filter(Boolean);
  }
  return found;
}

describe("the ways to support the project", () => {
  it("lists nothing while no account is set", () => {
    expect(supportLinks(NONE)).toEqual([]);
  });

  it("lists each way that has an account, in order, with where it goes", () => {
    expect(supportLinks({ githubSponsors: "some-one", koFi: "someone_1", buyMeACoffee: "someone", payPal: "SomeOne" })).toEqual([
      { way: "githubSponsors", label: "GitHub Sponsors", address: "github.com/sponsors/some-one" },
      { way: "koFi", label: "Ko-fi", address: "ko-fi.com/someone_1" },
      { way: "buyMeACoffee", label: "Buy Me a Coffee", address: "buymeacoffee.com/someone" },
      { way: "payPal", label: "PayPal", address: "paypal.me/SomeOne" },
    ]);
    expect(supportLinks({ ...NONE, koFi: "someone" }).map((link) => link.way)).toEqual(["koFi"]);
  });

  it("leaves out an account that isn't a plain name", () => {
    for (const name of ["a/b", "../x", "a b", "a.b", "https://example.com/x", "a?b=c", "a#b", " ", "é", "x".repeat(65)]) {
      expect(supportLinks({ ...NONE, payPal: name }), name).toEqual([]);
    }
  });

  it("has only empty or usable names in app/support.json", () => {
    const set = Object.values(accounts).filter((name) => name !== "");
    expect(supportLinks()).toHaveLength(set.length);
    expect(Object.keys(accounts).sort()).toEqual(Object.keys(NONE).sort());
  });

  it("says the same in .github/FUNDING.yml as in app/support.json", () => {
    const sponsor = funding();
    const one = (name: string) => (name ? [name] : []);
    expect(sponsor.github ?? [], "github").toEqual(one(accounts.githubSponsors));
    expect(sponsor.ko_fi ?? [], "ko_fi").toEqual(one(accounts.koFi));
    expect(sponsor.buy_me_a_coffee ?? [], "buy_me_a_coffee").toEqual(one(accounts.buyMeACoffee));
    expect(sponsor.custom ?? [], "custom (the PayPal.Me link)").toEqual(one(accounts.payPal && `https://paypal.me/${accounts.payPal}`));
    // Nothing else is offered there that the app doesn't list.
    expect(Object.keys(sponsor).sort()).toEqual(["buy_me_a_coffee", "custom", "github", "ko_fi"]);
  });
});
