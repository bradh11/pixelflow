#!/usr/bin/env node
// Checks PixelFlow's dependencies against the public advisory databases.
//
//   node scripts/audit.mjs
//
// Runs `cargo audit` on both Cargo workspaces (the repo root and app/src-tauri) and
// `pnpm audit --prod` on app/, prints every finding, and exits 1 only when a finding should
// stop a merge:
//
//   - it is a vulnerability (not a notice that a package is unmaintained or unsound), and
//   - its severity is High or Critical, or it has no severity score (treated as High), and
//   - a fixed version exists, and
//   - it is not in IGNORED below.
//
// Everything else is printed and left to the weekly run to keep in view. Needs cargo-audit
// (`cargo install cargo-audit --locked`) and pnpm, and network access to the RustSec and npm
// advisory databases.

import { execFileSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

/**
 * Advisories that never fail the check, each with the reason. Remove an entry when its fix
 * lands. `id` is a RUSTSEC, GHSA, or CVE identifier.
 */
export const IGNORED = [
  // { id: "RUSTSEC-0000-0000", reason: "Why this does not affect PixelFlow, and what would fix it." },
];

/**
 * Notices (unmaintained, unsound, yanked) already looked at. They never fail the check; listing
 * them here only keeps a new one from being lost among the known ones.
 */
export const KNOWN_NOTICES = [
  {
    id: "RUSTSEC-2024-0370",
    reason:
      "proc-macro-error is a build-time macro helper of the GTK 3 bindings Tauri uses on Linux. No fix until Tauri moves off gtk-rs 0.18.",
  },
  {
    id: "RUSTSEC-2024-0429",
    reason:
      "glib 0.18's VariantStrIter is unsound. Linux only, through Tauri's GTK 3 bindings; neither PixelFlow nor the GTK crates it links (tao, wry, tauri, muda, gtk, gdk, gio, webkit2gtk, soup3) use that type. Fixed in glib 0.20, which Tauri's Linux stack does not use yet.",
  },
  {
    id: "RUSTSEC-2026-0370",
    reason:
      "zvariant is unmaintained: it is now zbus 6's `wire` module. Linux only, through zbus 5 in keyring's Secret Service store (zbus-secret-service-keyring-store, secret-service), where pf-ai keeps AI provider keys; PixelFlow does not use zvariant itself. Goes away when that store moves to zbus 6: keyring 4.2.0, zbus-secret-service-keyring-store 1.0.1 and secret-service 5.2.0 are the newest and all still use zbus 5.",
  },
  {
    id: "RUSTSEC-2026-0371",
    reason:
      "zbus_names is unmaintained: it is now zbus 6's `names` module. Linux only, through zbus 5 in keyring's Secret Service store (zbus-secret-service-keyring-store, secret-service), where pf-ai keeps AI provider keys; PixelFlow does not use zbus_names itself. Goes away when that store moves to zbus 6: keyring 4.2.0, zbus-secret-service-keyring-store 1.0.1 and secret-service 5.2.0 are the newest and all still use zbus 5.",
  },
  {
    id: "RUSTSEC-2026-0372",
    reason:
      "zvariant_derive is unmaintained: zbus 6 provides its derive macros. A build-time macro of zvariant, Linux only, through zbus 5 in keyring's Secret Service store (zbus-secret-service-keyring-store, secret-service); PixelFlow does not use it. Goes away when that store moves to zbus 6: keyring 4.2.0, zbus-secret-service-keyring-store 1.0.1 and secret-service 5.2.0 are the newest and all still use zbus 5.",
  },
  {
    id: "RUSTSEC-2026-0386",
    reason:
      "tokio's safe AsyncFd constructors (new, with_interest, try_new, try_with_interest) are unsound. tokio is Tauri's async runtime in the desktop app, and in the engine's lock file only for WebAssembly (wasm-bindgen-futures, through cpal). Nothing calls AsyncFd: not PixelFlow, and no crate in either lock file but tokio itself. Fixed in tokio 1.53.3 (and 1.54); `cargo update -p tokio` takes it.",
  },
];

const CVSS3 = {
  AV: { N: 0.85, A: 0.62, L: 0.55, P: 0.2 },
  AC: { L: 0.77, H: 0.44 },
  UI: { N: 0.85, R: 0.62 },
  CIA: { H: 0.56, L: 0.22, N: 0 },
  PR: { unchanged: { N: 0.85, L: 0.62, H: 0.27 }, changed: { N: 0.85, L: 0.68, H: 0.5 } },
};

/** The base score of a CVSS 3.0 or 3.1 vector; `null` for anything else (CVSS 4, no vector). */
export function cvss3Score(vector) {
  if (typeof vector !== "string" || !/^CVSS:3\.[01]\//.test(vector)) return null;
  const m = Object.fromEntries(
    vector
      .split("/")
      .slice(1)
      .map((part) => part.split(":")),
  );
  const changed = m.S === "C";
  const av = CVSS3.AV[m.AV];
  const ac = CVSS3.AC[m.AC];
  const ui = CVSS3.UI[m.UI];
  const pr = CVSS3.PR[changed ? "changed" : "unchanged"][m.PR];
  const [c, i, a] = [m.C, m.I, m.A].map((v) => CVSS3.CIA[v]);
  if ([av, ac, ui, pr, c, i, a].some((v) => v === undefined) || !["U", "C"].includes(m.S)) return null;
  const iss = 1 - (1 - c) * (1 - i) * (1 - a);
  const impact = changed ? 7.52 * (iss - 0.029) - 3.25 * (iss - 0.02) ** 15 : 6.42 * iss;
  if (impact <= 0) return 0;
  const exploitability = 8.22 * av * ac * pr * ui;
  const raw = changed ? Math.min(1.08 * (impact + exploitability), 10) : Math.min(impact + exploitability, 10);
  // "Round up" as the CVSS 3.1 specification defines it.
  const scaled = Math.round(raw * 100000);
  return scaled % 10000 === 0 ? scaled / 100000 : (Math.floor(scaled / 10000) + 1) / 10;
}

export function severityOfScore(score) {
  if (score === null) return "unknown";
  if (score >= 9) return "critical";
  if (score >= 7) return "high";
  if (score >= 4) return "moderate";
  return score > 0 ? "low" : "none";
}

/** The findings in one `cargo audit --json` report. */
export function cargoFindings(report, scope) {
  const findings = [];
  for (const v of report.vulnerabilities?.list ?? []) {
    findings.push({
      scope,
      kind: "vulnerability",
      ids: [v.advisory.id, ...(v.advisory.aliases ?? [])],
      package: `${v.package.name} ${v.package.version}`,
      title: v.advisory.title,
      severity: severityOfScore(cvss3Score(v.advisory.cvss)),
      fixed: (v.versions?.patched ?? []).join(", "),
      url: `https://rustsec.org/advisories/${v.advisory.id}`,
    });
  }
  for (const [kind, warnings] of Object.entries(report.warnings ?? {})) {
    for (const w of warnings) {
      findings.push({
        scope,
        kind,
        ids: w.advisory ? [w.advisory.id, ...(w.advisory.aliases ?? [])] : [],
        package: `${w.package.name} ${w.package.version}`,
        title: w.advisory?.title ?? `${w.package.name} ${w.package.version} is ${kind}`,
        severity: "notice",
        fixed: (w.versions?.patched ?? []).join(", "),
        url: w.advisory ? `https://rustsec.org/advisories/${w.advisory.id}` : "",
      });
    }
  }
  return findings;
}

/** The findings in one `pnpm audit --json` report. */
export function npmFindings(report, scope) {
  return Object.values(report.advisories ?? {}).map((a) => {
    const patched = a.patched_versions ?? "";
    return {
      scope,
      kind: "vulnerability",
      ids: [a.github_advisory_id, ...(a.cves ?? [])].filter(Boolean),
      package: `${a.module_name} ${[...new Set((a.findings ?? []).map((f) => f.version))].join(", ")}`.trim(),
      title: a.title,
      severity: ["critical", "high", "moderate", "low"].includes(a.severity) ? a.severity : "unknown",
      // npm writes "<0.0.0" when no version fixes it.
      fixed: patched === "<0.0.0" ? "" : patched,
      url: a.url ?? "",
    };
  });
}

/**
 * Whether a finding fails the check, and why or why not:
 * `{ blocks: boolean, why: string }`.
 */
export function judge(finding, ignored = IGNORED, known = KNOWN_NOTICES) {
  const listed = (list) => list.find((entry) => finding.ids.includes(entry.id));
  if (finding.kind !== "vulnerability") {
    const entry = listed(known);
    return { blocks: false, why: entry ? `known notice: ${entry.reason}` : `new notice (${finding.kind}): look at it` };
  }
  const entry = listed(ignored);
  if (entry) return { blocks: false, why: `ignored: ${entry.reason}` };
  if (!["high", "critical", "unknown"].includes(finding.severity)) {
    return { blocks: false, why: `${finding.severity} severity` };
  }
  if (!finding.fixed) return { blocks: false, why: "no fixed version exists yet" };
  const scored = finding.severity === "unknown" ? "no severity score (treated as high)" : `${finding.severity} severity`;
  return { blocks: true, why: `${scored}, fixed in ${finding.fixed}` };
}

/** Runs a tool that reports findings as JSON on stdout (and exits non-zero when it has any). */
function jsonFrom(command, args, cwd) {
  let out;
  try {
    out = execFileSync(command, args, {
      cwd,
      maxBuffer: 256 * 1024 * 1024,
      stdio: ["ignore", "pipe", "inherit"],
      shell: process.platform === "win32",
    });
  } catch (error) {
    if (!error.stdout || error.stdout.length === 0) {
      throw new Error(`${command} ${args.join(" ")} did not run: ${error.message}`);
    }
    out = error.stdout;
  }
  return JSON.parse(out.toString("utf8"));
}

function main() {
  const findings = [
    ...cargoFindings(jsonFrom("cargo-audit", ["audit", "--json"], ROOT), "Cargo.lock"),
    ...cargoFindings(
      jsonFrom("cargo-audit", ["audit", "--json"], path.join(ROOT, "app", "src-tauri")),
      "app/src-tauri/Cargo.lock",
    ),
    ...npmFindings(jsonFrom("pnpm", ["audit", "--prod", "--json"], path.join(ROOT, "app")), "app/pnpm-lock.yaml (production)"),
  ];
  const annotate = process.env.GITHUB_ACTIONS === "true";
  let blocking = 0;
  for (const f of findings) {
    const verdict = judge(f);
    if (verdict.blocks) blocking += 1;
    const line = `${f.scope}: ${f.package}: ${f.title} [${f.ids.join(", ") || f.kind}] ${f.url}\n    ${verdict.blocks ? "FAILS THE CHECK" : "does not fail the check"}: ${verdict.why}`;
    console.log(line);
    if (annotate && (verdict.blocks || verdict.why.startsWith("new notice"))) {
      console.log(`::${verdict.blocks ? "error" : "warning"}::${f.package}: ${f.title} (${f.ids[0] ?? f.kind}): ${verdict.why}`);
    }
  }
  const vulnerabilities = findings.filter((f) => f.kind === "vulnerability").length;
  console.log(
    `\n${vulnerabilities} vulnerabilit${vulnerabilities === 1 ? "y" : "ies"}, ${findings.length - vulnerabilities} notice(s); ${blocking} fail the check.`,
  );
  process.exit(blocking > 0 ? 1 : 0);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) main();
