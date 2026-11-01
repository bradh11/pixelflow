# Contract checks between the window and the shell

The window (`app/src`, TypeScript) and the shell (`app/src-tauri`, Rust) agree on every command,
reply and event only by convention: the types in `app/src/api` are written by hand to mirror the
Rust ones. Three sets of committed files in `app/src/api/contracts/` hold the two sides to each
other. Each is written by one side's tests and read by the other's, and each fails a test when it
is stale. A fourth set, kept the same way, holds the video export to the picture the previews
draw.

## What is checked

**What the shell sends** (`replies/*.json`). The shell's tests
(`app/src-tauri/src/tests/contracts/replies.rs` and the files beside it) run real commands
through Tauri's mock runtime and write down what came back: replies, every event's payload,
rejections, and every member of each enum. Each entry names the TypeScript type it must fit.
`replies.test.ts` feeds them to the TypeScript compiler and fails on a type error, on a union
member one side has and the other doesn't, on a field the window declares that the shell never
sends, and on an event whose listener expects another payload.

**What the window sends** (`commandCalls.json`). `commandCalls.test.ts` calls every method of
every API object (`tauriBackend`, the sequencer, the assistant, the window) against a mocked
`invoke` and writes down each command and its arguments. The shell's tests
(`tests/contracts/commands.rs`) check each one against the shell's own source (the command is
registered, every key is a parameter, every required parameter is sent), run it through the mock
runtime to see that its arguments are read, and check that no field inside an argument is one the
Rust type silently drops. Commands that can't be run against fakes are listed in `NOT_RUN` with
the reason; they are still checked by name.

**The browser's stand-ins** (`parity/*.json`). `MemoryBackend` and `MemorySequencer` do in
TypeScript what the engine does. `tests/parity.rs` runs scripts of commands against the real
engine and writes down what each step answered and left behind; `parity.test.ts` runs the same
scripts against the stand-ins and compares every step. A difference the stand-ins have on purpose
is listed in `KNOWN` or `ENGINE_FILLS_IN` in that file, with the reason, and fails once it no
longer holds.

**What the previews draw** (`pixels/*.json`). The flat previews draw the show's pixels in
TypeScript (`app/src/lib/pixelBatches.ts`, fitted and sized as `app/src/lib/flatPreview.ts` says);
the video export draws them in Rust (`crates/pf-video/src/dots.rs` and `raster.rs`), and a frame
of a video must be the preview's picture of that moment. Each file is a scene: a small made-up
layout and a frame of colors (a sparse string, a dense one, strings crossing, matrices, lit beside
unlit, a fade, pixels closer than a screen pixel), a preview's canvas, and the picture of dots the
preview draws there with no glow. A few draw with the viewer's View options too: pixels that are
off hidden (`dark` is all zeros) and Boost dim (`boost`, its step; the frame on file is the show's,
and each side lifts it with its own code). `pixels.test.ts` draws every scene with the preview's own code
and writes the files. The export's test (`shared_scenes` in `crates/pf-video/src/raster.rs`) draws
the same scenes with the export's code. It fails unless it fits each display in the same view,
uses the same dot size and the same color for a pixel that's off, and draws every pixel of the
picture the same: the same color and the same alpha, with no tolerance. The scenes' coordinates
are whole numbers of 128ths, so both sides start from the very same numbers.

## When a check fails

A failure names the file, the entry or step, and what differs. Decide which side is wrong and fix
that side. Don't regenerate to make a real difference go away.

When the change was meant (a new command, a new field, a new edit), regenerate:

```sh
cd app/src-tauri && PIXELFLOW_UPDATE_CONTRACTS=1 cargo test --locked -- contracts parity
cd app && PIXELFLOW_UPDATE_CONTRACTS=1 npx vitest run src/api/contracts
cd app/src-tauri && cargo test --locked -- contracts parity
```

The first writes what the shell sends and the parity files. The second writes what the window
sends and checks the window against the shell's files. The third checks the shell against the
window's calls. When both sides changed, the first can fail on the window's calls, which the
second then rewrites; the files it writes are written all the same. Commit the changed files with
the change that caused them.

The second also writes the pictures the previews draw. After a change to how the previews draw
pixels, make the same change to the export and check it from the repository's root:

```sh
cargo test --locked -p pf-video
```

## Adding to them

- A new command: call it in `commandCalls.test.ts` (that suite fails until every method of the API
  objects is called, the shell's until every registered command is recorded), and record its
  reply in a scenario beside `tests/contracts/replies.rs`, naming the type it must fit.
- A new event: the shell's tests fail until its payload is recorded.
- A new enum variant: the lists in `tests/contracts/replies/unions.rs` match without a wildcard,
  so the shell's tests don't compile until it's added.
- A behaviour the stand-ins imitate: add a scenario to `tests/parity.rs` and its name to
  `SCENARIOS`.
- A way the previews draw pixels that no scene shows: add a scene to `SCENES` in
  `pixels.test.ts`. The export's test reads every file in `pixels/`.

The files hold no temp paths, times, or random ids (`Stable` in `tests/contracts/support.rs`
replaces them), so they are the same on every run and platform.
