#!/usr/bin/env node
// PixelFlow's version number: one number, written in several files that must agree.
//
//   node scripts/version.mjs check                 print the version; exit 1 if the files disagree
//   node scripts/version.mjs check --tag v0.2.0    the same, and the tag must be that version
//   node scripts/version.mjs set 0.2.0             write 0.2.0 into every file
//   node scripts/version.mjs set 0.2.0 --dry-run   say what would change; write nothing
//
// The files:
//
//   Cargo.toml                        [workspace.package] version (every engine crate uses it)
//   app/src-tauri/Cargo.toml          [package] version (the desktop shell)
//   app/src-tauri/tauri.conf.json     "version" (what the installers and the About box show)
//   app/package.json                  "version"
//   Cargo.lock                        the entries for PixelFlow's own crates
//   app/src-tauri/Cargo.lock          the same
//
// The lockfiles are edited as text, so nothing is downloaded and no other entry moves: CI builds
// with --locked, which fails if a lockfile names another version than its manifest.
//
// A version is three numbers (1.2.3) and a tag is that with a "v" in front. Nothing after the
// third number: the Windows .msi installer cannot carry a label like "-beta".
//
// Needs only Node. How a release is cut: docs/releasing.md.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

const WORKSPACE_MANIFEST = "Cargo.toml";
const SHELL_MANIFEST = "app/src-tauri/Cargo.toml";
const TAURI_CONFIG = "app/src-tauri/tauri.conf.json";
const PACKAGE_JSON = "app/package.json";
const LOCKFILES = ["Cargo.lock", "app/src-tauri/Cargo.lock"];

/** Whether `text` is a version this project uses: three numbers, nothing else. */
export function isVersion(text) {
  return typeof text === "string" && /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(text);
}

/** The lines of `[section]` in a TOML file: where they start and end (the header excluded). */
function sectionLines(lines, section) {
  const start = lines.findIndex((line) => line.trim() === `[${section}]`);
  if (start < 0) return null;
  let end = lines.findIndex((line, i) => i > start && /^\s*\[/.test(line));
  if (end < 0) end = lines.length;
  return { start: start + 1, end };
}

const TOML_VERSION = /^(\s*version\s*=\s*")([^"]*)(".*)$/;

/** The `version = "…"` of `[section]` in a TOML file's text; `null` when it has none. */
export function tomlVersion(text, section) {
  const lines = text.split("\n");
  const at = sectionLines(lines, section);
  if (!at) return null;
  for (let i = at.start; i < at.end; i++) {
    const found = TOML_VERSION.exec(lines[i]);
    if (found) return found[2];
  }
  return null;
}

/** The TOML text with `[section]`'s version set to `version` (everything else untouched). */
export function setTomlVersion(text, section, version) {
  const lines = text.split("\n");
  const at = sectionLines(lines, section);
  if (!at) throw new Error(`no [${section}] section`);
  for (let i = at.start; i < at.end; i++) {
    if (TOML_VERSION.test(lines[i])) {
      lines[i] = lines[i].replace(TOML_VERSION, `$1${version}$3`);
      return lines.join("\n");
    }
  }
  throw new Error(`[${section}] has no version`);
}

// The top-level "version" of a JSON file written with two-space indents (both of ours are). The
// file is edited as text so its layout and key order stay as they are.
const JSON_VERSION = /^( {2}"version"\s*:\s*")([^"]*)(".*)$/m;

/** The top-level `"version"` in a JSON file's text; `null` when it has none. */
export function jsonVersion(text) {
  const found = JSON_VERSION.exec(text);
  return found ? found[2] : null;
}

/** The JSON text with its top-level version set to `version`. */
export function setJsonVersion(text, version) {
  if (!JSON_VERSION.test(text)) throw new Error('no top-level "version"');
  return text.replace(JSON_VERSION, `$1${version}$3`);
}

/**
 * The `[[package]]` entries of a Cargo.lock for PixelFlow's own crates (`names`): each one's
 * name, version, and the line its version is on. Its own crates have no `source` line, which
 * tells them from a crates.io package that happens to share a name.
 */
function ownLockEntries(lines, names) {
  const entries = [];
  for (let i = 0; i < lines.length; i++) {
    if (lines[i].trim() !== "[[package]]") continue;
    let end = i + 1;
    while (end < lines.length && lines[end].trim() !== "" && lines[end].trim() !== "[[package]]") end++;
    const block = lines.slice(i + 1, end);
    const name = block.map((l) => /^name = "([^"]*)"$/.exec(l)?.[1]).find(Boolean);
    const versionAt = block.findIndex((l) => /^version = "[^"]*"$/.test(l));
    const local = !block.some((l) => l.startsWith("source = "));
    if (name && names.includes(name) && local && versionAt >= 0) {
      const line = i + 1 + versionAt;
      entries.push({ name, version: /"([^"]*)"/.exec(lines[line])[1], line });
    }
  }
  return entries;
}

/** The version of each of PixelFlow's own crates (`names`) in a Cargo.lock's text, by name. */
export function lockVersions(text, names) {
  return Object.fromEntries(ownLockEntries(text.split("\n"), names).map((e) => [e.name, e.version]));
}

/** The Cargo.lock text with PixelFlow's own crates (`names`) set to `version`. */
export function setLockVersions(text, names, version) {
  const lines = text.split("\n");
  for (const entry of ownLockEntries(lines, names)) {
    lines[entry.line] = `version = "${version}"`;
  }
  return lines.join("\n");
}

/** The package name in a crate manifest's text. */
function packageName(text) {
  const lines = text.split("\n");
  const at = sectionLines(lines, "package");
  if (!at) return null;
  for (let i = at.start; i < at.end; i++) {
    const found = /^\s*name\s*=\s*"([^"]*)"/.exec(lines[i]);
    if (found) return found[1];
  }
  return null;
}

/** The names of PixelFlow's own crates: every crate in crates/, and the desktop shell. */
export function ownCrates(root = ROOT) {
  const read = (file) => fs.readFileSync(path.join(root, file), "utf8");
  const crates = fs
    .readdirSync(path.join(root, "crates"))
    .filter((dir) => fs.existsSync(path.join(root, "crates", dir, "Cargo.toml")))
    .map((dir) => packageName(read(`crates/${dir}/Cargo.toml`)))
    .filter(Boolean);
  return [...crates, packageName(read(SHELL_MANIFEST))].sort();
}

/**
 * Every place the version is written: `{ file, what, version }`, where `version` is `null` when
 * the file doesn't say.
 */
export function versionsIn(root = ROOT) {
  const read = (file) => fs.readFileSync(path.join(root, file), "utf8");
  const names = ownCrates(root);
  const found = [
    { file: WORKSPACE_MANIFEST, what: "[workspace.package]", version: tomlVersion(read(WORKSPACE_MANIFEST), "workspace.package") },
    { file: SHELL_MANIFEST, what: "[package]", version: tomlVersion(read(SHELL_MANIFEST), "package") },
    { file: TAURI_CONFIG, what: '"version"', version: jsonVersion(read(TAURI_CONFIG)) },
    { file: PACKAGE_JSON, what: '"version"', version: jsonVersion(read(PACKAGE_JSON)) },
  ];
  for (const file of LOCKFILES) {
    for (const [name, version] of Object.entries(lockVersions(read(file), names))) {
      found.push({ file, what: name, version });
    }
  }
  return found;
}

/**
 * What is wrong with the versions `found` (from [`versionsIn`]), as sentences; none when they
 * all agree. With `tag`, the tag must be "v" and that version.
 */
export function problems(found, tag) {
  const out = [];
  const wanted = found[0].version;
  for (const { file, what, version } of found) {
    if (version === null) out.push(`${file} has no version (${what}).`);
    else if (!isVersion(version)) {
      out.push(`${file} (${what}) says ${version}, which is not three numbers like 1.2.3.`);
    } else if (version !== wanted) out.push(`${file} (${what}) says ${version}, but ${found[0].file} says ${wanted}.`);
  }
  if (tag !== undefined && out.length === 0 && tag !== `v${wanted}`) {
    out.push(
      `The tag is ${tag}, but the files say ${wanted} (which would be the tag v${wanted}). ` +
        `Set the version first: node scripts/version.mjs set ${tag.replace(/^v/, "")}`,
    );
  }
  return out;
}

/**
 * Writes `version` into every file under `root` (or, with `dryRun`, writes nothing). Returns the
 * changes as `{ file, from, to }`, one for each file that held another version.
 */
export function setVersion(version, { root = ROOT, dryRun = false } = {}) {
  if (!isVersion(version)) {
    throw new Error(`${version} is not a version: use three numbers, like 1.2.3.`);
  }
  const names = ownCrates(root);
  const edits = [
    [WORKSPACE_MANIFEST, (t) => tomlVersion(t, "workspace.package"), (t) => setTomlVersion(t, "workspace.package", version)],
    [SHELL_MANIFEST, (t) => tomlVersion(t, "package"), (t) => setTomlVersion(t, "package", version)],
    [TAURI_CONFIG, jsonVersion, (t) => setJsonVersion(t, version)],
    [PACKAGE_JSON, jsonVersion, (t) => setJsonVersion(t, version)],
    ...LOCKFILES.map((file) => [
      file,
      (t) => [...new Set(Object.values(lockVersions(t, names)))].join(", "),
      (t) => setLockVersions(t, names, version),
    ]),
  ];
  const changes = [];
  for (const [file, current, write] of edits) {
    const at = path.join(root, file);
    const before = fs.readFileSync(at, "utf8");
    const after = write(before);
    if (after === before) continue;
    changes.push({ file, from: current(before), to: version });
    if (!dryRun) fs.writeFileSync(at, after);
  }
  return changes;
}

function main(argv) {
  const [command, ...rest] = argv;
  if (command === "check" || command === undefined) {
    const tagAt = rest.indexOf("--tag");
    const tag = tagAt >= 0 ? rest[tagAt + 1] : undefined;
    if (tagAt >= 0 && !tag) {
      console.error("--tag needs the tag, like --tag v1.2.3.");
      return 2;
    }
    const found = versionsIn();
    const wrong = problems(found, tag);
    if (wrong.length > 0) {
      console.error("PixelFlow's version is not the same everywhere:");
      for (const line of wrong) console.error(`  ${line}`);
      return 1;
    }
    console.log(found[0].version);
    return 0;
  }
  if (command === "set") {
    const version = rest.find((arg) => !arg.startsWith("--"));
    const dryRun = rest.includes("--dry-run");
    if (!version) {
      console.error("Which version? Like: node scripts/version.mjs set 1.2.3");
      return 2;
    }
    let changes;
    try {
      changes = setVersion(version, { dryRun });
    } catch (error) {
      console.error(error.message);
      return 2;
    }
    if (changes.length === 0) {
      console.log(`Every file already says ${version}.`);
      return 0;
    }
    for (const { file, from, to } of changes) {
      console.log(`${file}: ${from} -> ${to}`);
    }
    console.log(dryRun ? "Dry run: nothing was written." : `The version is now ${version}.`);
    return 0;
  }
  console.error("Usage: node scripts/version.mjs check [--tag vX.Y.Z] | set X.Y.Z [--dry-run]");
  return 2;
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  process.exit(main(process.argv.slice(2)));
}
