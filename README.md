# PixelFlow

A modern, fast, easy-to-use application for designing and running pixel light displays. A fresh take on xLights, built on a modern stack and free of legacy tech debt.

> **Status:** early development. Milestone 1 (layout, mapping & output) is in progress.

## Goals

- **Easy to use:** guided setup, drag-and-drop wiring, plain-language errors, undo everywhere.
- **Built for scale:** 200k+ pixels and 1,200+ universes at 40 fps.
- **No manual channel math:** channels and universes are assigned automatically from your wiring.
- **AI-assisted editing:** bring your own Anthropic or OpenAI key and edit layouts, configs, and (later) sequences with natural-language prompts.

## Roadmap

| Milestone | Focus |
|---|---|
| **M1** | Layout & props, automatic channel mapping, sACN/DDP output, FPP/WLED discovery and config push, test patterns, 2D/3D preview, camera-based pixel mapping, xLights import, AI assistant |
| M2 | 3D camera mapping |
| M3 | Sequencer: timeline, effects, audio analysis (beats/sections), lyric-to-prop mapping, AI-generated sequences |
| M4 | Rendering and `.fseq` export, FPP playback integration |

## Stack

- **Engine:** Rust (headless, real-time output)
- **Desktop shell:** Tauri 2
- **UI:** React + TypeScript + Vite, three.js (WebGPU) preview

## Development

Requires [Rust](https://rustup.rs). The toolchain version is pinned in `rust-toolchain.toml`, and rustup installs it automatically.

```sh
cargo test --workspace                                   # run all tests
cargo clippy --workspace --all-targets -- -D warnings    # lint
cargo fmt --all                                          # format

# Try the CLI on the demo show
cargo run -p pf-cli -- validate examples/shows/demo.pixelflow.json
cargo run -p pf-cli -- map examples/shows/demo.pixelflow.json
```

### Send a test pattern to real controllers

`test-pattern` sends live sACN/DDP output to the controllers in a show file, so point it at
your own show (the demo show's addresses are examples):

```sh
cargo run -p pf-cli -- test-pattern my-show.pixelflow.json --pattern chase --target "prop:Mega Tree"
cargo run -p pf-cli -- test-pattern my-show.pixelflow.json --pattern identify --target "port:Main FPP:2" --seconds 30
```

Patterns: `solid`, `cycle`, `chase`, `ramp`, `alternate`, `identify`, `walk`. Targets: `show`,
`prop:NAME`, `group:NAME`, `controller:NAME`, `port:CONTROLLER:NUMBER`. Use `--bind <local IP>`
to choose the network interface. Output stops with a blackout frame when the run ends.

### Run the desktop app

Requires [Node.js](https://nodejs.org) 22+ and [pnpm](https://pnpm.io).

```sh
cd app
pnpm install
pnpm tauri dev      # opens PixelFlow with live reload
pnpm test           # UI tests
pnpm tauri build    # builds an installable app
```

For UI-only work in a plain browser, run `pnpm dev` and open `http://localhost:1420/?demo`
(an in-memory sample show; nothing is sent to your controllers).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for branching, issues, and release workflow.
