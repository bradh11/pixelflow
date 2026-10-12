# The shell's structure

The desktop shell (`app/src-tauri/src`) is the bridge between the window and the engine: commands
the window calls, events it listens for, and the app's state. Most command groups have a module of
their own. `lib.rs` still holds too much: it was about 4,000 lines, 3,400 of them tests of other
modules' commands. This is the plan for making it small without the window noticing.

Stage 0 is done. The rest is to do, one small pull request per step.

## What no step may change

- **Command names.** The window calls each command by name (`app/src/api`).
  `commands::tests::the_window_calls_the_commands_by_these_names` pins all 154. Moving a command
  leaves that test alone. A step that has to edit it has renamed, added, or dropped a command.
- **Arguments, replies, messages, and event names** (`audio-progress`, `sequence-export-progress`,
  `video-export-progress`, `lyrics-progress`, `align-models-progress`, `fpp-send-progress`,
  `fpp-download-progress`, `assistant-event`, `pictures-arrived`, `menu`). The tests say what they
  are, so tests move with their commands unchanged.
- **Who may call what**: `capabilities/` and `preview_window::may_call`.
- **Behaviour.** A step moves code. A bug found on the way is fixed in its own pull request.

## Where things go

| Module | Holds |
| --- | --- |
| `lib.rs` | The module list, `run()`, and the app's own life: quitting, autosave, `context()` |
| `state.rs` | `AppState`, `PathArg`, `Reply`, `message` |
| `commands.rs` | The one list of commands, `with_commands()`, and the test that pins their names |
| `show_file.rs` | The show itself: snapshot, edits, undo and redo, new, sample, open, save, history |
| `output.rs` | Test patterns: start, stop, status |
| every other module | Its commands, as now, and the tests of those commands |
| `testing.rs` | What tests run on: `app()`, `call()`, the event helpers, and fixtures two modules share |
| `tests/*.rs` | Tests that cross modules or are about the app itself, under `mod tests` in `lib.rs` |

## Stages

**0. A harness every module can use (done).** `app()`, `app_in()`, `app_without_window()`, `call()`
and `call_raw()` live in `testing.rs`; the assistant's tests build on the same app. The device-setup
tests moved to `device_setup.rs`, the list of commands to `commands.rs` with its names pinned, and
the output commands to `output.rs`. `lib.rs` went from 4,010 lines to about 3,250.

**1. The last commands leave `lib.rs`.** The eleven show commands, with `open_show_at`, `remember`
and `SAMPLE_SHOW`, go to `show_file.rs`. Their entries in `commands.rs` change from `crate::` to
`show_file::`, and nothing else does. `recent`, which opens a show the same way, names the new
path.

**2. The state gets a file.** `AppState`, `PathArg`, `Reply` and `message` go to `state.rs` and are
re-exported from the crate root, so every `crate::AppState` in other modules keeps working. No
field is added, removed, or renamed.

**3. Tests follow their commands**, one module per step, largest first: `sequencer`, `devices`,
`fpp_send` and `fpp_download`, `files`, `pictures`, `recent`, `playback`, `xlights` and `vendor`,
`camera_map`, `layout` and `house`, `pickers`, `preview_window`, `show_file`. Find lyrics' command
tests in `assistant.rs` go to `lyrics.rs`, which has the fixture for them. A test moves whole: its
name and body stay as they are. A fixture that two modules need goes to `testing.rs` first, in a
commit of its own.

**4. What is left gets files.** Tests that cross modules (a sequence authored, previewed, played,
and exported) or are about the app itself (the window's permissions, what it may load, quitting,
the menu) go to `src/tests/*.rs`. `mod tests` stays in `lib.rs` as their parent and keeps importing
the harness by name, so a file there still gets it with `use super::*`.

After stage 4 `lib.rs` is about 300 lines.

## Checking a step

```sh
cd app/src-tauri
cargo fmt --check && cargo clippy --all-targets --locked -- -D warnings && cargo test --locked
cargo test --locked -- --list | grep -c ': test$'   # never drops: a moved test still runs
```

- The pinned names test passes without being edited.
- The lines taken out of one file are the lines put into another. `git diff --color-moved` shows
  it; so does comparing the sorted removed lines with the sorted added ones. Only `pub(crate)` and
  `use` lines differ.
- New tests of long work (exports, sends, downloads, Find lyrics) listen for the event by its name
  and cancel from inside the listener with `testing::call_at`, so a cancel lands at a known step.
  They never wait on a timer.
