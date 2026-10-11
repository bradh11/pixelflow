// Tests for the audit policy: node --test scripts/audit.test.mjs

import assert from "node:assert/strict";
import { test } from "node:test";
import { cargoFindings, cvss3Score, judge, npmFindings, severityOfScore } from "./audit.mjs";

test("CVSS 3 vectors score as the specification's examples do", () => {
  assert.equal(cvss3Score("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H"), 9.8);
  assert.equal(cvss3Score("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:C/C:H/I:H/A:H"), 10);
  assert.equal(cvss3Score("CVSS:3.1/AV:L/AC:L/PR:L/UI:N/S:U/C:H/I:H/A:H"), 7.8);
  assert.equal(cvss3Score("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:N/I:N/A:H"), 7.5);
  assert.equal(cvss3Score("CVSS:3.1/AV:N/AC:H/PR:N/UI:N/S:U/C:H/I:N/A:N"), 5.9);
  assert.equal(cvss3Score("CVSS:3.0/AV:N/AC:L/PR:L/UI:R/S:C/C:L/I:L/A:N"), 5.4);
  assert.equal(cvss3Score("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:N/I:N/A:N"), 0);
});

test("a vector that is not CVSS 3 has no score, and no score is unknown severity", () => {
  assert.equal(cvss3Score(null), null);
  assert.equal(cvss3Score("CVSS:4.0/AV:N/AC:L/AT:N/PR:N/UI:N/VC:H/VI:H/VA:H/SC:N/SI:N/SA:N"), null);
  assert.equal(cvss3Score("CVSS:3.1/AV:X/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H"), null);
  assert.equal(severityOfScore(null), "unknown");
  assert.equal(severityOfScore(9.8), "critical");
  assert.equal(severityOfScore(7), "high");
  assert.equal(severityOfScore(6.9), "moderate");
  assert.equal(severityOfScore(3.9), "low");
});

const vulnerability = (over = {}) => ({
  scope: "Cargo.lock",
  kind: "vulnerability",
  ids: ["RUSTSEC-2099-0001", "GHSA-aaaa-bbbb-cccc"],
  package: "example 1.0.0",
  title: "Example",
  severity: "high",
  fixed: ">=1.0.1",
  url: "",
  ...over,
});

test("only high or critical vulnerabilities with a fix fail the check", () => {
  assert.equal(judge(vulnerability(), [], []).blocks, true);
  assert.equal(judge(vulnerability({ severity: "critical" }), [], []).blocks, true);
  assert.equal(judge(vulnerability({ severity: "moderate" }), [], []).blocks, false);
  assert.equal(judge(vulnerability({ severity: "low" }), [], []).blocks, false);
  assert.equal(judge(vulnerability({ fixed: "" }), [], []).blocks, false);
});

test("a vulnerability with no severity score is treated as high", () => {
  const verdict = judge(vulnerability({ severity: "unknown" }), [], []);
  assert.equal(verdict.blocks, true);
  assert.match(verdict.why, /treated as high/);
  assert.equal(judge(vulnerability({ severity: "unknown", fixed: "" }), [], []).blocks, false);
});

test("an ignored advisory never fails the check, by any of its identifiers", () => {
  const ignored = [{ id: "GHSA-aaaa-bbbb-cccc", reason: "not reachable" }];
  const verdict = judge(vulnerability({ severity: "critical" }), ignored, []);
  assert.equal(verdict.blocks, false);
  assert.match(verdict.why, /not reachable/);
});

test("notices never fail the check, and a new one is called out", () => {
  const notice = vulnerability({ kind: "unsound", severity: "notice", ids: ["RUSTSEC-2099-0002"] });
  assert.match(judge(notice, [], []).why, /new notice/);
  assert.equal(judge(notice, [], []).blocks, false);
  assert.match(judge(notice, [], [{ id: "RUSTSEC-2099-0002", reason: "seen" }]).why, /known notice: seen/);
});

test("cargo audit reports are read: vulnerabilities and warnings", () => {
  const report = {
    vulnerabilities: {
      list: [
        {
          advisory: {
            id: "RUSTSEC-2099-0001",
            aliases: ["CVE-2099-1"],
            title: "Remote crash",
            cvss: "CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:N/I:N/A:H",
            url: "https://example.test/a",
          },
          package: { name: "example", version: "1.0.0" },
          versions: { patched: [">=1.0.1"], unaffected: [] },
        },
        {
          advisory: { id: "RUSTSEC-2099-0003", aliases: [], title: "No score", cvss: null },
          package: { name: "other", version: "2.0.0" },
          versions: { patched: [], unaffected: [] },
        },
      ],
    },
    warnings: {
      unmaintained: [
        {
          kind: "unmaintained",
          advisory: { id: "RUSTSEC-2099-0002", aliases: [], title: "Unmaintained" },
          package: { name: "old", version: "0.1.0" },
          versions: { patched: [], unaffected: [] },
        },
      ],
      yanked: [{ kind: "yanked", advisory: null, package: { name: "gone", version: "0.2.0" }, versions: null }],
    },
  };
  const findings = cargoFindings(report, "Cargo.lock");
  assert.deepEqual(
    findings.map((f) => [f.kind, f.package, f.severity, f.fixed]),
    [
      ["vulnerability", "example 1.0.0", "high", ">=1.0.1"],
      ["vulnerability", "other 2.0.0", "unknown", ""],
      ["unmaintained", "old 0.1.0", "notice", ""],
      ["yanked", "gone 0.2.0", "notice", ""],
    ],
  );
  assert.deepEqual(
    findings.map((f) => judge(f, [], []).blocks),
    [true, false, false, false],
  );
  assert.deepEqual(cargoFindings({ vulnerabilities: { list: [] }, warnings: {} }, "x"), []);
});

test("pnpm audit reports are read: severity, and whether a fix exists", () => {
  const report = {
    advisories: {
      1: {
        github_advisory_id: "GHSA-1111-2222-3333",
        cves: ["CVE-2099-2"],
        module_name: "left",
        title: "Prototype pollution",
        severity: "high",
        patched_versions: ">=2.0.0",
        findings: [{ version: "1.0.0" }, { version: "1.0.0" }],
        url: "https://example.test/b",
      },
      2: {
        github_advisory_id: "GHSA-4444-5555-6666",
        module_name: "right",
        title: "No fix",
        severity: "critical",
        patched_versions: "<0.0.0",
        findings: [{ version: "3.0.0" }],
      },
    },
  };
  const findings = npmFindings(report, "app");
  assert.deepEqual(
    findings.map((f) => [f.package, f.severity, f.fixed]),
    [
      ["left 1.0.0", "high", ">=2.0.0"],
      ["right 3.0.0", "critical", ""],
    ],
  );
  assert.deepEqual(
    findings.map((f) => judge(f, [], []).blocks),
    [true, false],
  );
  assert.deepEqual(npmFindings({ advisories: {} }, "app"), []);
});
