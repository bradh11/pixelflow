#!/usr/bin/env node
// Dependency licence inventory and third-party notices for PixelFlow.
//
//   node scripts/licenses.mjs            write the two generated files
//   node scripts/licenses.mjs --check    change nothing; exit 1 if either file is out of date
//
// Writes:
//   docs/licensing/dependencies.md                       the inventory, between the GENERATED markers
//   app/src-tauri/resources/THIRD-PARTY-NOTICES.txt      the notices the built app bundles
//
// Reads `cargo metadata` for both Cargo workspaces (the engine at the repo root and the desktop
// app in app/src-tauri) and the installed packages under app/node_modules, so run `pnpm install`
// in app/ first. Needs only Node and cargo: no other tools, and no network beyond what
// `cargo metadata` uses to fill the local registry the first time.
//
// This reports what each package's own manifest and licence files say. It is an inventory, not
// legal advice.

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const INVENTORY = "docs/licensing/dependencies.md";
const NOTICES = "app/src-tauri/resources/THIRD-PARTY-NOTICES.txt";
const STANDARD_TEXTS = "scripts/license-texts";
const BEGIN = "<!-- BEGIN GENERATED (scripts/licenses.mjs): do not edit below this line by hand -->";
const END = "<!-- END GENERATED -->";

const WORKSPACES = [
  { key: "engine", manifest: "Cargo.toml" },
  { key: "app", manifest: "app/src-tauri/Cargo.toml" },
];

// The desktop targets an installer would be built for. A package is "shipped" on a platform when
// it is linked into the binary for either architecture.
const PLATFORMS = [
  ["macOS", ["aarch64-apple-darwin", "x86_64-apple-darwin"]],
  ["Windows", ["x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc"]],
  ["Linux", ["x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"]],
];

// Licence families that carry source-sharing (copyleft) conditions.
const COPYLEFT = /^(AGPL|GPL|LGPL|MPL|EPL|EUPL|CDDL|OSL|CECILL|CC-BY-SA|SSPL|RPL|CPAL|QPL)/;
// Permissive licences seen often enough to need no comment.
const COMMON = new Set([
  "MIT",
  "MIT-0",
  "Apache-2.0",
  "BSD-2-Clause",
  "BSD-3-Clause",
  "ISC",
  "Zlib",
  "0BSD",
  "Unlicense",
  "CC0-1.0",
  "BSL-1.0",
]);
// Permissive, but with terms of their own worth reading once (data licences, exceptions).
const SPECIAL = new Set(["Unicode-3.0", "Unicode-DFS-2016", "CDLA-Permissive-2.0", "BlueOak-1.0.0", "OpenSSL"]);

// Data built into the app that is not a package (so no manifest lists it).
const EMBEDDED = [
  {
    name: "CMU Pronouncing Dictionary (cmudict)",
    license: "BSD-2-Clause",
    url: "https://github.com/cmusphinx/cmudict",
    where: "compressed into the app from crates/pf-lexicon/data/cmudict.zst",
    file: "crates/pf-lexicon/data/cmudict-LICENSE.txt",
  },
];

// Fetched by the app when the user asks for it; not part of the installer.
const DOWNLOADED = [
  {
    name: "wav2vec2-base-960h (speech model, ONNX conversion by the ONNX Community)",
    license: "Apache-2.0",
    url: "https://huggingface.co/onnx-community/wav2vec2-base-960h-ONNX",
    where: "downloaded from Hugging Face when the user turns on lyric alignment; not in the installer",
  },
];

// Notes printed beside a licence file in the notices, where the file alone could mislead.
const FILE_NOTES = {
  "zstd-sys:zstd/COPYING":
    "The zstd library is offered under either the BSD licence (zstd/LICENSE, above or below) or GPL-2.0 (this file), at the recipient's choice.",
  "zstd-sys:zstd/LICENSE":
    "The zstd library is offered under either this BSD licence or GPL-2.0 (zstd/COPYING), at the recipient's choice.",
  "libdbus-sys:vendor/dbus/COPYING":
    "Applies to the copy of libdbus in the package's source. PixelFlow's build links the system's libdbus instead (the `vendored` feature is off).",
  "libdbus-sys:vendor/dbus/cmake/modules/COPYING-CMAKE-SCRIPTS":
    "Applies to build scripts in the package's source, which are not part of the app.",
};

const read = (file) => fs.readFileSync(file, "utf8");
const readJson = (file) => JSON.parse(read(file));
const rel = (...parts) => path.join(ROOT, ...parts);
const byName = (a, b) =>
  a.name.toLowerCase() < b.name.toLowerCase()
    ? -1
    : a.name.toLowerCase() > b.name.toLowerCase()
      ? 1
      : a.version < b.version
        ? -1
        : a.version > b.version
          ? 1
          : 0;

// ---------------------------------------------------------------------------------------------
// Licence expressions

/** `MIT/Apache-2.0` and friends, written the SPDX way. */
function normalizeExpression(text) {
  if (!text) return "";
  return text
    .replace(/\s*\/\s*/g, " OR ")
    .replace(/\s+/g, " ")
    .trim();
}

function licenceIds(expression) {
  return expression
    .replace(/[()]/g, " ")
    .split(/\s+(?:OR|AND|WITH)\s+|\s+/)
    .filter((id) => id && !/^(OR|AND|WITH)$/.test(id));
}

/** A plain list of alternatives (`A OR B OR C`): its options, else `null`. */
function alternatives(expression) {
  if (/[()]|\bAND\b/.test(expression)) return null;
  return expression.split(/\s+OR\s+/).map((part) => part.trim());
}

/** The same expression for `Apache-2.0 OR MIT` and `MIT OR Apache-2.0`, to group by. */
function groupKey(expression) {
  const options = alternatives(expression);
  return options ? [...options].sort().join(" OR ") : expression;
}

/**
 * What a licence expression needs said about it:
 * `copyleft` (a copyleft licence applies), `choice` (copyleft is one option, a permissive one
 * exists), `special` (permissive with terms of its own), `unknown` (missing or unrecognized),
 * else `null`.
 */
function flagOf(expression) {
  if (!expression) return "unknown";
  const options = alternatives(expression) ?? [expression];
  const copyleftOption = (option) => licenceIds(option).some((id) => COPYLEFT.test(id));
  if (options.every(copyleftOption)) return "copyleft";
  if (options.some(copyleftOption)) return "choice";
  const ids = licenceIds(expression).filter((id) => id !== "LLVM-exception");
  if (ids.some((id) => !COMMON.has(id) && !SPECIAL.has(id))) return "unknown";
  // A special licence matters only when no common one is offered instead.
  const plain = (option) => licenceIds(option).every((id) => COMMON.has(id) || id === "LLVM-exception");
  if (!options.some(plain)) return "special";
  return null;
}

// ---------------------------------------------------------------------------------------------
// Licence files

const LICENCE_FILE = /^(licen[sc]e|copying|copyright|notice|unlicense|patents)/i;
const NOT_TEXT = /\.(rs|c|h|cc|cpp|py|js|mjs|ts|toml|json|ya?ml|sh|cmake|html|svg|png)$/i;
const SKIP_DIRS = new Set(["tests", "test", "benches", "examples", "docs", "doc", "target", "node_modules", ".git", ".github"]);
const MAX_LICENCE_BYTES = 256 * 1024;

/** A package's licence files: those at its top level, and (for vendored code) a few levels down. */
function licenceFiles(dir, depth = 3, prefix = "") {
  let entries;
  try {
    entries = fs.readdirSync(dir, { withFileTypes: true });
  } catch {
    return [];
  }
  const found = [];
  for (const entry of entries.sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0))) {
    const full = path.join(dir, entry.name);
    const name = prefix + entry.name;
    if (entry.isDirectory()) {
      if (depth > 0 && !SKIP_DIRS.has(entry.name)) found.push(...licenceFiles(full, depth - 1, `${name}/`));
    } else if (entry.isFile() && LICENCE_FILE.test(entry.name) && !NOT_TEXT.test(entry.name)) {
      if (fs.statSync(full).size <= MAX_LICENCE_BYTES) found.push({ file: name, text: cleanText(read(full)) });
    }
  }
  return found;
}

function cleanText(text) {
  return text
    .replace(/^﻿/, "")
    .replace(/\r\n?/g, "\n")
    .split("\n")
    .map((line) => line.replace(/\s+$/, ""))
    .join("\n")
    .replace(/^\n+|\n+$/g, "");
}

// ---------------------------------------------------------------------------------------------
// Cargo

function cargoMetadata(manifest, triple) {
  const args = ["metadata", "--format-version", "1", "--locked", "--manifest-path", rel(manifest)];
  if (triple) args.push("--filter-platform", triple);
  const out = execFileSync("cargo", args, {
    cwd: ROOT,
    maxBuffer: 512 * 1024 * 1024,
    stdio: ["ignore", "pipe", "inherit"],
  });
  return JSON.parse(out);
}

const RANK = { shipped: 0, build: 1, dev: 2 };

/**
 * How each package in a resolved graph is used: `shipped` (linked into the binaries), `build`
 * (build scripts and procedural macros: runs while compiling, not in the binary), or `dev`
 * (tests and benchmarks only).
 */
function classify(meta) {
  const packages = new Map(meta.packages.map((p) => [p.id, p]));
  const nodes = new Map(meta.resolve.nodes.map((n) => [n.id, n]));
  const members = new Set(meta.workspace_members);
  const procMacro = (id) => packages.get(id).targets.some((t) => t.kind.includes("proc-macro"));
  const use = new Map();
  const queue = [];
  const visit = (id, how) => {
    const now = use.get(id);
    if (now === undefined || RANK[how] < RANK[now]) {
      use.set(id, how);
      queue.push(id);
    }
  };
  for (const id of members) visit(id, "shipped");
  while (queue.length > 0) {
    const id = queue.shift();
    const how = use.get(id);
    for (const dep of nodes.get(id)?.deps ?? []) {
      for (const kind of dep.dep_kinds) {
        let next = how;
        if (kind.kind === "dev") {
          if (!members.has(id)) continue;
          next = "dev";
        } else if (kind.kind === "build" && how === "shipped") {
          next = "build";
        }
        if (next === "shipped" && procMacro(dep.pkg)) next = "build";
        visit(dep.pkg, next);
      }
    }
  }
  return use;
}

function cargoWorkspace(workspace) {
  const full = cargoMetadata(workspace.manifest);
  const packages = new Map();
  for (const p of full.packages) {
    if (p.source === null) continue; // PixelFlow's own crates
    packages.set(p.id, {
      id: p.id,
      name: p.name,
      version: p.version,
      license: normalizeExpression(p.license),
      licenseFile: p.license_file,
      repository: p.repository ?? p.homepage ?? "",
      authors: p.authors ?? [],
      dir: path.dirname(p.manifest_path),
      use: null,
      platforms: [],
    });
  }
  for (const [platform, triples] of PLATFORMS) {
    for (const triple of triples) {
      for (const [id, how] of classify(cargoMetadata(workspace.manifest, triple))) {
        const p = packages.get(id);
        if (!p) continue;
        if (p.use === null || RANK[how] < RANK[p.use]) p.use = how;
        if (how === "shipped" && !p.platforms.includes(platform)) p.platforms.push(platform);
      }
    }
  }
  const own = full.packages.filter((p) => p.source === null);
  return {
    key: workspace.key,
    manifest: workspace.manifest,
    packages: [...packages.values()].sort(byName),
    own: own.map((p) => ({ name: p.name, license: p.license, licenseFile: p.license_file })).sort(byName),
  };
}

// ---------------------------------------------------------------------------------------------
// npm (the installed production dependencies of app/)

function npmProduction() {
  const app = rel("app");
  const manifest = readJson(path.join(app, "package.json"));
  if (!fs.existsSync(path.join(app, "node_modules"))) {
    throw new Error("app/node_modules is missing: run `pnpm install` in app/ first.");
  }
  const resolve = (from, name) => {
    for (let dir = from; ; dir = path.dirname(dir)) {
      const candidate = path.join(dir, "node_modules", name);
      if (fs.existsSync(path.join(candidate, "package.json"))) return fs.realpathSync(candidate);
      if (dir === app || path.dirname(dir) === dir) return null;
    }
  };
  const seen = new Map();
  const walk = (from, name, required, direct) => {
    const dir = resolve(from, name);
    if (!dir) {
      if (required) throw new Error(`${name} (needed by ${path.relative(app, from) || "app"}) is not installed.`);
      return;
    }
    const known = seen.get(dir);
    if (known) {
      known.direct ||= direct;
      return;
    }
    const pkg = readJson(path.join(dir, "package.json"));
    const license =
      typeof pkg.license === "string"
        ? pkg.license
        : (pkg.license?.type ?? (pkg.licenses ?? []).map((l) => l.type ?? l).join(" OR "));
    const repository = typeof pkg.repository === "string" ? pkg.repository : (pkg.repository?.url ?? pkg.homepage ?? "");
    seen.set(dir, {
      name: pkg.name,
      version: pkg.version,
      license: normalizeExpression(license),
      repository: repository.replace(/^git\+/, "").replace(/\.git$/, ""),
      authors: [typeof pkg.author === "string" ? pkg.author : pkg.author?.name].filter(Boolean),
      dir,
      direct,
      use: "shipped",
    });
    for (const dep of Object.keys(pkg.dependencies ?? {})) walk(dir, dep, true, false);
    for (const dep of Object.keys(pkg.optionalDependencies ?? {})) walk(dir, dep, false, false);
    // A required peer is part of the bundle (React for a React component library). Optional
    // peers are mostly type declarations, which are not shipped.
    for (const dep of Object.keys(pkg.peerDependencies ?? {})) {
      if (!pkg.peerDependenciesMeta?.[dep]?.optional) walk(dir, dep, false, false);
    }
  };
  for (const name of Object.keys(manifest.dependencies ?? {})) walk(app, name, true, true);
  return [...seen.values()].sort(byName);
}

// ---------------------------------------------------------------------------------------------
// The inventory (Markdown)

const FLAG_TEXT = {
  copyleft: "copyleft",
  choice: "copyleft is one option",
  special: "own terms",
  unknown: "unknown or missing",
};

function table(headers, rows) {
  const line = (cells) => `| ${cells.join(" | ")} |`;
  return [line(headers), line(headers.map(() => "---")), ...rows.map(line)].join("\n");
}

function summaryRows(packages) {
  const groups = new Map();
  for (const p of packages) {
    const key = groupKey(p.license) || "(none stated)";
    const group = groups.get(key) ?? { key, count: 0, flag: flagOf(p.license) };
    group.count += 1;
    groups.set(key, group);
  }
  return [...groups.values()]
    .sort((a, b) => b.count - a.count || (a.key < b.key ? -1 : 1))
    .map((g) => [`\`${g.key}\``, String(g.count), g.flag ? FLAG_TEXT[g.flag] : ""]);
}

function flagged(packages, flag) {
  return packages.filter((p) => flagOf(p.license) === flag);
}

function flagList(title, packages, describe) {
  if (packages.length === 0) return `**${title}:** none.`;
  return `**${title}** (${packages.length}):\n\n${packages.map((p) => `- ${describe(p)}`).join("\n")}`;
}

function inventory(engine, app, npm) {
  const shipped = (ws) => ws.packages.filter((p) => p.use === "shipped");
  const count = (ws, how) => ws.packages.filter((p) => p.use === how).length;
  const unused = (ws) => ws.packages.filter((p) => p.use === null).length;
  const where = (p) => (p.platforms.length === PLATFORMS.length ? "all" : p.platforms.join(", "));
  const describeCargo = (p) =>
    `\`${p.name}\` ${p.version}: \`${p.license || "none stated"}\` (${p.use === "shipped" ? `shipped: ${where(p)}` : (p.use ?? "not used on desktop targets")})`;
  const out = [];

  out.push("## Counts");
  out.push(
    table(
      ["Scope", "Third-party packages", "Shipped", "Build-time only", "Tests only", "Not used on desktop targets"],
      [
        ...[engine, app].map((ws) => [
          ws.key === "engine" ? "(a) Engine workspace (`Cargo.toml`)" : "(b) Desktop app (`app/src-tauri/Cargo.toml`)",
          String(ws.packages.length),
          String(count(ws, "shipped")),
          String(count(ws, "build")),
          String(count(ws, "dev")),
          String(unused(ws)),
        ]),
        ["(c) App UI, production (`app/package.json`)", String(npm.length), String(npm.length), "0", "0", "0"],
      ],
    ),
  );
  out.push(
    "\"Shipped\" means linked into a binary for macOS, Windows, or Linux. \"Build-time only\" covers build scripts and procedural macros, which run while compiling and are not in the binary. \"Not used on desktop targets\" are packages the lockfile lists for other platforms (Android, iOS, WebAssembly).",
  );

  out.push("## PixelFlow's own packages");
  const own = [...engine.own, ...app.own.filter((p) => !engine.own.some((e) => e.name === p.name))];
  const stated = own.filter((p) => p.license || p.licenseFile);
  out.push(
    stated.length === 0
      ? `None of PixelFlow's ${own.length} own packages states a licence (no \`license\` or \`license-file\` in any manifest).`
      : table(
          ["Package", "Licence stated"],
          own.map((p) => [`\`${p.name}\``, p.license || p.licenseFile || "none"]),
        ),
  );

  for (const [title, packages] of [
    ["(a) Engine workspace: shipped packages by licence", shipped(engine)],
    ["(b) Desktop app: shipped packages by licence", shipped(app)],
    ["(c) App UI: production packages by licence", npm],
  ]) {
    out.push(`## ${title}`);
    out.push(table(["Licence (as the package states it)", "Packages", "Flag"], summaryRows(packages)));
  }

  out.push("## Flags");
  out.push(
    "Across both Cargo workspaces (every use, not only shipped) and the app UI's production packages.",
  );
  const cargo = new Map();
  for (const ws of [engine, app]) {
    for (const p of ws.packages) {
      const known = cargo.get(p.id);
      if (!known || RANK[p.use ?? "dev"] < RANK[known.use ?? "dev"] || p.platforms.length > known.platforms.length) {
        cargo.set(p.id, p);
      }
    }
  }
  const everyCargo = [...cargo.values()].sort(byName);
  const describeNpm = (p) => `\`${p.name}\` ${p.version} (npm): \`${p.license || "none stated"}\``;
  for (const [flag, title] of [
    ["copyleft", "Copyleft licence applies"],
    ["choice", "Copyleft is one of several options (a permissive option is offered)"],
    ["special", "Permissive, with terms of their own"],
    ["unknown", "Unknown, missing, or unrecognized licence"],
  ]) {
    const items = [
      ...flagged(everyCargo, flag).map(describeCargo),
      ...flagged(npm, flag).map(describeNpm),
    ];
    out.push(items.length === 0 ? `**${title}:** none.` : `**${title}** (${items.length}):\n\n${items.map((i) => `- ${i}`).join("\n")}`);
  }
  const noFile = [...shipped(app), ...npm].filter((p) => licenceFiles(p.dir).length === 0);
  out.push(
    flagList(
      "Shipped in the app with no licence file in the package (the notices use the standard text of the stated licence)",
      noFile,
      (p) => `\`${p.name}\` ${p.version}: \`${p.license}\``,
    ),
  );

  out.push("## Every Cargo package");
  const useIn = (ws, id) => {
    const p = ws.packages.find((x) => x.id === id);
    if (!p) return "";
    if (p.use === "shipped") return `shipped (${where(p)})`;
    return p.use === "build" ? "build" : p.use === "dev" ? "tests" : "not on desktop";
  };
  out.push(
    table(
      ["Package", "Version", "Licence", "(a) Engine", "(b) App"],
      everyCargo.map((p) => [`\`${p.name}\``, p.version, `\`${p.license || "none stated"}\``, useIn(engine, p.id), useIn(app, p.id)]),
    ),
  );

  out.push("## Every production npm package");
  out.push(
    table(
      ["Package", "Version", "Licence", "Listed in `app/package.json`"],
      npm.map((p) => [`\`${p.name}\``, p.version, `\`${p.license || "none stated"}\``, p.direct ? "yes" : ""]),
    ),
  );
  return out.join("\n\n");
}

// ---------------------------------------------------------------------------------------------
// The notices (plain text)

const RULE = "=".repeat(80);
const THIN = "-".repeat(80);

function wrap(text, width = 80, indent = "") {
  const lines = [];
  let line = indent;
  for (const word of text.split(/\s+/)) {
    if (line.length + word.length + 1 > width && line.trim().length > 0) {
      lines.push(line.trimEnd());
      line = indent;
    }
    line += (line.trim().length > 0 ? " " : "") + word;
  }
  if (line.trim().length > 0) lines.push(line.trimEnd());
  return lines.join("\n");
}

function standardText(id) {
  const file = rel(STANDARD_TEXTS, `${id}.txt`);
  return fs.existsSync(file) ? cleanText(read(file)) : null;
}

function notices(app, npm) {
  const components = [
    ...app.packages.filter((p) => p.use === "shipped").map((p) => ({ ...p, kind: "Rust" })),
    ...npm.map((p) => ({ ...p, kind: "JavaScript" })),
  ].sort(byName);
  const problems = [];
  const texts = new Map(); // hash -> { text, users }
  const standard = new Map(); // licence id -> names of the components that rely on it
  for (const c of components) {
    const files = licenceFiles(c.dir);
    c.hasFiles = files.length > 0;
    for (const { file, text } of files) {
      const hash = createHash("sha256").update(text).digest("hex");
      const entry = texts.get(hash) ?? { text, users: [] };
      entry.users.push({ label: `${c.name} ${c.version}`, file, note: FILE_NOTES[`${c.name}:${file}`] });
      texts.set(hash, entry);
    }
    if (!c.hasFiles) {
      const ids = licenceIds(c.license).filter((id) => id !== "LLVM-exception" && standardText(id) !== null);
      if (ids.length === 0) problems.push(`${c.name} ${c.version} (${c.license || "no licence stated"}) has no licence file and no standard text in ${STANDARD_TEXTS}/`);
      for (const id of ids) standard.set(id, [...(standard.get(id) ?? []), `${c.name} ${c.version}`]);
    }
  }

  const out = [];
  out.push("PixelFlow: third-party notices");
  out.push(RULE);
  out.push(
    wrap(
      "PixelFlow includes the third-party software and data listed below. Each is the work of its own authors and is provided under its own licence. This file lists them (part 1) and reproduces their licence texts (parts 2 and 3).",
    ),
  );
  out.push(
    wrap(
      "This file is generated from the app's lockfiles by scripts/licenses.mjs. Do not edit it by hand.",
    ),
  );

  out.push(`${RULE}\nPart 1. What is included\n${RULE}`);
  out.push("Data built into the app:");
  out.push(
    EMBEDDED.map((e) => `  ${e.name}\n    licence: ${e.license}\n    source:  ${e.url}\n    ${wrap(e.where, 80, "    ").trim()}`).join("\n\n"),
  );
  out.push("Fetched only when you ask for it (not part of the installer):");
  out.push(
    DOWNLOADED.map((e) => `  ${e.name}\n    licence: ${e.license}\n    source:  ${e.url}\n    ${wrap(e.where, 80, "    ").trim()}`).join("\n\n"),
  );
  out.push(
    wrap(
      `Software components (${components.length}). Those marked with * ship no licence file of their own; the standard text of their licence is in part 3.`,
    ),
  );
  out.push(
    components
      .map((c) => {
        const lines = [`  ${c.name} ${c.version}${c.hasFiles ? "" : " *"} (${c.kind})`, `    licence: ${c.license || "none stated"}`];
        if (c.authors.length > 0 && !c.hasFiles) lines.push(`    authors: ${c.authors.join(", ")}`);
        if (c.repository) lines.push(`    source:  ${c.repository}`);
        return lines.join("\n");
      })
      .join("\n"),
  );

  out.push(`${RULE}\nPart 2. Licence texts shipped with the components\n${RULE}`);
  for (const e of EMBEDDED) {
    out.push(`${e.name}\n${THIN}\n${cleanText(read(rel(e.file)))}`);
  }
  const groups = [...texts.values()].map((g) => {
    g.users.sort((a, b) => (a.label.toLowerCase() < b.label.toLowerCase() ? -1 : a.label.toLowerCase() > b.label.toLowerCase() ? 1 : a.file < b.file ? -1 : 1));
    return g;
  });
  groups.sort((a, b) => {
    const x = `${a.users[0].label.toLowerCase()} ${a.users[0].file}`;
    const y = `${b.users[0].label.toLowerCase()} ${b.users[0].file}`;
    return x < y ? -1 : x > y ? 1 : 0;
  });
  for (const g of groups) {
    const users = g.users.map((u) => `  ${u.label} (${u.file})${u.note ? `\n${wrap(`Note: ${u.note}`, 80, "      ")}` : ""}`);
    out.push(`${RULE}\nThe text below is from:\n${users.join("\n")}\n${THIN}\n${g.text}`);
  }

  out.push(`${RULE}\nPart 3. Standard licence texts\n${RULE}`);
  out.push(
    wrap(
      "For the components marked * in part 1, which name their licence but ship no licence file. The copyright holders are the authors listed for each component in part 1.",
    ),
  );
  for (const id of [...standard.keys()].sort()) {
    const users = standard.get(id).sort((a, b) => (a.toLowerCase() < b.toLowerCase() ? -1 : 1));
    out.push(`${RULE}\n${id}, for:\n${wrap(users.join(", "), 80, "  ")}\n${THIN}\n${standardText(id)}`);
  }
  return { text: `${out.join("\n\n")}\n`, problems, components: components.length };
}

// ---------------------------------------------------------------------------------------------

function spliceGenerated(existing, generated) {
  const block = `${BEGIN}\n\n${generated}\n\n${END}`;
  if (existing === null) return `# Dependency licences\n\n${block}\n`;
  const start = existing.indexOf(BEGIN);
  const end = existing.indexOf(END);
  if (start < 0 || end < start) throw new Error(`${INVENTORY} has lost its GENERATED markers.`);
  return existing.slice(0, start) + block + existing.slice(end + END.length);
}

function main() {
  const check = process.argv.includes("--check");
  const [engine, app] = WORKSPACES.map(cargoWorkspace);
  const npm = npmProduction();
  const made = notices(app, npm);
  const inventoryPath = rel(INVENTORY);
  const existing = fs.existsSync(inventoryPath) ? read(inventoryPath) : null;
  const outputs = [
    [INVENTORY, spliceGenerated(existing, inventory(engine, app, npm))],
    [NOTICES, made.text],
  ];
  for (const problem of made.problems) console.error(`warning: ${problem}`);
  let stale = false;
  for (const [file, text] of outputs) {
    const target = rel(file);
    const current = fs.existsSync(target) ? read(target) : null;
    if (current === text) continue;
    if (check) {
      stale = true;
      console.error(`${file} is out of date: run \`node scripts/licenses.mjs\` and commit the result.`);
    } else {
      fs.mkdirSync(path.dirname(target), { recursive: true });
      fs.writeFileSync(target, text);
      console.log(`wrote ${file}`);
    }
  }
  if (!check) {
    console.log(`${made.components} components in the notices; ${made.problems.length} without a licence text.`);
  }
  process.exit(stale || (check && made.problems.length > 0) ? 1 : 0);
}

main();
