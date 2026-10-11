# Scale benchmark

PixelFlow's goal is 200,000 pixels at 40 frames a second. `pf-bench` (`crates/pf-bench`) measures
how close it is. It builds a synthetic show and sequence of any size, runs them through the
engine's own rendering, channel mapping, packetizing, and sending, and reports where the time goes
and whether every frame made it.

> **Everything here is synthetic, and nothing leaves the machine.** The shows and sequences are
> made up in code. Packets go to listeners on the loopback interface, or nowhere. No network card,
> controller, or screen takes part. Read a result as "the engine has room for this many pixels on
> this machine", not as "this show will play on your network".

- [Run it](#run-it)
- [What runs](#what-runs)
- [The shows](#the-shows) and [the sequences](#the-sequences)
- [Reading the results](#reading-the-results)
- [Baseline](#baseline): the numbers from one machine
- [What the baseline says](#what-the-baseline-says): verdicts, bottlenecks, recommendations
- [Caveats](#caveats)

## Run it

```sh
cargo run --release -p pf-bench -- --tier 200k --seconds 60 --mode paced --sink loopback
```

plays the typical 200,000-pixel sequence in real time for 60 seconds (after 2 seconds of warm-up),
sending to loopback listeners, and prints that run's tables.

| Option | What it chooses | Default |
| --- | --- | --- |
| `--tier` | Pixels in the show: `20k`, `100k`, `200k`, or any count from 500 | `200k` |
| `--variant` | The sequence: `typical` or `worst` | `typical` |
| `--mode` | `paced` (real time), `asap` (flat out), `engine` (the engine itself), `effects` (cost by effect) | `paced` |
| `--sink` | `null` (packets are built and dropped) or `loopback` (UDP to 127.0.0.1) | `null` |
| `--seconds` | Seconds measured | `60` |
| `--warmup-seconds` | Seconds played first and reported apart | `2` |
| `--send-hz` | Output passes a second in a paced run, instead of playback's twice the frame rate | |
| `--calm` | Before each run, wait (up to two minutes) for the one-minute load average to fall below this | |
| `--json`, `--markdown` | Also write the results to a file | |
| `--render` | Print a report saved with `--json` again, and run nothing | |
| `--describe` | Print the show and its sequences, and run nothing | |

The whole matrix (three tiers, both sequences, flat out and in real time, both sinks, then the
engine check and the cost of each effect at the largest tier) is one command, about half an hour:

```sh
cargo run --release -p pf-bench -- --all --engine-check --effects --calm 5 \
    --json bench.json --markdown bench.md
```

Each run of the matrix is a process of its own, so one run can't warm caches or fill memory for
the next. `--tiers 20k,100k` runs part of it.

Use a release build (the tool says so when it isn't one), close what you can, and look at the
"Load average" column afterwards: a run that shared the machine with a compiler is a measurement
of the compiler.

## What runs

```text
            player thread                         output thread
 sequence ──▶ render ──▶ show frame ──▶ latest ──▶ map ──▶ packetize ──▶ send ──▶ sink
                            │           frame
                            └──▶ two copies, for the app's preview
```

Every stage is the engine's own code, called the way the engine calls it:

| Stage | What it is | In the engine |
| --- | --- | --- |
| Render | Draw every active effect of the sequence into the show frame | `pf_render::Renderer::render_frame`, as live playback and FSEQ export use it |
| Preview | Copy the frame twice, for the app to read | playback's `Frames::show` |
| Map | Gather each controller's channels: wiring order, color order, brightness, gamma | `pf_output::render_controller` |
| Packetize | Copy the channels into sACN and DDP packets | `SacnPackets::update`, `DdpPackets::update` |
| Send | Hand each packet to the socket | `pf_output::UdpTransport` |

The modes drive those stages differently:

- **`asap`** runs them one after another on one thread, as fast as they go. It answers "what does
  each stage cost?" and gives the ceiling: frames a second with nothing else in the way.
- **`paced`** is live playback with a stopwatch on it. The player thread works out which frame is
  due, draws it, copies it for the preview, hands it over, and sleeps to the next frame boundary
  (`pf-engine`'s `run_player`). The output thread sends the latest frame on `pf_output::FrameClock`
  at twice the sequence's rate, 80 passes a second, as playback does so that every frame goes out
  at least once. A third thread reads the preview frame 20 times a second, as the Play screen
  does. It answers "does every frame arrive, and when?"
- **`engine`** opens the same show and sequence in `pf_engine::Engine` and plays them, watching
  from outside which frame is showing. It can see less (skipped frames, and how long after it was
  due each frame was drawn), but it is the real thing, so it checks that `paced` behaves as
  playback does. The engine reads music and pictures from files, which the benchmark doesn't
  write, so this run plays without them.
- **`effects`** draws each kind of effect on its own, and each synthetic sequence with one kind at
  a time, to say which effects the time goes on.

Three tests keep the rebuilt pipeline honest: a pass over the controllers puts exactly the bytes
on the wire that `pf-output`'s own output thread does
(`a_pass_sends_what_the_engines_output_thread_sends`), the transport refuses any address that
isn't this machine (`nothing_leaves_the_machine_whichever_sink_is_used`), and a pass allocates
nothing (`an_output_pass_allocates_nothing`). `tests/smoke.rs` runs every mode on a 2,000-pixel
show for a second, in CI, without asserting how long anything took.

### The sinks

- **`null`**: every packet is built, counted, and dropped. No sockets. This isolates the engine's
  own work.
- **`loopback`**: each controller gets its own UDP listener on `127.0.0.1`, and packets are sent
  to it through the engine's `UdpTransport` from a socket bound to `127.0.0.1`. One thread reads
  every listener and counts what arrives. Sending is real system calls; what happens after the
  socket is the operating system's loopback path, which is not a network card.

## The shows

A show is built for a pixel count: a house's worth of outlines, arches, trees, and small props, and
the rest in matrices. Larger shows are more matrix, as real ones are (a house only has so many
roof lines). The same count always gives the same show.

| Tier | Pixels | Props | Outlines and lines | Arches | Trees | Stars, circles, canes | Matrices | Matrix sizes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 20k | 20,000 | 48 | 4,698 in 27 | 1,000 in 5 | 5,000 in 4 | 1,110 in 8 | 8,192 in 4 | 1 of 96x48, 1 of 64x32, 2 of 48x16 |
| 100k | 100,000 | 123 | 10,098 in 60 | 3,000 in 15 | 17,000 in 20 | 2,830 in 20 | 67,072 in 8 | 1 of 256x128, 1 of 192x96, 1 of 128x64, 1 of 96x48, 4 of 48x16 |
| 200k | 200,000 | 168 | 12,994 in 76 | 4,000 in 20 | 24,000 in 23 | 3,870 in 27 | 155,136 in 22 | 2 of 256x128, 2 of 192x96, 3 of 128x64, 3 of 96x48, 4 of 64x32, 8 of 48x16 |

Props stand in bands (yard, matrices, roof lines), so a group's grid sees them spread out as a
real layout would. Strings are wired to controllers of 16 ports with up to 600 pixels a port, a
prop carrying on to the next port when one fills; every fifth string is wired from its far end,
arches and small props are GRB, and every other string controller runs at 60% brightness with a
gamma of 2.2. A matrix of 4,000 pixels or more has a controller to itself. Two string controllers
in three are sACN (510-channel universes, unicast); matrices of 16,000 pixels or more are DDP and
the ones between take turns.

| Tier | Controllers | sACN | Universes | DDP | DDP packets | Packets a pass | Bytes a pass | Groups (members) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 20k | 3 | 15,392 px on 2 | 92 | 4,608 px on 1 | 10 | 102 | 70 KB | All Outlines (27), All Arches (5), All Trees (4), All Accents (8), All Matrices (4), Whole House (48) |
| 100k | 8 | 31,008 px on 4 | 185 | 68,992 px on 4 | 146 | 331 | 317 KB | All Outlines (60), All Arches (15), All Trees (20), All Accents (20), All Matrices (8), Whole House (123) |
| 200k | 17 | 57,408 px on 8 | 343 | 142,592 px on 9 | 302 | 645 | 631 KB | All Outlines (76), All Arches (20), All Trees (23), All Accents (27), All Matrices (22), Whole House (168) |

Each show has six groups: one for each kind of prop and one for the whole house. They use
xLights' layouts: "minimal grid" (400 cells across) for the outlines, the matrices, and the whole
house; "horizontal per model" for the arches; "per model default" for the trees; "vertical per
model" for the small props.

## The sequences

Two sequences are built for each show, 90 seconds long at 40 frames a second, timed to a made-up
song at 120 beats a minute (a kick, a snare, hats, a bass line, and a pad, analyzed into an audio
track exactly as a real song is). Effects start and end on four-second phrases.

- **typical**: every prop has a row of effects suited to it, resting about one phrase in eight.
  Four or five rows in ten have a second, third, or fourth layer that comes and goes. Each group
  has a row that plays 15% to 40% of the time, the whole house 20%. Three effects in ten fade in
  and out, two in ten have sparkles, one in eight of those on a tree, matrix, or group is blurred,
  and one in seven has a setting that rises and falls over it (a third of those also sparkle with
  the music).
- **worst**: every prop has four layers of heavy effects for the whole sequence, and every group
  two more: eight effects on every pixel in every frame. Every other layer has sparkles, every
  other a curve, and a third of the bottom layers on trees, matrices, and groups are blurred.

Both use sixteen kinds of effect: On, Color Wash, Bars, Spiral, Pinwheel, Plasma, Butterfly,
Twinkle, Meteors, Fire, Ripple, Text, VU Meter (on the synthetic song), Pulse (on its beats and
its bass), Picture (a generated 256 x 256 PNG), and Dancer (on the matrices, to the beat). The
worst case leaves out On.

| Tier | Sequence | Length | Rows | Layered rows | Effects | Effects a frame (mean / most) | Layers a pixel (mean / most) | Blur | Sparkles | Curves | Fades |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 20k | typical | 90 s | 54 | 24 (up to 4) | 1222 | 68 / 74 | 1.70 / 2.78 | 33 | 214 | 150 | 352 |
| 20k | worst | 90 s | 54 | 54 (up to 4) | 2580 | 204 / 204 | 8.00 / 8.00 | 83 | 1290 | 1196 | 0 |
| 100k | typical | 90 s | 129 | 57 (up to 4) | 3004 | 168 / 181 | 1.82 / 2.73 | 85 | 596 | 380 | 858 |
| 100k | worst | 90 s | 129 | 129 (up to 4) | 6180 | 504 / 504 | 8.00 / 8.00 | 167 | 3090 | 2856 | 0 |
| 200k | typical | 90 s | 174 | 70 (up to 4) | 3922 | 221 / 239 | 1.85 / 2.84 | 104 | 815 | 505 | 1131 |
| 200k | worst | 90 s | 174 | 174 (up to 4) | 8340 | 684 / 684 | 8.00 / 8.00 | 231 | 4170 | 3825 | 0 |

"Layers a pixel" is how many times the average pixel is drawn in a frame: a pixel under an effect
on its prop and another on its group is drawn twice.

## Reading the results

**Frames.** A paced run plays the sequence against the clock, so each frame has a moment it is due.

- **Dropped**: the frame never reached the sink. The player was still drawing the one before when
  it came due, or the output thread was still sending when the next frame replaced it.
- **Late**: the player handed the frame to the output thread after the next frame was due.
- **On time**: frames neither dropped nor late, of those due.
- **Handed over after due**: from the frame being due to the player having drawn it and handed it
  over.
- **On the wire after due**: from the frame being due to the output thread finishing the first
  pass that carried it. This includes waiting for the output thread's next pass, up to 12.5 ms.
  Where in that 12.5 ms a frame lands depends on when the two threads happened to start: in these
  runs, as when the engine plays without music, a pass starts just after each frame boundary, so
  a frame that is ready within 12 ms goes out about 14 ms after it was due and a slower one about
  27 ms after. With music the two clocks are unrelated and the wait is anywhere in the range.
- **Jitter**: how far the gaps between frames reaching the wire stray from their mean (standard
  deviation). Frames leave on the output thread's 12.5 ms grid, so a frame that is ready close to
  a pass sometimes makes it and sometimes waits for the next: that shows up here.

**Times.** Every time is in milliseconds, as `p50 / p95 / p99 / max` over every frame (or pass)
measured. p99 is nearest-rank: one frame in a hundred was slower.

- **Player: one frame** is rendering plus the preview copies: what the player thread does for a
  frame, against a budget of 25 ms.
- **Output: one pass** is map, packetize, and send for every controller. The output thread makes a
  pass every 12.5 ms; a pass that takes longer delays the next one, and frames are only lost when
  a pass takes longer than 25 ms.
- **Whole frame** (flat out) is everything for one frame on one thread: render, preview copies,
  and one pass.

The same rendering takes longer in a paced run than flat out, and longer on the null sink, where
the process is nearly idle, than on loopback: 4.8, 2.4, and 1.9 ms for the worst case at 20k. The
likely reason, not measured here, is that the operating system gives a thread that sleeps most of
each frame a slower core or a lower clock speed. Playback gets the same treatment, so the paced
figures are the ones that count for playback and the flat-out ones for what the code costs.

**40 FPS.** A run **passes** when at least 99 frames in 100 were on time, the player's frame and
the output thread's pass each fit 25 ms at the 99th percentile, and no packet was dropped at a
full send buffer or failed to send. One rule for everything: the 99th percentile within the frame
period. A flat-out run has no clock to keep; its frames are "late" when they took over 25 ms.

**Everything else.**

- **CPU cores** is the process's CPU time over wall time: 1.00 is one core kept busy. On the
  loopback sink it includes the thread that reads the listeners, which on a real network would be
  the controllers' work.
- **Threads** are the pipeline's (player, output, preview reader, and the listeners' reader on
  loopback) and the process's as the system counts them.
- **Peak RSS** is the most memory the process held, building the show and sequence included.
  **Peak heap**, **allocations a frame**, and **KB allocated a frame** come from a counting
  allocator in the benchmark binary (a few atomic additions for each allocation); in a paced run
  they include the preview reader's copy of the frame, 20 times a second.
- **Lost on loopback** is packets sent that the listeners never counted: the operating system
  dropped them between the two sockets. Flat out, that is the listeners falling behind.
- **First frame** and **warm-up** are the frames before measuring starts. The first frame builds
  every target's pixel buffer.
- **Channels lit** is how much of the frame was not black, on average: proof there was something
  to draw.
- **Load average** is the machine's one-minute load when the run began: what else it was doing.

## Baseline

Measured on 11 October 2026 at commit `086b228` with the command above (`--all --engine-check --effects
--calm 5`), 60 seconds a run after 2 of warm-up.

|  |  |
| --- | --- |
| CPU | Apple M5 Pro |
| Cores | 15 |
| Memory | 48 GB |
| OS | macOS 26.6.2 |
| Compiler | rustc 1.98.1 (48a229cea 2026-09-01) |
| Build | release (opt-level 3), aarch64-apple-darwin |

The machine was not idle. Other builds came and went, and an editor's indexer and an endpoint
security scanner ran throughout. The
"Load average" column says how busy it was as each run began; a run that began above 5 had waited
two minutes for quiet and gone ahead anyway. Treat the numbers as a floor, not a best case, and
see [three runs of the heavier cells](#three-runs-of-the-heavier-cells) for how much they move.

### In real time

| Tier | Sequence | Sink | FPS | Dropped | Player: one frame, ms (p50 / p95 / p99 / max) | Output: one pass, ms (p50 / p95 / p99 / max) | Handed over after due, ms (p50 / p95 / p99 / max) | Late | On time | On the wire after due, ms (p50 / p95 / p99 / max) | Jitter ms | CPU cores | Peak RSS | 40 FPS |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 20k | typical | null | 40.00 | 0 of 2400 | 0.45 / 1.26 / 1.68 / 1.88 | 0.024 / 0.11 / 0.11 / 0.19 | 2.11 / 3.66 / 4.29 / 5.08 | 0 | 100.00% | 14.1 / 14.3 / 14.5 / 15.0 | 4.47 | 0.03 | 20 MB | PASS |
| 20k | typical | loopback | 40.00 | 0 of 2400 | 0.41 / 0.76 / 0.84 / 1.50 | 1.99 / 2.46 / 2.55 / 2.99 | 1.67 / 3.42 / 3.90 / 4.35 | 0 | 100.00% | 15.3 / 16.1 / 16.3 / 16.7 | 5.45 | 0.37 | 20 MB | PASS |
| 20k | worst | null | 40.00 | 0 of 2400 | 4.84 / 6.91 / 8.17 / 10.2 | 0.069 / 0.10 / 0.11 / 0.24 | 6.64 / 8.83 / 10.1 / 12.6 | 0 | 100.00% | 14.1 / 14.5 / 14.5 / 16.5 | 0.78 | 0.20 | 33 MB | PASS |
| 20k | worst | loopback | 40.00 | 0 of 2400 | 2.38 / 3.08 / 3.36 / 11.2 | 1.71 / 2.04 / 2.32 / 22.4 | 4.39 / 5.70 / 6.14 / 18.6 | 0 | 100.00% | 15.3 / 15.8 / 16.0 / 37.0 | 1.08 | 0.41 | 33 MB | PASS |
| 100k | typical | null | 40.00 | 0 of 2400 | 2.23 / 5.06 / 6.43 / 13.5 | 0.16 / 0.29 / 0.36 / 0.75 | 4.69 / 7.03 / 8.40 / 15.6 | 0 | 100.00% | 14.2 / 14.6 / 14.7 / 26.7 | 0.91 | 0.12 | 46 MB | PASS |
| 100k | typical | loopback | 40.00 | 0 of 2400 | 2.78 / 4.81 / 5.19 / 8.20 | 7.25 / 8.26 / 8.61 / 21.7 | 5.06 / 7.11 / 7.75 / 10.3 | 0 | 100.00% | 19.6 / 20.8 / 21.2 / 29.2 | 0.73 | 0.83 | 45 MB | PASS |
| 100k | worst | null | 40.00 | 0 of 2400 | 11.9 / 15.0 / 16.3 / 20.3 | 0.15 / 0.24 / 0.37 / 0.54 | 12.8 / 17.4 / 18.8 / 22.9 | 0 | 100.00% | 14.3 / 27.0 / 27.1 / 27.2 | 3.91 | 0.44 | 82 MB | PASS |
| 100k | worst | loopback | 40.00 | 0 of 2400 | 9.11 / 12.6 / 13.9 / 22.3 | 4.87 / 7.89 / 8.46 / 31.6 | 10.4 / 13.8 / 16.5 / 27.6 | 1 | 99.96% | 17.7 / 32.8 / 33.4 / 45.6 | 2.88 | 1.06 | 81 MB | PASS |
| 200k | typical | null | 40.00 | 0 of 2400 | 5.23 / 8.79 / 9.08 / 21.1 | 0.33 / 0.56 / 0.68 / 1.02 | 6.95 / 9.92 / 10.6 / 22.8 | 0 | 100.00% | 14.2 / 14.6 / 14.8 / 26.8 | 0.74 | 0.24 | 77 MB | PASS |
| 200k | typical | loopback | 39.92 | 5 of 2400 | 3.55 / 6.97 / 7.70 / 12.0 | 8.29 / 14.2 / 18.2 / 79.2 | 5.72 / 8.84 / 10.1 / 17.0 | 0 | 99.79% | 15.8 / 30.4 / 40.0 / 52.1 | 4.42 | 1.39 | 79 MB | PASS |
| 200k | worst | null | 40.00 | 0 of 2400 | 15.6 / 18.3 / 20.2 / 21.2 | 0.23 / 0.30 / 0.31 / 0.50 | 18.0 / 20.4 / 21.7 / 22.9 | 0 | 100.00% | 26.7 / 27.1 / 27.2 / 27.2 | 0.48 | 0.66 | 101 MB | PASS |
| 200k | worst | loopback | 40.00 | 0 of 2400 | 16.0 / 18.6 / 21.1 / 24.3 | 8.28 / 8.83 / 11.3 / 16.0 | 18.3 / 20.6 / 22.6 / 26.5 | 3 | 99.88% | 33.9 / 34.4 / 37.2 / 49.3 | 0.73 | 1.86 | 101 MB | PASS |

| Tier | Sequence | Sink | Render (p50 / p95 / p99 / max) | Preview copies (p50 / p99) | Map (p50 / p99) | Packetize (p50 / p99) | Send (p50 / p95 / p99 / max) | Passes a second | Late passes | Draw starts after due (p50 / p95 / p99 / max) | Preview read (p50 / p99) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 20k | typical | null | 0.45 / 1.25 / 1.67 / 1.87 | 0.002 / 0.006 | 0.022 / 0.10 | 0.001 / 0.008 | 0.000 / 0.002 / 0.002 / 0.020 | 80.0 of 80 | 0 | 1.59 / 2.93 / 3.34 / 3.50 | 0.002 / 0.011 |
| 20k | typical | loopback | 0.41 / 0.76 / 0.84 / 1.50 | 0.002 / 0.003 | 0.029 / 0.059 | 0.002 / 0.005 | 1.95 / 2.42 / 2.51 / 2.95 | 80.0 of 80 | 0 | 1.20 / 2.86 / 3.36 / 3.50 | 0.002 / 0.009 |
| 20k | worst | null | 4.83 / 6.91 / 8.16 / 10.2 | 0.003 / 0.006 | 0.063 / 0.10 | 0.004 / 0.009 | 0.001 / 0.002 / 0.002 / 0.029 | 80.0 of 80 | 0 | 1.80 / 2.86 / 3.18 / 5.40 | 0.005 / 0.016 |
| 20k | worst | loopback | 2.38 / 3.08 / 3.36 / 11.2 | 0.002 / 0.003 | 0.026 / 0.058 | 0.002 / 0.005 | 1.69 / 2.00 / 2.29 / 22.4 | 80.0 of 80 | 0 | 1.79 / 3.25 / 3.44 / 10.6 | 0.002 / 0.009 |
| 100k | typical | null | 2.22 / 5.05 / 6.41 / 13.5 | 0.008 / 0.039 | 0.15 / 0.33 | 0.009 / 0.040 | 0.001 / 0.003 / 0.015 / 0.045 | 80.0 of 80 | 0 | 2.23 / 3.28 / 3.48 / 3.53 | 0.006 / 0.038 |
| 100k | typical | loopback | 2.76 / 4.80 / 5.17 / 8.19 | 0.008 / 0.046 | 0.17 / 0.25 | 0.012 / 0.043 | 7.07 / 8.06 / 8.39 / 21.5 | 80.0 of 80 | 0 | 2.08 / 3.17 / 3.46 / 6.50 | 0.006 / 0.046 |
| 100k | worst | null | 11.9 / 15.0 / 16.3 / 20.2 | 0.008 / 0.035 | 0.13 / 0.35 | 0.009 / 0.041 | 0.001 / 0.003 / 0.015 / 0.055 | 80.0 of 80 | 0 | 1.27 / 2.99 / 3.28 / 9.11 | 0.006 / 0.035 |
| 100k | worst | loopback | 9.10 / 12.6 / 13.9 / 22.3 | 0.008 / 0.014 | 0.11 / 0.20 | 0.009 / 0.025 | 4.75 / 7.72 / 8.28 / 31.5 | 80.0 of 80 | 1 | 1.15 / 2.16 / 3.16 / 9.15 | 0.006 / 0.047 |
| 200k | typical | null | 5.20 / 8.77 / 9.05 / 21.1 | 0.021 / 0.038 | 0.31 / 0.64 | 0.019 / 0.048 | 0.003 / 0.005 / 0.007 / 0.012 | 80.0 of 80 | 0 | 1.78 / 2.93 / 3.18 / 3.50 | 0.015 / 0.041 |
| 200k | typical | loopback | 3.54 / 6.95 / 7.68 / 12.0 | 0.016 / 0.031 | 0.22 / 0.34 | 0.015 / 0.038 | 8.06 / 13.8 / 17.9 / 78.9 | 78.6 of 80 | 60 | 2.13 / 3.07 / 3.47 / 9.66 | 0.010 / 0.062 |
| 200k | worst | null | 15.5 / 18.3 / 20.2 / 21.2 | 0.016 / 0.018 | 0.21 / 0.29 | 0.015 / 0.029 | 0.002 / 0.003 / 0.004 / 0.014 | 80.0 of 80 | 0 | 2.29 / 3.05 / 3.21 / 3.38 | 0.009 / 0.044 |
| 200k | worst | loopback | 16.0 / 18.6 / 21.0 / 24.3 | 0.016 / 0.030 | 0.22 / 0.27 | 0.015 / 0.026 | 8.05 / 8.59 / 11.0 / 15.7 | 80.0 of 80 | 1 | 2.15 / 2.93 / 3.14 / 3.41 | 0.010 / 0.069 |

### Three runs of the heavier cells

Run 1 is the row in the table above; runs 2 and 3 were made afterwards, each as
`pf-bench --tier T --variant V --mode paced --sink S --seconds 60 --calm 5`.

| Tier | Sequence | Sink | Run | FPS | Dropped | Late | On time | Player: one frame, ms (p50 / p95 / p99) | Output: one pass, ms (p50 / p95 / p99) | Load average | 40 FPS |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 100k | worst | null | 1 | 40.00 | 0 | 0 | 100.00% | 11.9 / 15.0 / 16.3 | 0.15 / 0.24 / 0.37 | 4.2 | PASS |
| 100k | worst | null | 2 | 40.00 | 0 | 0 | 100.00% | 11.8 / 13.9 / 14.8 | 0.16 / 0.23 / 0.27 | 2.4 | PASS |
| 100k | worst | null | 3 | 40.00 | 0 | 0 | 100.00% | 10.3 / 11.1 / 11.6 | 0.16 / 0.20 / 0.20 | 4.3 | PASS |
| 100k | worst | loopback | 1 | 40.00 | 0 | 1 | 99.96% | 9.11 / 12.6 / 13.9 | 4.87 / 7.89 / 8.46 | 4.3 | PASS |
| 100k | worst | loopback | 2 | 39.98 | 1 | 0 | 99.96% | 13.2 / 15.6 / 16.9 | 7.44 / 8.14 / 8.50 | 4.1 | PASS |
| 100k | worst | loopback | 3 | 40.00 | 0 | 0 | 100.00% | 8.78 / 9.79 / 10.4 | 4.67 / 5.16 / 5.94 | 2.2 | PASS |
| 200k | typical | null | 1 | 40.00 | 0 | 0 | 100.00% | 5.23 / 8.79 / 9.08 | 0.33 / 0.56 / 0.68 | 4.3 | PASS |
| 200k | typical | null | 2 | 40.00 | 0 | 0 | 100.00% | 4.60 / 8.64 / 9.29 | 0.32 / 0.43 / 0.51 | 4.8 | PASS |
| 200k | typical | null | 3 | 40.00 | 0 | 0 | 100.00% | 6.08 / 8.54 / 9.02 | 0.42 / 0.64 / 0.83 | 4.3 | PASS |
| 200k | typical | loopback | 1 | 39.92 | 5 | 0 | 99.79% | 3.55 / 6.97 / 7.70 | 8.29 / 14.2 / 18.2 | 3.6 | PASS |
| 200k | typical | loopback | 2 | 40.00 | 0 | 0 | 100.00% | 5.76 / 11.6 / 12.2 | 14.7 / 15.9 / 16.7 | 4.5 | PASS |
| 200k | typical | loopback | 3 | 39.77 | 14 | 0 | 99.42% | 3.44 / 6.98 / 7.28 | 8.19 / 13.9 / 19.0 | 3.1 | PASS |
| 200k | worst | null | 1 | 40.00 | 0 | 0 | 100.00% | 15.6 / 18.3 / 20.2 | 0.23 / 0.30 / 0.31 | 1.9 | PASS |
| 200k | worst | null | 2 | 40.00 | 0 | 8 | 99.67% | 15.8 / 20.2 / 21.9 | 0.23 / 0.33 / 0.53 | 4.4 | PASS |
| 200k | worst | null | 3 | 40.00 | 0 | 6 | 99.75% | 16.1 / 20.1 / 22.3 | 0.24 / 0.32 / 0.38 | 4.9 | PASS |
| 200k | worst | loopback | 1 | 40.00 | 0 | 3 | 99.88% | 16.0 / 18.6 / 21.1 | 8.28 / 8.83 / 11.3 | 1.9 | PASS |
| 200k | worst | loopback | 2 | 40.00 | 0 | 7 | 99.71% | 16.1 / 19.3 / 23.5 | 8.30 / 9.82 / 13.8 | 3.8 | PASS |
| 200k | worst | loopback | 3 | 38.98 | 61 | 580 | 73.29% | 17.6 / 29.7 / 36.4 | 8.91 / 15.4 / 15.8 | 3.1 | FAIL: frames late or dropped, over budget |

### Flat out

| Tier | Sequence | Sink | FPS | Render (p50 / p95 / p99 / max) | Preview copies (p50 / p99) | Map (p50 / p99) | Packetize (p50 / p99) | Send (p50 / p99) | Whole frame (p50 / p95 / p99 / max) | Frames over 25 ms | CPU cores | Allocations drawing a frame | Allocations sending it |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 20k | typical | null | 2547 | 0.34 / 0.56 / 0.60 / 1.18 | 0.002 / 0.006 | 0.021 / 0.027 | 0.001 / 0.002 | 0.000 / 0.000 | 0.36 / 0.59 / 0.62 / 1.21 | 0 of 2400 | 1.00 | 37 | 0 |
| 20k | typical | loopback | 608 | 0.33 / 0.56 / 0.59 / 1.11 | 0.002 / 0.002 | 0.022 / 0.027 | 0.001 / 0.002 | 1.24 / 1.59 | 1.61 / 1.89 / 2.03 / 3.20 | 0 of 2400 | 1.24 | 37 | 0 |
| 20k | worst | null | 505 | 1.94 / 2.25 / 2.33 / 4.39 | 0.002 / 0.002 | 0.021 / 0.028 | 0.001 / 0.002 | 0.000 / 0.000 | 1.96 / 2.27 / 2.36 / 4.41 | 0 of 2400 | 1.00 | 305 | 0 |
| 20k | worst | loopback | 278 | 2.03 / 2.33 / 2.45 / 4.41 | 0.002 / 0.002 | 0.022 / 0.025 | 0.001 / 0.002 | 1.54 / 1.61 | 3.59 / 3.87 / 4.00 / 5.94 | 0 of 2400 | 1.43 | 305 | 0 |
| 100k | typical | null | 561 | 1.48 / 2.61 / 2.70 / 5.12 | 0.008 / 0.009 | 0.10 / 0.13 | 0.006 / 0.009 | 0.001 / 0.002 | 1.61 / 2.72 / 2.82 / 5.24 | 0 of 2400 | 1.00 | 92 | 0 |
| 100k | typical | loopback | 160 | 1.58 / 2.75 / 2.81 / 5.87 | 0.008 / 0.010 | 0.11 / 0.17 | 0.007 / 0.015 | 4.14 / 7.27 | 6.05 / 7.86 / 9.30 / 11.1 | 0 of 2400 | 1.33 | 92 | 0 |
| 100k | worst | null | 117 | 8.42 / 9.73 / 10.6 / 13.1 | 0.008 / 0.010 | 0.10 / 0.14 | 0.007 / 0.011 | 0.001 / 0.001 | 8.54 / 9.85 / 10.8 / 13.2 | 0 of 2400 | 1.00 | 783 | 0 |
| 100k | worst | loopback | 77 | 8.49 / 9.32 / 9.83 / 16.2 | 0.008 / 0.010 | 0.11 / 0.12 | 0.008 / 0.012 | 4.57 / 5.32 | 13.0 / 14.1 / 14.8 / 23.3 | 0 of 2400 | 1.28 | 783 | 0 |
| 200k | typical | null | 262 | 3.16 / 6.55 / 6.69 / 11.7 | 0.016 / 0.019 | 0.21 / 0.24 | 0.013 / 0.018 | 0.002 / 0.003 | 3.40 / 6.78 / 6.94 / 11.9 | 0 of 2400 | 1.00 | 136 | 0 |
| 200k | typical | loopback | 84 | 3.22 / 6.62 / 6.71 / 12.4 | 0.016 / 0.019 | 0.22 / 0.27 | 0.015 / 0.024 | 7.97 / 10.1 | 11.4 / 14.5 / 15.0 / 20.4 | 0 of 2400 | 1.52 | 136 | 0 |
| 200k | worst | null | 64 | 15.2 / 17.3 / 19.6 / 22.8 | 0.016 / 0.018 | 0.21 / 0.25 | 0.015 / 0.018 | 0.002 / 0.003 | 15.4 / 17.5 / 19.9 / 23.1 | 0 of 2400 | 1.00 | 1114 | 0 |
| 200k | worst | loopback | 41 | 15.5 / 17.6 / 20.2 / 21.0 | 0.015 / 0.019 | 0.21 / 0.27 | 0.016 / 0.024 | 8.21 / 10.8 | 23.9 / 27.3 / 29.6 / 33.3 | 509 of 2400 | 1.30 | 1114 | 0 |

### Network, memory, and warm-up

| Tier | Sequence | Mode | Sink | Packets/s | Mbit/s | Send buffer full / failed | Lost on loopback | Threads (pipeline / process) | Peak RSS | Peak heap | Allocations a frame | KB allocated a frame | First frame ms | Warm-up ms (mean / max) | Channels lit | Load average |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 20k | typical | asap | null | 259824 | 1461 | 0 / 0 | - | 1 / 1 | 20 MB | 8 MB | 37 | 52 | 0.4 | 0.3 / 0.4 | 45% | 4.2 |
| 20k | typical | asap | loopback | 61990 | 349 | 0 / 0 | 0.01% | 2 / 2 | 20 MB | 8 MB | 37 | 52 | 1.9 | 1.6 / 1.9 | 45% | 4.2 |
| 20k | typical | paced | null | 8160 | 46 | 0 / 0 | - | 3 / 3 | 20 MB | 9 MB | 37 | 79 | 0.3 | 0.3 / 0.3 | 44% | 4.2 |
| 20k | typical | paced | loopback | 8160 | 46 | 0 / 0 | 0.00% | 4 / 4 | 20 MB | 9 MB | 37 | 79 | 0.3 | 0.3 / 0.5 | 44% | 2.3 |
| 20k | typical | engine | null | 8158 | 46 | 0 / 0 | - | 3 / 3 | 24 MB | 8 MB | - | - | - | - | - | 3.9 |
| 20k | worst | asap | null | 51556 | 290 | 0 / 0 | - | 1 / 1 | 31 MB | 15 MB | 305 | 207 | 3.7 | 1.9 / 3.7 | 79% | 2.3 |
| 20k | worst | asap | loopback | 28338 | 159 | 0 / 0 | 0.00% | 2 / 2 | 30 MB | 15 MB | 305 | 207 | 5.3 | 3.4 / 5.3 | 79% | 2.2 |
| 20k | worst | paced | null | 8160 | 46 | 0 / 0 | - | 3 / 3 | 33 MB | 16 MB | 305 | 234 | 3.7 | 4.5 / 7.7 | 78% | 2.7 |
| 20k | worst | paced | loopback | 8160 | 46 | 0 / 0 | 0.01% | 4 / 4 | 33 MB | 16 MB | 305 | 234 | 3.8 | 2.6 / 3.8 | 78% | 2.1 |
| 20k | worst | engine | null | 8160 | 46 | 0 / 0 | - | 3 / 3 | 46 MB | 17 MB | - | - | - | - | - | 3.4 |
| 100k | typical | asap | null | 185706 | 1458 | 0 / 0 | - | 1 / 1 | 42 MB | 24 MB | 92 | 349 | 1.5 | 1.2 / 1.5 | 46% | 2.6 |
| 100k | typical | asap | loopback | 52838 | 415 | 0 / 0 | 0.00% | 2 / 2 | 42 MB | 24 MB | 92 | 349 | 6.6 | 5.3 / 6.8 | 46% | 2.6 |
| 100k | typical | paced | null | 26480 | 208 | 0 / 0 | - | 3 / 3 | 46 MB | 26 MB | 93 | 485 | 1.4 | 1.2 / 2.7 | 45% | 3.4 |
| 100k | typical | paced | loopback | 26478 | 208 | 0 / 0 | 0.00% | 4 / 4 | 45 MB | 26 MB | 93 | 488 | 2.1 | 2.0 / 2.4 | 45% | 4.5 |
| 100k | typical | engine | null | 26480 | 208 | 0 / 0 | - | 3 / 3 | 56 MB | 26 MB | - | - | - | - | - | 4.8 |
| 100k | worst | asap | null | 38690 | 304 | 0 / 0 | - | 1 / 1 | 78 MB | 55 MB | 783 | 1383 | 18.2 | 9.6 / 18.2 | 79% | 3.4 |
| 100k | worst | asap | loopback | 25364 | 199 | 0 / 0 | 0.04% | 2 / 2 | 78 MB | 55 MB | 783 | 1383 | 22.5 | 14.4 / 22.5 | 79% | 3.5 |
| 100k | worst | paced | null | 26485 | 208 | 0 / 0 | - | 3 / 3 | 82 MB | 57 MB | 783 | 1519 | 17.6 | 11.0 / 17.6 | 78% | 4.2 |
| 100k | worst | paced | loopback | 26472 | 208 | 0 / 0 | 0.00% | 4 / 4 | 81 MB | 57 MB | 783 | 1520 | 18.0 | 10.5 / 18.0 | 78% | 4.3 |
| 100k | worst | engine | null | 26485 | 208 | 0 / 0 | - | 3 / 3 | 99 MB | 39 MB | - | - | - | - | - | 28.3 |
| 200k | typical | asap | null | 168912 | 1354 | 0 / 0 | - | 1 / 1 | 77 MB | 50 MB | 136 | 584 | 4.6 | 2.8 / 4.6 | 48% | 4.7 |
| 200k | typical | asap | loopback | 54239 | 435 | 0 / 0 | 0.00% | 2 / 2 | 74 MB | 50 MB | 136 | 584 | 13.5 | 10.6 / 13.5 | 48% | 4.2 |
| 200k | typical | paced | null | 51601 | 414 | 0 / 0 | - | 3 / 3 | 77 MB | 51 MB | 136 | 855 | 3.9 | 4.9 / 7.6 | 48% | 4.3 |
| 200k | typical | paced | loopback | 50690 | 406 | 0 / 0 | 0.00% | 4 / 4 | 79 MB | 51 MB | 136 | 858 | 3.9 | 2.7 / 3.9 | 48% | 3.6 |
| 200k | typical | engine | null | 51600 | 414 | 0 / 0 | - | 3 / 3 | 89 MB | 52 MB | - | - | - | - | - | 4.8 |
| 200k | worst | asap | null | 41143 | 330 | 0 / 0 | - | 1 / 1 | 98 MB | 61 MB | 1114 | 2906 | 37.4 | 19.5 / 37.4 | 78% | 2.4 |
| 200k | worst | asap | loopback | 26498 | 212 | 0 / 0 | 0.00% | 2 / 2 | 98 MB | 61 MB | 1114 | 2906 | 49.3 | 29.0 / 49.3 | 78% | 2.4 |
| 200k | worst | paced | null | 51602 | 414 | 0 / 0 | - | 3 / 3 | 101 MB | 63 MB | 1114 | 3175 | 37.8 | 19.8 / 37.8 | 78% | 1.9 |
| 200k | worst | paced | loopback | 51589 | 414 | 0 / 0 | 0.00% | 4 / 4 | 101 MB | 63 MB | 1114 | 3180 | 36.9 | 20.4 / 36.9 | 78% | 1.9 |
| 200k | worst | engine | null | 51589 | 414 | 0 / 0 | - | 3 / 3 | 141 MB | 67 MB | - | - | - | - | - | 3.4 |

### The engine itself

| Tier | Sequence | Sink | FPS | Dropped | Late | Drawn and handed over after due, ms (p50 / p95 / p99 / max) | Passes a second | CPU cores | Peak RSS | 40 FPS |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 20k | typical | null | 40.00 | 0 of 2400 | 0 | 1.00 / 2.00 / 3.00 / 3.00 | 80.0 of 80 | 0.07 | 24 MB | PASS |
| 20k | worst | null | 40.00 | 0 of 2400 | 0 | 3.00 / 5.00 / 5.00 / 6.00 | 80.0 of 80 | 0.14 | 46 MB | PASS |
| 100k | typical | null | 40.00 | 0 of 2400 | 0 | 2.00 / 5.00 / 6.00 / 9.00 | 80.0 of 80 | 0.14 | 56 MB | PASS |
| 100k | worst | null | 40.00 | 0 of 2400 | 1 | 9.00 / 14.0 / 18.0 / 33.0 | 80.0 of 80 | 0.42 | 99 MB | PASS |
| 200k | typical | null | 40.00 | 0 of 2400 | 0 | 4.00 / 7.00 / 8.00 / 12.0 | 80.0 of 80 | 0.21 | 89 MB | PASS |
| 200k | worst | null | 40.00 | 0 of 2400 | 0 | 17.0 / 19.0 / 21.0 / 24.0 | 80.0 of 80 | 0.69 | 141 MB | PASS |

- Played by the engine itself, without the music or the picture (it reads those from files).

"Drawn and handed over after due" is the paced runs' "handed over after due", seen from
outside. The two agree to within what the missing music and picture make and the engine's
whole-millisecond clock.

### Effects at 200k

An empty frame (clearing and writing every pixel) takes 0.24 ms; every figure below is on top of that.

#### Each kind alone

| Effect | On every prop, ms a frame | ns a pixel | Allocations a frame | KB allocated a frame | On the whole-house group, ms a frame | ns a pixel |
| --- | --- | --- | --- | --- | --- | --- |
| Fire | 9.84 | 49.2 | 168 | 168 | 0.81 | 4.0 |
| Plasma | 4.14 | 20.7 | 336 | 56 | 3.37 | 16.9 |
| Twinkle | 2.58 | 12.9 | 0 | 0 | 2.56 | 12.8 |
| Pinwheel | 2.08 | 10.4 | 336 | 3140 | 1.72 | 8.6 |
| Butterfly | 1.88 | 9.4 | 0 | 0 | 1.49 | 7.4 |
| Meteors | 1.81 | 9.1 | 168 | 63 | 0.96 | 4.8 |
| Picture | 1.35 | 6.8 | 401 | 177 | 0.90 | 4.5 |
| Dancer | 1.01 | 5.1 | 168 | 3131 | 0.62 | 3.1 |
| Text | 0.89 | 4.5 | 1680 | 91 | 0.64 | 3.2 |
| Spiral | 0.87 | 4.4 | 0 | 0 | 0.65 | 3.3 |
| VU Meter | 0.83 | 4.1 | 224 | 3138 | 0.69 | 3.4 |
| Bars | 0.81 | 4.0 | 0 | 0 | 0.66 | 3.3 |
| Color Wash | 0.80 | 4.0 | 0 | 0 | 0.76 | 3.8 |
| Ripple | 0.73 | 3.7 | 0 | 0 | 0.57 | 2.8 |
| Pulse | 0.67 | 3.4 | 0 | 0 | 0.57 | 2.9 |
| On | 0.61 | 3.1 | 0 | 0 | 0.59 | 2.9 |

#### Blur, sparkles, and curves (on every prop)

| Effect | ms a frame | ns a pixel |
| --- | --- | --- |
| Plasma | 3.95 | 19.7 |
| Plasma with sparkles | 4.36 | 21.8 |
| Plasma with blur 2 | 5.88 | 29.4 |
| Plasma with blur 8 | 6.29 | 31.5 |
| Plasma with a curve on its speed | 3.96 | 19.8 |

#### The typical sequence, first 60 s: 3.24 ms a frame

| Effect | Effects | Layers a pixel | Alone, ms a frame | Share | ns a pixel drawn |
| --- | --- | --- | --- | --- | --- |
| Twinkle | 358 | 0.323 | 0.82 | 25% | 12.7 |
| Plasma | 32 | 0.103 | 0.40 | 12% | 19.4 |
| Meteors | 325 | 0.160 | 0.35 | 11% | 10.9 |
| Ripple | 344 | 0.311 | 0.26 | 8% | 4.1 |
| Fire | 34 | 0.056 | 0.25 | 8% | 22.6 |
| Color Wash | 364 | 0.174 | 0.25 | 8% | 7.1 |
| Pinwheel | 76 | 0.061 | 0.17 | 5% | 14.0 |
| Bars | 321 | 0.168 | 0.16 | 5% | 4.8 |
| Butterfly | 66 | 0.065 | 0.13 | 4% | 10.0 |
| Spiral | 74 | 0.090 | 0.087 | 3% | 4.8 |
| Picture | 17 | 0.063 | 0.083 | 3% | 6.6 |
| Dancer | 16 | 0.053 | 0.078 | 2% | 7.4 |
| Text | 15 | 0.072 | 0.074 | 2% | 5.1 |
| Pulse | 315 | 0.095 | 0.072 | 2% | 3.8 |
| VU Meter | 15 | 0.053 | 0.059 | 2% | 5.6 |
| On | 169 | 0.014 | 0.012 | 0% | 4.1 |
| **All kinds, added up** | 2541 | 1.859 | 3.25 | 100% |  |

Prop rows alone take 2.40 ms a frame and group rows alone 0.82 ms. Of the whole sequence's time, blur accounts for 0.34 ms a frame, sparkles for 0.100 ms, and curves on other settings for 0.026 ms (what the sequence saves without them). Drawing it allocates 133 times a frame, 583 KB in all.

#### The worst sequence, first 60 s: 15.5 ms a frame

| Effect | Effects | Layers a pixel | Alone, ms a frame | Share | ns a pixel drawn |
| --- | --- | --- | --- | --- | --- |
| Plasma | 678 | 1.108 | 4.33 | 28% | 19.5 |
| Butterfly | 678 | 1.124 | 2.24 | 15% | 10.0 |
| Meteors | 678 | 1.050 | 1.77 | 11% | 8.4 |
| Twinkle | 614 | 0.722 | 1.68 | 11% | 11.6 |
| Fire | 156 | 0.354 | 1.52 | 10% | 21.4 |
| Ripple | 678 | 1.071 | 0.89 | 6% | 4.2 |
| Spiral | 186 | 0.962 | 0.88 | 6% | 4.6 |
| Pinwheel | 156 | 0.344 | 0.72 | 5% | 10.4 |
| Picture | 64 | 0.292 | 0.37 | 2% | 6.4 |
| Dancer | 64 | 0.278 | 0.34 | 2% | 6.1 |
| Text | 64 | 0.274 | 0.28 | 2% | 5.0 |
| VU Meter | 64 | 0.264 | 0.26 | 2% | 4.9 |
| Bars | 492 | 0.052 | 0.068 | 0% | 6.5 |
| Color Wash | 492 | 0.052 | 0.062 | 0% | 6.0 |
| Pulse | 492 | 0.052 | 0.047 | 0% | 4.5 |
| **All kinds, added up** | 5556 | 8.000 | 15.5 | 100% |  |

Prop rows alone take 8.25 ms a frame and group rows alone 6.90 ms. Of the whole sequence's time, blur accounts for 0.51 ms a frame, sparkles for 1.30 ms, and curves on other settings for 0.16 ms (what the sequence saves without them). Drawing it allocates 1111 times a frame, 2920 KB in all.

## What the baseline says

### Does it hold 40 frames a second?

| Tier | Typical sequence | Worst case |
| --- | --- | --- |
| 20k | **Pass.** A frame takes the player 0.8 to 1.7 ms at the 99th percentile: 7% of its 25 ms at most. | **Pass.** 3.4 to 8.2 ms. |
| 100k | **Pass.** 5.2 to 6.4 ms: a quarter of the budget. | **Pass**, six runs of six. 10.4 to 16.9 ms; one frame dropped and one late in 14,400. |
| 200k | **Pass.** Null sink: three runs of three with nothing dropped or late, 9.0 to 9.3 ms. Loopback: three of three, but 5 and 14 frames of 2,400 dropped in two of them, lost in the output thread while it was sending. | **Marginal.** Null sink: three of three, nothing dropped, up to 8 frames late, 20.2 to 22.3 ms of 25. Loopback: two of three; the third dropped 61 frames and delivered 580 late. |

Flat out, with nothing else in the way, a frame's whole work (render, preview copies, one pass to
the null sink) takes this much of the 25 ms at the 99th percentile:

| Tier | Typical | Worst case |
| --- | --- | --- |
| 20k | 0.62 ms (2%), 2,547 frames a second | 2.36 ms (9%), 505 |
| 100k | 2.82 ms (11%), 561 | 10.8 ms (43%), 117 |
| 200k | 6.94 ms (28%), 262 | 19.9 ms (80%), 64; slowest frame 23.1 ms |

### Where the time goes

**Rendering is nearly all of it, on one thread.** Flat out on the null sink at 200k, a typical
frame is 3.16 ms of rendering, 0.21 ms of mapping, 0.013 ms of packetizing, and 0.016 ms of
preview copies: rendering is 93% of the frame, and 99% in the worst case (15.2 of 15.4 ms). The
shares are the same at 20k and 100k. Rendering runs on a single core: a paced worst-case run at
200k keeps 0.66 of one core busy while fourteen sit idle.

**No one effect dominates; the cost is pixels times layers.** At 200k the typical sequence draws
each pixel 1.86 times a frame for 3.24 ms; the worst case draws each 8 times for 15.5 ms. That is 9
to 10 ns for each pixel of each layer either way. The effects range from 3 to 4 ns a pixel (On,
Pulse, Ripple, Bars, Color Wash) to 13 (Twinkle) and 20 (Plasma). In the typical sequence Twinkle
is the largest share (25%) only because it is on the most pixels; in the worst case Plasma is
(28%). Group rows cost about what prop rows do for the same pixels: in the worst case, with four
layers of each on every pixel, the six group rows are 6.9 ms and the 168 prop rows 8.3 ms.

**Fire is the one outlier.** Alone on every prop it costs 49 ns a pixel, more than twice the next
effect, but only 4 ns on the whole-house group. Each Fire effect simulates its 32 x 32 grid from
cold for 56 steps every frame, whatever the size of what it is drawn on: about 0.06 ms an effect,
9.8 ms for 168 of them. It is 8% of the typical sequence's time with 0.056 layers a pixel.

**Blur and sparkles are modest.** Blur adds about 10 ns a pixel to an effect (Plasma 19.7 to
29.4), 0.34 ms a frame to the typical sequence and 0.51 ms to the worst. Sparkles add 2 ns a
pixel, 0.10 ms and 1.30 ms. Curves cost nothing measurable.

**The output thread is cheap until it reaches the socket.** A pass over 200,000 pixels (17
controllers, 343 universes, 645 packets, 631 KB) takes 0.22 ms to map and packetize, and allocates
nothing. Sending those 645 packets to loopback takes 8 ms: about 12 microseconds a packet, over
thirty times the rest of the pass. A bare `sendto` in a loop costs as much on this machine, so
that is what a send costs here, not something PixelFlow adds, and a network card will differ
either way. But the shape matters: playback sends every frame twice, so at 200k the output thread
spends 16 ms of every 25 sending, more than the player spends drawing a typical frame, and its
passes overrun their 12.5 ms period at the 99th percentile (16.7 to 19.0 ms). In two of the three
typical runs some passes took longer than a frame (the longest 55 and 79 ms) and 14 and 5 frames
were replaced before they could be sent. Every dropped frame in the typical 200k runs was lost
this way; the player never took more than 22 ms over one.

**Memory is small.** Peak RSS is 20 MB at 20k, 46 MB at 100k, and 77 MB at 200k for the typical
sequence (101 MB for the worst case; 141 MB when the engine itself plays it).
Rendering allocates 136 times a frame for the typical 200k sequence (584 KB) and 1,114 times for
the worst case (2.9 MB): Text allocates ten times an effect, and Pinwheel, VU Meter, and Dancer
each take a fresh canvas of 16 bytes a pixel every frame.

**A heavy sequence's first frame is over budget.** The first frame builds every target's pixel
buffer: 37 ms for the worst case at 200k, against 15 ms a frame once it is running, so the show
starts with a stutter. The typical sequence's first frame is 4 ms.

**The preview is free on the engine's side.** Copying the frame for the app takes 0.016 ms at
200k, and the app's read of it (a lock and a 600 KB copy) 0.010 to 0.015 ms. What the browser
then does to draw it was not measured.

### One fix made here

The benchmark found that playback ran late long before it ran out of time. After drawing a frame
the player slept until the next frame boundary, working that out from the clock it had read
*before* drawing. Once a frame took more than 15 ms to draw (the 25 ms period less the 10 ms the
player sleeps at most), the sleep ran past the boundary, every frame started as late as the one
before had taken to draw, and frames were skipped well before drawing took 25 ms.

The time spent drawing now comes off the sleep (`until_next_frame` in `pf-engine`'s
`playback.rs`). The 200k worst case, 60 seconds a run, two runs each way:

| Played by | Sink | Player | Run | FPS | Dropped | Late | Draw starts after due, ms (p50 / p99) | Handed over after due, ms (p50 / p95 / p99) | Load average |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| the benchmark | null | before | 1 | 37.19 | 168 of 2400 | 2178 | 20.8 / 24.7 | 40.7 / 46.9 / 50.4 | 4.7 |
| the benchmark | null | before | 2 | 39.99 | 0 of 2400 | 2400 | 17.9 / 22.0 | 33.5 / 37.6 / 42.3 | 4.7 |
| the benchmark | null | after | 1 | 39.98 | 1 of 2400 | 90 | 1.64 / 3.87 | 20.6 / 24.8 / 26.7 | 5.0 |
| the benchmark | null | after | 2 | 40.00 | 0 of 2400 | 2 | 2.27 / 3.21 | 18.0 / 20.1 / 21.8 | 3.5 |
| the engine | null | before | 1 | 39.38 | 37 of 2400 | 2342 | - | 33.0 / 44.0 / 46.0 | 5.2 |
| the engine | null | before | 2 | 39.95 | 3 of 2400 | 2392 | - | 32.0 / 36.0 / 41.0 | 2.7 |
| the engine | null | after | 1 | 40.00 | 0 of 2400 | 5 | - | 17.0 / 21.0 / 23.0 | 4.9 |
| the engine | null | after | 2 | 39.95 | 3 of 2400 | 13 | - | 16.0 / 20.0 / 23.0 | 3.6 |
| the benchmark | loopback | before | 1 | 39.82 | 10 of 2400 | 2386 | 19.2 / 23.5 | 36.5 / 42.3 / 45.1 | 4.8 |
| the benchmark | loopback | before | 2 | 39.75 | 14 of 2400 | 2373 | 18.7 / 23.4 | 35.3 / 41.6 / 45.2 | 2.9 |
| the benchmark | loopback | after | 1 | 39.12 | 53 of 2400 | 256 | 1.44 / 19.8 | 20.9 / 36.7 / 54.6 | 5.0 |
| the benchmark | loopback | after | 2 | 39.95 | 3 of 2400 | 115 | 1.73 / 15.5 | 19.5 / 24.9 / 41.8 | 4.0 |

On the null sink, frames that reached the output thread 32 to 41 ms after they were due now
reach it in 16 to 21, and "late" fell from over nine frames in ten to under four in a hundred. In
the engine's first pair of runs 37 skipped frames became none. On loopback the frames are as much
earlier, but the player shares the machine with the sending and the listeners there, and how many
frames it drops depends on what else is running: see the repeated runs above.

### Recommendations

In rough order of what they would buy. None is done here.

1. **Render on more than one core.** Rendering is 93% to 99% of a frame and uses one core. Rows
   are drawn one at a time into their own buffers and only then laid over the frame in order, so
   they can be drawn side by side. Four cores should bring the worst case at 200k from 15 ms a
   frame to 5 or so and make 200,000 pixels of anything comfortable. It is also the only way to
   go beyond 200,000.
2. **Send each frame once, when it is ready** (issue #63: drive rendering and output from one
   clock). The output thread free-runs at twice the frame rate. That doubles the packets (51,600 a
   second at 200k where 25,800 would do), makes a frame wait up to 12.5 ms for a pass, and puts
   12.5 ms steps in when frames reach the wire (jitter of 4 to 5 ms at 20k, where frames are ready
   just as a pass starts). Halving the rate alone is not the answer. At 40 passes a second, still
   free-running, the kernel's share of a minute fell from 76 s to 38 s, but frames waited up to a
   whole period for a pass, and 7 fell between the two clocks and were never sent (typical 200k
   sequence, loopback):

   | Passes a second | FPS | Dropped | Output: one pass, ms (p50 / p95 / p99) | Packets/s | Mbit/s | CPU cores (user + system s) | On the wire after due, ms (p50 / p95 / p99) | Jitter ms |
   | --- | --- | --- | --- | --- | --- | --- | --- | --- |
   | 80 | 40.00 | 0 | 13.0 / 15.2 / 15.7 | 48528 | 389 | 1.51 (14.2 + 76.2) | 25.2 / 35.4 / 38.0 | 4.62 |
   | 40 | 39.88 | 7 | 8.57 / 9.32 / 11.4 | 25799 | 207 | 0.82 (11.5 + 37.9) | 35.9 / 36.7 / 38.2 | 1.92 |

   A pass started by the frame's arrival would halve the sending, remove the wait, and lose
   nothing.
3. **Batch the sends** (issue #63: `sendmmsg`, a larger send buffer, a measurement on a real
   network card). Sending is one system call a packet, 645 a pass at 200k. On Linux `sendmmsg`
   hands the kernel a pass in a few calls. This needs measuring on real hardware; loopback on a
   Mac can't say what it is worth.
4. **Keep Fire's grid from frame to frame**, as the other effects that carry state do, and
   simulate from cold only after a seek. Saves most of 0.06 ms for each Fire effect each frame:
   0.25 ms a frame in the typical 200k sequence, 1.5 ms in the worst case.
5. **Build pixel buffers when a sequence opens**, not when a target is first drawn. Takes the
   37 ms first frame out of the worst case, and the smaller hitch each time an effect first uses a
   new render style mid-song.
6. **Reuse the drawn effects' canvases and Text's buffers** rather than allocating them every
   frame. Its worth was not measured and is probably small (rendering is arithmetic, not
   allocation); it is here because 2.9 MB a frame is avoidable.
7. **Give the player and output threads a higher priority.** The runs that lost frames were
   sharing the machine: the same worst-case run dropped nothing one time and 61 frames the next,
   with one frame taking 39 ms to draw that takes 16. Playback of a show should not lose to a
   compiler.
8. **Measure 1,200 universes end to end.** These shows put 200,000 pixels on 343 universes and
   302 DDP packets. An all-sACN show of the same size is 1,177 packets a pass, nearly twice the
   sends.

### What can be claimed

On this evidence:

- **Yes:** "On an Apple M5 Pro, PixelFlow's engine renders, maps, and packetizes a typical
  200,000-pixel sequence in 3.4 ms a frame (6.9 ms at the 99th percentile), a quarter of what 40
  frames a second allows, on one core and in under 80 MB." And: "it plays that sequence in real
  time for a minute without dropping or delaying a frame" (three runs of three on the null
  sink; sending to loopback it lost 0, 5, and 14 frames of 2,400).
- **With the qualifier:** "It holds 40 frames a second at 200,000 pixels" is true of typical
  content. A worst case of eight effect layers on every pixel fits (19.9 ms at the 99th
  percentile) with a fifth of the budget to spare, and loses frames when the machine is busy.
  100,000 pixels has room for the worst case.
- **Not yet:** anything about a real network or controllers (only loopback was measured, and its
  cost here belongs to the operating system); "1,200 universes" end to end; other computers (one
  core does the rendering, and this is a fast one: a core half as fast would still have room for
  the typical 200k sequence, at 14 ms for the 99th percentile, and none for the worst case, at
  40); the app's own cost of drawing the preview; sequences that are not these.

## Caveats

- **Synthetic content.** The shows and sequences are plausible, not real. A real sequence may
  lean on effects these use lightly, or on the 21 kinds they don't use at all. `--mode effects`
  says what each of the sixteen costs; it says nothing about the others.
- **Loopback is not a network.** There is no network card, switch, or controller, and nothing
  that drops, delays, or reorders packets the way a real network does. The cost of sending is the
  operating system's loopback path, on a machine that was also running an endpoint security scanner.
- **No screen.** The engine's side of the preview is measured (copying the frame, and the app's
  read of it). Drawing it in the app is not.
- **One machine, one minute.** One fast laptop-class CPU, runs of 60 seconds, a machine that was
  doing other things. The heavier cells were run three times because single runs disagree.
- **The paced runs are playback rebuilt, not playback.** They use the engine's renderer, mapping,
  packets, transport, and frame clock, with the player's and output thread's loops written again
  around a stopwatch. `--mode engine` and the byte-for-byte test are the checks that they match.
- **Timings on a shared CI runner mean nothing.** The CI smoke test asserts that the harness
  works, never how fast.
