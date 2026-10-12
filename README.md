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

## Documentation

- [Your first show](docs/first-show.md): from an empty show, or an xLights show, to lights playing music.
- [Troubleshooting](docs/troubleshooting.md): nothing lights, wrong colors, controllers not found, Send to FPP problems, and more.
- [Real-hardware acceptance checklist](docs/hardware-acceptance.md): a checklist to fill in against real controllers.
- [xLights import fidelity](docs/fidelity.md): how an imported sequence looks next to xLights, and how that is measured.
- [Shaders](docs/shaders.md): the Shader effect, shader files, where they live, what runs, and its limits.
- [Scale benchmark](docs/benchmarks.md): how large a show the engine can run.
- [Your data: what stays on your computer](docs/privacy-and-data-flow.md): what PixelFlow sends, and when.
- [Releasing](docs/releasing.md): building the macOS and Windows installers, signing, installing a test build, and where the app keeps its data.

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
setup so you can add them to a show. Finding and reading change nothing on a controller. The
app only writes to one when you choose **Send setup to this controller…** (FPP and WLED; Falcon
controllers are read-only), and it shows what will change first.

```sh
make discover                                    # or: cargo run -p pf-cli -- discover
cargo run -p pf-cli -- discover --host 10.0.0.50  # also check an address directly
cargo run -p pf-cli -- device 10.0.0.50           # one controller's setup and what importing adds
```

In the app, use **Find my controllers** on the welcome screen or **Scan network** on the
**Controllers** screen.

Discovery listens for FPP's MultiSync ping and mDNS, checks the web page of every address on
the /24 around each of your private network addresses, and asks each FPP which controllers it sends to. If a firewall
blocks the replies (macOS does for unsigned command-line tools), the network check and FPP's
list usually still find your controllers; a controller an FPP lists that doesn't answer is reported so you
can check its power and network cable.

### Play a sequence

**Play** is the show-time view: your lights filling the window while a sequence plays, with a
transport under them (play, pause, back to the start, stop, a seek bar, 2D or 3D, and glow).

- **What you're working on plays there.** A sequence open on the **Sequence** screen shows on
  Play ready to go. It's the same playback as the Sequence screen's, so play, pause, and the
  position stay in step between the two. **Show on my lights** (the same toggle on Sequence)
  sends it to your controllers as it plays; off, it plays only on screen.
- **Full screen** (or F) puts everything else away, leaving the lights and a transport that
  hides itself while a sequence plays. Escape brings the app back.
- **Playlist** opens a drawer with everything there is to play: the open sequence, the
  sequences saved with this show, and the show's rendered ones. Picking one plays it. Each
  rendered sequence's music, its volume, and its **Light timing** (move the lights earlier or
  later against the music) are there too, and so is what's on your FPP. **Light timing** is
  per sequence; **Preview sync** in Settings is separate and moves only the on-screen preview.

PixelFlow also plays rendered xLights/FPP sequences (`.fseq`) straight to your controllers:

1. On **Controllers**, open your FPP and choose **Set up my show from this FPP**, then
   **Add N controllers** (N is how many it lists). This tells PixelFlow which sequence channels
   belong to each controller, and it works even while a controller is offline.
2. On **Play**, open the **Playlist** and choose **Add sequence**, then pick the `.fseq`.
   PixelFlow finds its music next to it. Props you've imported light up in the preview. A
   controller whose strings aren't imported yet shows the channels it receives as a grid.

If an FPP is playing at the same time, its output overrides PixelFlow's. The Play screen warns
you about this and can stop the FPP for you.

### Put the preview in its own window

**Pop out the preview** (on the Sequence screen's preview, on Play's transport, and as
**Preview window** on the Layout screen's tool bar) opens the preview in a window of its own, so
the main window's room goes to the timeline and controls, or the lights go to another display.
Drag it there and use its **Full screen** button (or F) to fill that display.

The window shows what the screen in front would: the open sequence at the playhead on Sequence
and Play, and the lights live (a rendered sequence playing, a test pattern) on Layout and the
other screens. On Layout the canvas stays as it is, since it's the editor; the window is a
second, look-only view, so you can edit in 2D and watch in 3D. It only looks: 2D or 3D and glow
can be changed there, and play or pause asked for, but nothing else. Close it, or choose
**Bring it back**, and the preview returns to where it was. PixelFlow remembers that it was
popped out, and where its window was.

### Control an FPP

On **Controllers**, open an FPP to see what it's playing, how long is left, and what's scheduled next.
**Stop now**, **Stop after this**, and **Play** (for a sequence stored on the FPP) change what the
FPP is doing, and they only run when you click them. Everything else PixelFlow does with your
controllers only reads, apart from **Send setup**, which asks first.

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
