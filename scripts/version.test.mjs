// Tests for the version helper: node --test scripts/version.test.mjs

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import {
  isVersion,
  jsonVersion,
  lockVersions,
  ownCrates,
  problems,
  setJsonVersion,
  setLockVersions,
  setTomlVersion,
  setVersion,
  tomlVersion,
  versionsIn,
} from "./version.mjs";

test("a version is three numbers and nothing else", () => {
  for (const good of ["0.1.0", "1.2.3", "10.20.30"]) assert.ok(isVersion(good), good);
  for (const bad of ["1.2", "1.2.3.4", "v1.2.3", "1.2.3-beta", "01.2.3", "1.2.x", "", undefined]) {
    assert.ok(!isVersion(bad), String(bad));
  }
});

const MANIFEST = `[workspace]
members = ["crates/*"]

[workspace.package]
version = "0.1.0"
edition = "2024"

[workspace.dependencies]
serde = { version = "1.0.229", features = ["derive"] }
`;

test("a manifest's version is read from its own section and set there only", () => {
  assert.equal(tomlVersion(MANIFEST, "workspace.package"), "0.1.0");
  assert.equal(tomlVersion(MANIFEST, "package"), null);
  const next = setTomlVersion(MANIFEST, "workspace.package", "0.2.0");
  assert.equal(tomlVersion(next, "workspace.package"), "0.2.0");
  assert.equal(next, MANIFEST.replace('version = "0.1.0"', 'version = "0.2.0"'));
  assert.ok(next.includes('serde = { version = "1.0.229"'), "a dependency's version is left alone");
  assert.throws(() => setTomlVersion(MANIFEST, "package", "0.2.0"), /no \[package\] section/);
});

const CONFIG = `{
  "productName": "PixelFlow",
  "version": "0.1.0",
  "bundle": {
    "windows": { "wix": { "version": "9.9.9" } }
  }
}
`;

test("a JSON file's top-level version is set without touching its layout or a nested one", () => {
  assert.equal(jsonVersion(CONFIG), "0.1.0");
  const next = setJsonVersion(CONFIG, "0.2.0");
  assert.equal(next, CONFIG.replace('"version": "0.1.0"', '"version": "0.2.0"'));
  assert.ok(next.includes('"version": "9.9.9"'));
  assert.equal(jsonVersion("{}"), null);
  assert.throws(() => setJsonVersion("{}", "0.2.0"), /no top-level "version"/);
});

const LOCK = `version = 4

[[package]]
name = "pf-model"
version = "0.1.0"
dependencies = [
 "serde",
]

[[package]]
name = "pixelflow-app"
version = "0.1.0"

[[package]]
name = "serde"
version = "0.1.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "abc"

[[package]]
name = "pf-model"
version = "9.9.9"
source = "registry+https://github.com/rust-lang/crates.io-index"
`;

test("only PixelFlow's own crates move in a lockfile", () => {
  const names = ["pf-model", "pixelflow-app", "serde"];
  assert.deepEqual(lockVersions(LOCK, names), { "pf-model": "0.1.0", "pixelflow-app": "0.1.0" });
  const next = setLockVersions(LOCK, names, "0.2.0");
  assert.deepEqual(lockVersions(next, names), { "pf-model": "0.2.0", "pixelflow-app": "0.2.0" });
  // A crates.io package keeps its version, even one named like ours or at the same version.
  assert.ok(next.includes('name = "serde"\nversion = "0.1.0"\nsource'));
  assert.ok(next.includes('name = "pf-model"\nversion = "9.9.9"\nsource'));
  assert.ok(next.startsWith("version = 4\n"), "the lockfile's own format number is not a package's");
});

const agreed = (version) => [
  { file: "Cargo.toml", what: "[workspace.package]", version },
  { file: "app/package.json", what: '"version"', version },
];

test("disagreeing files and a tag for another version are named plainly", () => {
  assert.deepEqual(problems(agreed("1.2.3")), []);
  assert.deepEqual(problems(agreed("1.2.3"), "v1.2.3"), []);
  assert.deepEqual(problems(agreed("1.2.3"), "v1.2.4"), [
    "The tag is v1.2.4, but the files say 1.2.3 (which would be the tag v1.2.3). " +
      "Set the version first: node scripts/version.mjs set 1.2.4",
  ]);
  assert.match(problems(agreed("1.2.3"), "1.2.3")[0], /^The tag is 1\.2\.3, but/);
  const mixed = agreed("1.2.3");
  mixed[1].version = "1.2.2";
  assert.deepEqual(problems(mixed, "v1.2.3"), [
    'app/package.json ("version") says 1.2.2, but Cargo.toml says 1.2.3.',
  ]);
  mixed[1].version = null;
  assert.deepEqual(problems(mixed), ['app/package.json has no version ("version").']);
  assert.match(problems(agreed("1.2.3-beta"))[0], /which is not three numbers like 1\.2\.3\.$/);
});

test("this repository's versions agree, lockfiles included", () => {
  const found = versionsIn();
  assert.deepEqual(problems(found), []);
  // Every one of PixelFlow's own crates is in the engine's lockfile, and the shell is in its own.
  const crates = ownCrates();
  assert.ok(crates.includes("pf-model") && crates.includes("pixelflow-app"), crates.join(", "));
  const inLock = (file) => found.filter((f) => f.file === file).map((f) => f.what);
  for (const name of crates.filter((n) => n !== "pixelflow-app")) {
    assert.ok(inLock("Cargo.lock").includes(name), `${name} is in Cargo.lock`);
  }
  assert.ok(inLock("app/src-tauri/Cargo.lock").includes("pixelflow-app"));
});

/** A copy of the files the helper reads and writes, in a folder of its own. */
function copyOfTheRepository() {
  const root = path.resolve(import.meta.dirname, "..");
  const copy = fs.mkdtempSync(path.join(os.tmpdir(), "pixelflow-version-"));
  const files = [
    "Cargo.toml",
    "Cargo.lock",
    "app/package.json",
    "app/src-tauri/Cargo.toml",
    "app/src-tauri/Cargo.lock",
    "app/src-tauri/tauri.conf.json",
    ...fs.readdirSync(path.join(root, "crates")).map((dir) => `crates/${dir}/Cargo.toml`),
  ];
  for (const file of files) {
    fs.mkdirSync(path.dirname(path.join(copy, file)), { recursive: true });
    fs.copyFileSync(path.join(root, file), path.join(copy, file));
  }
  return copy;
}

test("setting a version changes every file together, and a dry run changes none", () => {
  const root = copyOfTheRepository();
  try {
    const before = versionsIn(root);
    const planned = setVersion("98.76.54", { root, dryRun: true });
    assert.deepEqual(versionsIn(root), before, "a dry run writes nothing");
    assert.deepEqual(
      planned.map((c) => c.file),
      [
        "Cargo.toml",
        "app/src-tauri/Cargo.toml",
        "app/src-tauri/tauri.conf.json",
        "app/package.json",
        "Cargo.lock",
        "app/src-tauri/Cargo.lock",
      ],
    );
    assert.ok(planned.every((c) => c.from === before[0].version && c.to === "98.76.54"));

    assert.deepEqual(setVersion("98.76.54", { root }), planned);
    const after = versionsIn(root);
    assert.equal(after.length, before.length);
    assert.ok(after.every((f) => f.version === "98.76.54"), JSON.stringify(after));
    assert.deepEqual(problems(after, "v98.76.54"), []);
    assert.deepEqual(setVersion("98.76.54", { root }), [], "nothing left to change");
    assert.throws(() => setVersion("98.76", { root }), /not a version/);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});
