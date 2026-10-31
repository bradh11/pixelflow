// Tests for the encoder check: node --test scripts/encoder-check.test.mjs

import assert from "node:assert/strict";
import { test } from "node:test";
import { problems } from "./encoder-check.mjs";

// What `cargo tree -e normal,build,features -i openh264-sys2` prints for the app as it must be.
const SHIPPED = `openh264-sys2 v0.9.8
└── openh264 v0.9.8
    └── openh264 feature "libloading"
        └── pf-video v0.1.0 (/repo/crates/pf-video)
            └── pf-video feature "default"
                └── pixelflow-app v0.1.0 (/repo/app/src-tauri)
└── openh264-sys2 feature "libloading"
    └── openh264 feature "libloading" (*)
`;

// The same, with a dependency that turns on the encoder compiled from source.
const FROM_SOURCE = `openh264-sys2 v0.9.8
└── openh264 v0.9.8
    ├── openh264 feature "libloading"
    │   └── pf-video v0.1.0 (/repo/crates/pf-video)
    │       └── pf-video feature "encoder-from-source"
    │           └── pixelflow-app v0.1.0 (/repo/app/src-tauri)
    └── openh264 feature "source"
        └── pf-video feature "encoder-from-source" (*)
├── openh264-sys2 feature "libloading"
│   └── openh264 feature "libloading" (*)
└── openh264-sys2 feature "source"
    └── openh264 feature "source" (*)
`;

test("the app as it must be passes", () => {
  assert.deepEqual(problems(SHIPPED), []);
});

test("an encoder compiled from source fails, naming each feature that brings it", () => {
  const found = problems(FROM_SOURCE);
  assert.equal(found.length, 3);
  assert.match(found[0], /openh264-sys2 feature "source"/);
  assert.match(found[1], /openh264 feature "source"/);
  assert.match(found[2], /pf-video feature "encoder-from-source"/);
});

test("an app that can't load Cisco's binary fails too", () => {
  const found = problems("openh264-sys2 v0.9.8\n└── openh264 v0.9.8\n");
  assert.deepEqual(found.length, 1);
  assert.match(found[0], /libloading/);
});
