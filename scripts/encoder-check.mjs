#!/usr/bin/env node
// Checks that the desktop app is built without an H.264 encoder compiled from source.
//
//   node scripts/encoder-check.mjs
//
// The app uses Cisco's own OpenH264 binary, which each user's copy downloads from Cisco when the
// user agrees (crates/pf-video/src/cisco.rs): Cisco's patent licence covers only the binary
// Cisco hands out. OpenH264 compiled from source (`openh264-sys2`'s `source` feature, turned on
// by pf-video's `encoder-from-source`) is for tests and developer tools only.
//
// This asks Cargo which features the app's normal and release builds turn on, on every target
// (dev-dependencies left out, as `cargo build` and `tauri build` leave them out), and exits 1 if
// any of those features is among them. Needs only cargo; reads the app's Cargo.lock as it is.

import { execFileSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

/** The features that put a from-source encoder in a build. */
export const FORBIDDEN = [
  'openh264-sys2 feature "source"',
  'openh264 feature "source"',
  'pf-video feature "encoder-from-source"',
];

/** The feature the app's encoder must come through: Cisco's binary, loaded at run time. */
export const REQUIRED = 'openh264-sys2 feature "libloading"';

/**
 * What is wrong with a build, from `cargo tree -e normal,build,features -i openh264-sys2`'s
 * output: each forbidden feature it turns on, and whether runtime loading is missing.
 */
export function problems(tree) {
  const found = [];
  for (const feature of FORBIDDEN) {
    if (tree.split("\n").some((line) => line.includes(feature))) {
      found.push(`It turns on ${feature}: OpenH264 compiled from source would be in the app.`);
    }
  }
  if (!tree.includes(REQUIRED)) {
    found.push(`It doesn't turn on ${REQUIRED}: the app couldn't load Cisco's binary.`);
  }
  return found;
}

/** The app's dependency tree to openh264-sys2, with features, as its builds resolve it. */
function appTree() {
  return execFileSync(
    "cargo",
    [
      "tree",
      "--manifest-path",
      path.join(ROOT, "app", "src-tauri", "Cargo.toml"),
      "--locked",
      "--target",
      "all",
      "-e",
      "normal,build,features",
      "-i",
      "openh264-sys2",
    ],
    { encoding: "utf8", stdio: ["ignore", "pipe", "inherit"] },
  );
}

function main() {
  const tree = appTree();
  const found = problems(tree);
  if (found.length > 0) {
    console.error("The desktop app's build is not as it must be:");
    for (const problem of found) console.error(`  - ${problem}`);
    console.error("\nHow openh264-sys2 comes into it:\n");
    console.error(tree);
    process.exit(1);
  }
  console.log("The desktop app has no H.264 encoder compiled from source: it loads Cisco's OpenH264 binary at run time.");
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) main();
