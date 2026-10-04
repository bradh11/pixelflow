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

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for branching, issues, and release workflow.
