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

Common tasks have `make` shortcuts — run `make` to list them. The most useful:

```sh
make setup   # install the app's dependencies (once)
make run     # run the desktop app with live reload
make test    # run every test
make lint    # formatting and lint checks (same as CI)
make build   # build an installable app
```

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

### Find your controllers

PixelFlow finds FPP, Falcon, and WLED controllers on your network and reads their pixel
setup so you can add them to a show. It only reads — nothing on a controller is changed.

```sh
make discover                                    # or: cargo run -p pf-cli -- discover
cargo run -p pf-cli -- discover --host 10.0.0.50  # also check an address directly
cargo run -p pf-cli -- device 10.0.0.50           # one controller's setup and what importing adds
```

In the app, use **Discover my devices** on the welcome screen or the **Devices** screen.

Discovery listens for FPP's MultiSync ping and mDNS, checks the web page of every address on
the /24 around each of your private network addresses, and asks each FPP which controllers it sends to. If a firewall
blocks the replies (macOS does for unsigned command-line tools), the network check and FPP's
list usually still find your controllers; a controller an FPP lists that doesn't answer is reported so you
can check its power and network cable.

### Play a sequence

PixelFlow plays rendered xLights/FPP sequences (`.fseq`) straight to your controllers and shows
them on screen:

1. On **Devices**, open your FPP and choose **Add to show** next to each controller it sends to.
   This tells PixelFlow which sequence channels belong to each controller, and it works even
   while a controller is offline.
2. On **Play**, choose **Open sequence…** and pick the `.fseq`. You get play, pause, seek, and
   stop. Props you've imported light up in the preview. A controller whose strings aren't
   imported yet shows the channels it receives as a grid.

If an FPP is playing at the same time, its output overrides PixelFlow's. The Play screen warns
you about this and can stop the FPP for you.

### Control an FPP

On **Devices**, open an FPP to see what it's playing, how long is left, and what's scheduled next.
**Stop now**, **Stop after this**, and **Play** (for a sequence stored on the FPP) change what the
FPP is doing, and they only run when you click them. Everything else PixelFlow does with your
controllers only reads.

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
