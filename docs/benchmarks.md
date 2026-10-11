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
- [How rendering uses cores](#how-rendering-uses-cores)
- [The shows](#the-shows) and [the sequences](#the-sequences)
- [Reading the results](#reading-the-results)
- [Baseline](#baseline): the numbers from one machine
- [What the baseline says](#what-the-baseline-says): verdicts, bottlenecks, recommendations
- [Lights against the music](#lights-against-the-music): how closely playback keeps to the music
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
| `--mode` | `paced` (real time), `asap` (flat out), `engine` (the engine itself), `effects` (cost by effect), `sync` ([lights against the music](#lights-against-the-music)) | `paced` |
| `--sink` | `null` (packets are built and dropped) or `loopback` (UDP to 127.0.0.1) | `null` |
| `--seconds` | Seconds measured | `60` |
| `--warmup-seconds` | Seconds played first and reported apart | `2` |
| `--send-hz` | Output passes a second in a paced run, instead of playback's twice the frame rate | |
| `--render-threads` | Threads every frame is drawn on, however small the frame (`1`: the thread that asks for it), instead of [the engine's own choice](#how-rendering-uses-cores) | |
| `--load-threads` | In a sync run, threads that do nothing but keep a core busy each | `0` |
| `--seek-seconds` | In a sync run, seconds between the benchmark's own jumps about the sequence (`0`: none) | `20` |
| `--calm` | Before each run, wait (up to two minutes) for the one-minute load average to fall below this | |
| `--json`, `--markdown` | Also write the results to a file | |
| `--render` | Print a report saved with `--json` again, and run nothing | |
| `--describe` | Print the show and its sequences, and run nothing | |
| `--digest` | Print a hash of every frame the tier's sequences draw in their first `--seconds`, and run nothing ([the same frames](#the-same-frames-on-any-number-of-threads)) | |

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
                │           │           frame
     render threads         └──▶ two copies, for the app's preview
```

Every stage is the engine's own code, called the way the engine calls it:

| Stage | What it is | In the engine |
| --- | --- | --- |
| Render | Draw every active effect of the sequence into the show frame, a large frame on [several threads](#how-rendering-uses-cores) | `pf_render::Renderer::render_frame`, as live playback and FSEQ export use it |
| Preview | Copy the frame twice, for the app to read | playback's `Frames::show` |
| Map | Gather each controller's channels: wiring order, color order, brightness, gamma | `pf_output::render_controller` |
| Packetize | Copy the channels into sACN and DDP packets | `SacnPackets::update`, `DdpPackets::update` |
| Send | Hand each packet to the socket | `pf_output::UdpTransport` |

The modes drive those stages differently:

- **`asap`** runs them one after another, as fast as they go, on one thread but for the threads
  rendering shares a frame out among, which it does as an export does: every frame worth it. It
  answers "what does each stage cost?" and gives the ceiling: frames a second with nothing else
  in the way.
- **`paced`** is live playback with a stopwatch on it. The player thread works out which frame is
  due, draws it (sharing out only a frame it wouldn't have ready in time, as playback does),
  copies it for the preview, hands it over, and sleeps to the next frame boundary
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
- **`sync`** plays in the engine too, on a loop with jumps, following a music clock the benchmark
  makes, and times every frame against that clock. It answers "do the lights keep to the music?"
  and has [a section of its own](#lights-against-the-music). Like `engine`, it plays without the
  music's audio track and the picture.

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

## How rendering uses cores

Rendering is most of a frame's work, and a frame with enough of it is shared out among threads.
One thread asks for the frame (the player thread, an export, the preview) and waits for it; a
few threads kept for the purpose draw it. A frame is drawn in four steps
(`crates/pf-render/src/render.rs`):

| Step | What it does | Who does it |
| --- | --- | --- |
| Plan | Finds the rows and effects playing at this moment, the buffer each draws on, and which tiles each row has pixels in | The thread that asked |
| Make ready | For each effect: its settings at this moment, its shader, and for a blurred effect its colors on the buffer's grid, blurred there when the grid is all pixels | Every effect at once |
| Blur in turn | The blurs of grids with gaps between their pixels (a group's, a line's) | One blur after another, a large one's passes shared out |
| Draw | For each tile of the show: every row with pixels there, in the sequence's order, its layers mixed from the bottom up and the row laid over the frame; then the tile's bytes | Every tile at once |

- **Tiles.** The show's pixels are cut into runs of up to 2,048: a large prop into several, small
  props that follow one another into one. Everything a frame does to a pixel once its effects are
  ready (start a row, mix its layers, lay the row over the rows before, write the bytes) reads and
  writes that pixel alone, so a tile needs nothing from any other and a row that mixes with the
  rows below it costs no more to share out than one that covers them. A tile's part of a row
  stays in the processor's nearest cache through all its layers. Sizes from 1,024 to 4,096 measure
  the same; 512 and 8,192 are slower.
- **How many threads.** Every renderer in the process draws on one shared set: one thread for
  every four of the machine's cores and one more, eight at most (four of the fifteen cores of the
  [baseline](#baseline)'s machine, three of eight, two of four). They are started the first time
  a frame is worth sharing out and sleep between frames. There are few of them on purpose:
  [played in real time, more threads made the slowest frames
  slower](#how-many-threads-are-worth-having). `PIXELFLOW_RENDER_THREADS` sets the number (`1`:
  every frame on the thread that asks), and a renderer can be given a set of its own
  (`Renderer::set_pool`).
- **Light frames stay on one thread.** Counting a pixel once for each effect on it, a frame with
  fewer than 60,000 pixels to draw is always drawn by the thread that asked: sharing it out
  [saves too little](#where-sharing-a-frame-out-starts-to-pay). Played by a clock, so is a frame
  with fewer than a million: one thread has it done in half a frame's time or less, and a frame
  shared out [is at the mercy of whatever else the machine is
  doing](#played-by-a-clock-a-frame-is-shared-out-only-when-it-has-to-be). An export, which
  nothing waits on, shares out everything over the lower line (`Renderer::set_flat_out`).
- **Buffers are laid out before playing.** Playback builds every buffer the sequence draws on
  when it starts, and again after an edit, not in the frame that first needs one
  (`Renderer::lay_out_ahead`).

### The same frames on any number of threads

A frame is the same bytes however many threads draw it, and the same as the renderer drew on one
thread before it used more. Each pixel goes through the same arithmetic in the same order
whichever thread draws its tile; the effects made ready at the same time don't look at one
another; and the one thing that does depend on order, blur, keeps its order.

Three things check it:

- `crates/pf-bench/tests/render_threads.rs` draws the typical and worst-case sequences, and a
  third with [a little of everything](#the-sequences) in it, on 1, 2, 4, and as many threads as
  the machine would use, with the show cut into tiles of 3, 61, and a million pixels, and three
  renderers drawing at the same moment, and compares every byte of every frame. It runs in CI.
- `pf-bench --digest` prints a hash of every frame of five sequences at any tier. The renderer as
  it was on one thread and as it is now print the same twenty hashes for 3,000, 20,000, 100,000,
  and 200,000 pixels, on 1, 2, 4, 5, and 12 threads. A digest belongs to the machine it was made
  on (the system's sines and cosines differ in their last bit from one system to another), so it
  compares builds, not machines.
- A corpus of imported xLights sequences scores the same on the [fidelity](fidelity.md)
  scorecard to every digit, before and after, with the ids an import makes up held fixed for
  the comparison (they seed effects, so two runs of one build score a little differently
  without that).

**Blur is the reason one step is done in turn.** A blur works in a space shared by all of them.
For a grid that is all pixels (a matrix, a tree) it writes every cell it reads, and what the
space held before doesn't matter: those blurs run at the same time, each in a space of its own,
and what each left is copied into the shared one afterwards. A grid with gaps (a group's, a
line's) is blurred only near its pixels, and each sum takes in, with the cells just past the ends
of each run, whatever the blur before left there. In exact arithmetic that goes out again as the
sum moves on; in floating point a trace stays in its last decimal place. Most of the time that
never reaches a byte. But a few things that are drawn after a blur ask a yes-or-no question of a
color (is this pixel lit at all?), and those turn a trace into a pixel on or off. So today's
frames depend on the order blurs were done in, back to when the renderer was made, and the
renderer keeps that order rather than draw different frames.

That dependence is older than this change, and it is a flaw: a frame reached by a jump can differ
from the same frame reached by playing, in the pixels of blurred effects on grids with gaps.
Twenty frames over the first five seconds of each sequence, drawn after playing up to them and
again by a renderer made for that frame alone, on the renderer as it was:

| Tier | Typical | Worst case | Varied | Varied, blur off |
| --- | --- | --- | --- | --- |
| 20k | the same | the same | 6 frames differ, in 726 bytes of 1,200,000 | the same |
| 100k | the same | the same | 4 frames differ, in 582 bytes of 6,000,000 | the same |
| 200k | the same | the same | 5 frames differ, in 2,317 bytes of 12,000,000 | the same |

Where a byte differs it can differ by its whole range (245 to 255 at each tier): a pixel lit one
way and dark the other. The varied sequence is built to find such things (every blend on every
kind of buffer); the other two, which are closer to what a show does, don't show it in these
frames. Clearing the cells a blur reads past its runs would make every frame depend on the
document alone and let every blur run at once, at the price of changing those pixels once. It has
not been done here: this change draws the frames the renderer drew before, bit for bit.

### Where sharing a frame out starts to pay

Small shows drawn flat out, on the thread that asks and on four, in milliseconds a frame (the
median of the frames of five seconds, the best of three runs):

| Pixels | Sequence | Pixels to draw a frame | On the thread that asks | On four threads | CPU cores on four |
| --- | --- | --- | --- | --- | --- |
| 1,000 | typical | 1,230 | 0.011 | 0.025 | 2.6 |
| 1,000 | worst | 8,000 | 0.16 | 0.17 | 1.7 |
| 2,000 | typical | 4,060 | 0.039 | 0.047 | 1.9 |
| 2,000 | worst | 16,000 | 0.33 | 0.24 | 1.8 |
| 5,000 | typical | 10,100 | 0.092 | 0.096 | 2.7 |
| 5,000 | worst | 40,000 | 0.64 | 0.32 | 2.8 |
| 10,000 | typical | 17,900 | 0.15 | 0.11 | 2.7 |
| 10,000 | worst | 80,000 | 1.33 | 0.49 | 3.2 |
| 20,000 | typical | 33,000 | 0.26 | 0.18 | 2.4 |
| 20,000 | worst | 160,000 | 1.97 | 0.70 | 3.3 |
| 40,000 | typical | 69,600 | 0.48 | 0.24 | 2.9 |
| 40,000 | worst | 320,000 | 4.30 | 1.29 | 3.6 |

"Pixels to draw" counts a pixel once for each effect on it. Four threads are no sooner than one
below about 10,000 of them and a third sooner by 16,000 to 33,000, at two to three times the CPU.
The line is drawn at 60,000, above where sharing first wins: a frame under it costs one thread a
millisecond at most, and no one is waiting for a part of that.

That is the line for a frame drawn flat out. Played by a clock the line is far higher, for [a
reason of its own](#played-by-a-clock-a-frame-is-shared-out-only-when-it-has-to-be).

### How many threads are worth having

Drawn flat out, a frame is sooner for every thread it is given, by less each time (200,000
pixels, null sink, 30 seconds a run, on the [baseline](#baseline)'s machine of five fast cores
and ten slower ones):

| Threads | Worst case: render, ms (p50 / p99) | Times faster | CPU cores | Typical: render, ms (p50 / p99) | Times faster | CPU cores |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | 16.5 / 20.7 | 1.0 | 1.0 | 3.77 / 6.93 | 1.0 | 1.0 |
| 2 | 8.29 / 10.4 | 2.0 | 1.9 | 1.88 / 3.42 | 2.0 | 1.9 |
| 3 | 5.69 / 7.08 | 2.9 | 2.8 | 1.34 / 2.96 | 2.8 | 2.6 |
| 4 | 4.41 / 5.57 | 3.7 | 3.7 | 1.05 / 1.84 | 3.6 | 3.3 |
| 5 | 3.74 / 4.67 | 4.4 | 4.5 | 0.92 / 1.62 | 4.1 | 3.9 |
| 6 | 3.42 / 4.13 | 4.8 | 5.3 | 0.91 / 1.48 | 4.2 | 4.6 |
| 8 | 2.97 / 4.05 | 5.5 | 6.7 | 0.91 / 1.37 | 4.1 | 5.7 |
| 10 | 2.65 / 3.62 | 6.2 | 8.0 | 0.85 / 1.25 | 4.5 | 6.9 |
| 12 | 2.47 / 4.31 | 6.7 | 9.1 | 0.82 / 1.34 | 4.6 | 7.8 |
| 15 | 2.31 / 3.82 | 7.1 | 10.7 | 0.81 / 1.22 | 4.7 | 9.0 |

(`--render-threads N`; other builds were running, and the load average was 2.4 to 5.7 through the
worst-case runs and 5.3 to 6.3 through the typical ones, whose one-thread figure is a quarter
slower for it than in [before and after](#before-and-after).) Up to four or five threads, each
one is worth nearly a whole thread; after that the fast cores are taken and each is worth less.
Where the time goes, step by step, at the median:

| Sequence | Threads | Plan | Make ready | Blurs in turn | Tiles | Render |
| --- | --- | --- | --- | --- | --- | --- |
| worst | 1 | 0.095 | 3.03 | 0.032 | 13.3 | 16.5 |
| worst | 2 | 0.079 | 1.49 | 0.034 | 6.67 | 8.29 |
| worst | 4 | 0.080 | 0.81 | 0.036 | 3.44 | 4.41 |
| worst | 8 | 0.13 | 0.63 | 0.045 | 2.08 | 2.97 |
| worst | 12 | 0.13 | 0.63 | 0.049 | 1.56 | 2.47 |
| typical | 1 | 0.025 | 0.83 | 0.014 | 3.00 | 3.77 |
| typical | 2 | 0.025 | 0.42 | 0.015 | 1.53 | 1.88 |
| typical | 4 | 0.025 | 0.24 | 0.016 | 0.79 | 1.05 |
| typical | 8 | 0.035 | 0.25 | 0.017 | 0.54 | 0.91 |
| typical | 12 | 0.035 | 0.26 | 0.017 | 0.45 | 0.82 |

The tiles go on getting sooner (8.5 times on twelve threads). Making effects ready gains little
past four threads; its pieces are whole effects of very uneven size, and why it stops there was
not looked into. Past four threads that step is the limit, a quarter of the frame at twelve. The
two steps on one thread, planning and the blurs done in turn, are a fortieth of a frame at four
threads and a fourteenth at twelve.

Played in real time it is not so simple. The threads sleep between frames, the system has to find
each a core when it is woken, and a frame is done when its slowest piece is. Forty seconds a run,
null sink, every frame shared out among `--render-threads`:

| Tier | Sequence | Threads | Player: one frame, ms (p50 / p95 / p99 / max) | Make ready (p50 / p99 / max) | Tiles (p50 / p99 / max) | Dropped, late of frames due | CPU cores | Load average |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 200k | worst | 1 | 16.3 / 20.4 / 20.7 / 25.3 | 3.04 / 4.80 / 6.31 | 13.0 / 16.8 / 22.1 | 0, 1 of 1600 | 0.69 | 4.7 |
| 200k | worst | 2 | 10.1 / 10.8 / 11.0 / 11.6 | 1.85 / 2.77 / 3.06 | 7.93 / 8.97 / 9.56 | 0, 0 of 1600 | 0.81 | 3.4 |
| 200k | worst | 3 | 6.09 / 8.09 / 9.23 / 9.41 | 1.11 / 2.24 / 2.68 | 4.82 / 7.32 / 7.45 | 0, 0 of 1600 | 0.78 | 2.4 |
| 200k | worst | 4 | 6.74 / 7.55 / 7.82 / 8.79 | 1.28 / 2.16 / 2.76 | 5.02 / 6.21 / 7.65 | 0, 0 of 1600 | 1.03 | 3.8 |
| 200k | worst | 5 | 6.33 / 7.03 / 7.39 / 16.7 | 1.25 / 2.26 / 12.0 | 4.57 / 5.66 / 6.16 | 0, 0 of 1600 | 1.20 | 3.6 |
| 200k | worst | 6 | 4.24 / 5.38 / 6.59 / 21.9 | 0.82 / 1.67 / 18.0 | 2.91 / 4.71 / 6.79 | 0, 0 of 1600 | 0.95 | 4.0 |
| 200k | worst | 8 | 5.15 / 6.51 / 7.27 / 22.7 | 1.09 / 2.76 / 19.6 | 3.34 / 4.61 / 12.5 | 0, 0 of 1600 | 1.39 | 3.7 |
| 200k | worst | 12 | 3.58 / 6.73 / 11.0 / 33.6 | 1.09 / 4.97 / 29.7 | 2.01 / 4.89 / 12.9 | 2, 4 of 1600 | 1.43 | 3.2 |
| 200k | typical | 1 | 5.68 / 7.94 / 8.18 / 8.85 | 1.07 / 2.52 / 3.01 | 4.32 / 7.02 / 7.65 | 0, 0 of 1600 | 0.25 | 4.0 |
| 200k | typical | 2 | 2.73 / 6.25 / 6.84 / 18.6 | 0.65 / 2.01 / 12.2 | 2.28 / 4.99 / 5.69 | 0, 0 of 1600 | 0.30 | 2.9 |
| 200k | typical | 3 | 2.63 / 5.17 / 5.93 / 12.3 | 0.53 / 2.09 / 10.6 | 1.98 / 4.43 / 5.29 | 0, 0 of 1600 | 0.36 | 2.8 |
| 200k | typical | 4 | 1.20 / 2.12 / 2.37 / 11.8 | 0.29 / 0.84 / 10.2 | 0.82 / 1.63 / 11.5 | 0, 0 of 1600 | 0.23 | 2.2 |
| 200k | typical | 5 | 2.33 / 4.17 / 4.90 / 23.4 | 0.56 / 1.86 / 20.5 | 1.35 / 3.81 / 4.48 | 0, 0 of 1600 | 0.48 | 2.5 |
| 200k | typical | 6 | 1.32 / 2.88 / 4.63 / 21.7 | 0.38 / 2.17 / 20.4 | 0.75 / 1.96 / 10.8 | 0, 0 of 1600 | 0.34 | 2.6 |
| 200k | typical | 8 | 1.68 / 4.32 / 5.59 / 23.6 | 0.51 / 2.13 / 20.1 | 0.97 / 3.43 / 22.2 | 2, 2 of 1600 | 0.51 | 3.1 |
| 200k | typical | 12 | 1.30 / 3.49 / 4.54 / 21.5 | 0.52 / 1.12 / 20.4 | 0.58 / 3.23 / 13.1 | 0, 0 of 1600 | 0.49 | 3.2 |
| 100k | worst | 1 | 9.72 / 11.3 / 11.7 / 13.3 | 1.72 / 2.73 / 2.96 | 7.39 / 9.84 / 10.3 | 0, 0 of 1600 | 0.40 | 3.5 |
| 100k | worst | 2 | 6.39 / 7.59 / 7.84 / 17.5 | 1.13 / 1.86 / 11.4 | 4.72 / 6.09 / 6.38 | 0, 0 of 1600 | 0.48 | 2.7 |
| 100k | worst | 3 | 4.64 / 6.67 / 7.26 / 9.94 | 0.84 / 1.98 / 6.30 | 3.41 / 5.79 / 6.42 | 0, 0 of 1600 | 0.56 | 2.5 |
| 100k | worst | 4 | 2.62 / 5.73 / 6.39 / 24.1 | 0.59 / 1.66 / 18.6 | 1.95 / 4.97 / 12.8 | 0, 0 of 1600 | 0.49 | 2.4 |
| 100k | worst | 5 | 5.13 / 6.75 / 7.68 / 28.0 | 1.11 / 2.76 / 22.6 | 3.28 / 5.07 / 6.16 | 1, 1 of 1600 | 0.90 | 2.8 |
| 100k | worst | 6 | 2.58 / 6.08 / 7.25 / 23.1 | 0.60 / 2.25 / 20.8 | 1.69 / 4.82 / 20.8 | 0, 2 of 1600 | 0.65 | 3.6 |
| 100k | worst | 8 | 2.06 / 6.44 / 7.32 / 22.5 | 0.51 / 2.35 / 19.3 | 1.31 / 4.53 / 21.5 | 1, 1 of 1600 | 0.70 | 4.4 |
| 100k | worst | 12 | 1.88 / 5.77 / 7.07 / 27.0 | 0.57 / 2.05 / 25.4 | 1.05 / 4.44 / 21.6 | 2, 4 of 1600 | 0.74 | 5.5 |

- **The usual frame** is two to five times sooner on three or four threads than on one. Past
  that it moves about more from run to run than from one number of threads to the next.
- **The slowest frames get slower.** On any number of threads above one, now and then a frame
  takes 10 or 20 ms longer than the rest, most often while making effects ready: the first thing
  the threads do after sleeping. Steps of ten are the system's time slice, which is what a
  thread looks like that was woken and then waited its turn behind another program's; the machine
  was running other builds throughout (see the load averages), so that is the likely cause, not
  a proven one. The more threads a frame waits for, the more often one of them is kept waiting.
  On two to four threads no frame was dropped or late in any of these runs; on five to eight, in
  four runs of nine; on twelve, in two of three. (One thread had a frame late too, in the worst
  case at 200,000 pixels, which leaves it no time to spare.)
- **More threads cost more CPU** for the same frames: the threads look for work for a moment
  before they sleep.

So the shared set is small: about a quarter of the cores. That is a judgment from one machine
that has two kinds of core and was busy with other work, and it gives up some speed where
nothing is waiting on the clock: an export draws flat out and would be done sooner on more
threads. `PIXELFLOW_RENDER_THREADS` is there for that, and for a quiet machine with cores of one
kind, where more threads may cost nothing.

### Played by a clock, a frame is shared out only when it has to be

Four threads make a slow frame rarer, not impossible, and a machine with no core to spare makes
it common. Sharing out in real time every frame it pays to share flat out was tried first, and
measured three ways:

- **A minute of each real-time cell** (null sink): one frame late and one dropped, of 2,400, in
  three cells of six. One thread had a single frame late across all six.
- **The worst case at 100,000 pixels**, which one thread plays with nothing late (11.8 ms at the
  99th percentile): shared out, a minute of it was sooner at every percentile (7.4 ms) and had
  one frame late and one dropped.
- **With every core taken by something else** (`--mode sync` with thirty busy threads, as in
  [lights against the music](#lights-against-the-music)), the typical sequence at 200,000 pixels
  went from keeping in step to missing a frame in four:

| The typical sequence's frames drawn on | Began p50 / p95 / p99 / max (ms) | Sent p50 / p95 / p99 / max (ms) | Missed, of frames due | In step |
| --- | --- | --- | --- | --- |
| One thread, as the renderer was | 1.85 / 3.25 / 3.95 / 7.83 | 12.9 / 20.8 / 22.5 / 27.9 | 0 of 4,799 | yes |
| Four threads, every frame | 2.12 / 19.7 / 23.9 / 25.0 | 24.0 / 71.1 / 101.4 / 189.1 | 1,146 of 4,793 | NO |
| One thread, by the line as it is now | 1.83 / 3.21 / 3.93 / 6.10 | 14.3 / 20.3 / 22.3 / 24.1 | 0 of 4,798 | yes |

One thread that sleeps most of each frame is given a core when it wakes, however busy the
machine. A frame shared out waits for several threads to be given one, more than once in the
frame: each time a thread has to wait for a piece another took, it sleeps and must be woken.

What sharing buys in real time is also less than it looks. The output thread sends on a pass
every 12.5 ms, so a frame that one thread has ready in 6 ms and four have ready in 1.4 goes out
on the same pass.

So played by a clock, a renderer shares a frame out only when one thread would be hard put to
draw it in time: from a million pixels to draw, which is about 10 ms of drawing flat out on this
machine and 10 to 13 in real time, half a frame's time. Of the cells here that is the worst case
at 200,000 pixels alone; everything lighter is drawn on the player's thread as it always was.
`Renderer::set_flat_out` moves the line back to 60,000 for a renderer nothing is waiting on:
FSEQ export does, and so do this benchmark's flat-out runs. Playback doesn't, nor does anything
else that makes a renderer.

The better answer is for the thread that asks for a frame to draw it too, with the others
helping, so that no frame waits for a thread to wake. Sixty seconds of a frame's worth of pieces
every 25 ms, outside the renderer, on four threads: shared out by halves as the renderer does
now, 28 frames of 2,400 took 5 ms or more over the median and the slowest 35 ms; with the asking
thread taking part and pieces handed out one at a time, 2 of 2,400 and 11.5 ms. (On twelve
threads neither way was clean, and another build had started by then.) That is [the next thing
to do](#recommendations), and the line can come down when it is done.

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

A third sequence, **varied**, is never timed. It exists to check that the renderer draws the same
frames however it goes about it ([the same frames](#the-same-frames-on-any-number-of-threads)),
so nothing about it is likely and everything is in it: every kind of effect (39), every blend,
every render style and buffer transform, on a few props of each kind, a submodel, a prop with a
singing face, and every group; fades, sparkles, blur (set, and changing over the effect), color
adjustments, and curves; the effects that carry on from frame to frame, one of them twice under
the same id; and rows that cover, blend with, or replace the rows before them
(`crates/pf-bench/src/varied.rs`).

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
- **Whole frame** (flat out) is everything for one frame: render (on however many threads the
  frame is shared out among), preview copies, and one pass.

The same rendering takes longer in a paced run than flat out, and often longer on the null sink,
where the process is nearly idle, than on loopback: 1.25, 0.47, and 0.33 ms for the typical
sequence at 20k, all on one thread. The likely reason, not measured here, is that the operating
system gives a thread that sleeps most of each frame a slower core or a lower clock speed. Two
paced runs of one cell can differ as much (2.6 and 4.6 ms for the worst case at 20k, a quarter
of an hour apart). Playback gets the same treatment, so the paced figures are the ones that
count for playback and the flat-out ones for what the code costs.

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

Measured on 11 October 2026 with the command above (`--all --engine-check --effects`), 60 seconds
a run after 2 of warm-up, on `develop` as of `3adc261` with rendering [shared out among
threads](#how-rendering-uses-cores): four of them on this machine. The same runs with a frame
drawn on one thread, as it was until then, are in [before and after](#before-and-after).

|  |  |
| --- | --- |
| CPU | Apple M5 Pro |
| Cores | 15: five of a faster kind, ten of a slower |
| Memory | 48 GB |
| OS | macOS 26.6.2 |
| Compiler | rustc 1.98.1 (48a229cea 2026-09-01) |
| Build | release (opt-level 3), aarch64-apple-darwin |

The machine was not idle. Other builds were held off while these runs were made, but an editor's
indexer and an endpoint security scanner ran throughout. The "Load average" column says how busy
it was as each run began. Treat the numbers as a floor, not a best case; single runs move, most
of all in their slowest frames. The real-time rows for the worst case at 100,000 pixels (null
sink, loopback, and the engine) were run again a quarter of an hour after the rest, once the
line for [sharing a frame out by a
clock](#played-by-a-clock-a-frame-is-shared-out-only-when-it-has-to-be) had been put above that
case.

### In real time

| Tier | Sequence | Sink | FPS | Dropped | Player: one frame, ms (p50 / p95 / p99 / max) | Output: one pass, ms (p50 / p95 / p99 / max) | Handed over after due, ms (p50 / p95 / p99 / max) | Late | On time | On the wire after due, ms (p50 / p95 / p99 / max) | Jitter ms | CPU cores | Peak RSS | 40 FPS |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 20k | typical | null | 40.00 | 0 of 2400 | 1.25 / 2.18 / 2.44 / 3.47 | 0.10 / 0.11 / 0.12 / 0.65 | 3.19 / 4.86 / 5.36 / 7.70 | 0 | 100.00% | 14.1 / 14.5 / 14.5 / 18.5 | 2.76 | 0.07 | 22 MB | PASS |
| 20k | typical | loopback | 40.00 | 0 of 2400 | 0.48 / 0.79 / 0.88 / 1.15 | 1.97 / 2.48 / 2.55 / 4.14 | 1.68 / 3.32 / 3.91 / 4.16 | 0 | 100.00% | 15.4 / 16.1 / 16.2 / 18.3 | 5.56 | 0.37 | 21 MB | PASS |
| 20k | worst | null | 40.00 | 0 of 2400 | 2.60 / 3.00 / 3.25 / 5.42 | 0.030 / 0.079 / 0.084 / 0.29 | 4.98 / 5.85 / 6.11 / 7.51 | 0 | 100.00% | 14.1 / 14.5 / 14.5 / 15.6 | 0.67 | 0.11 | 40 MB | PASS |
| 20k | worst | loopback | 40.00 | 0 of 2400 | 2.76 / 3.20 / 3.35 / 4.18 | 1.86 / 2.04 / 2.16 / 8.80 | 4.86 / 5.95 / 6.23 / 7.77 | 0 | 100.00% | 15.4 / 15.9 / 16.0 / 17.3 | 0.78 | 0.44 | 40 MB | PASS |
| 100k | typical | null | 40.00 | 0 of 2400 | 2.66 / 5.90 / 6.58 / 8.75 | 0.14 / 0.41 / 0.50 / 0.72 | 4.80 / 7.86 / 8.72 / 11.2 | 0 | 100.00% | 14.1 / 14.6 / 14.7 / 14.8 | 0.92 | 0.14 | 43 MB | PASS |
| 100k | typical | loopback | 40.00 | 0 of 2400 | 1.94 / 3.20 / 3.51 / 12.0 | 4.84 / 6.09 / 7.55 / 17.1 | 3.99 / 5.72 / 6.32 / 12.2 | 0 | 100.00% | 17.7 / 18.8 / 20.4 / 26.5 | 0.77 | 0.74 | 44 MB | PASS |
| 100k | worst | null | 40.00 | 0 of 2400 | 8.97 / 10.8 / 11.5 / 12.9 | 0.11 / 0.19 / 0.19 / 1.26 | 10.3 / 11.9 / 12.5 / 14.3 | 0 | 100.00% | 14.2 / 14.5 / 14.6 / 27.1 | 0.80 | 0.38 | 96 MB | PASS |
| 100k | worst | loopback | 40.00 | 0 of 2400 | 9.06 / 10.1 / 10.5 / 12.2 | 4.76 / 5.18 / 6.27 / 8.30 | 10.3 / 11.4 / 12.0 / 13.0 | 0 | 100.00% | 17.5 / 18.2 / 18.9 / 30.8 | 1.13 | 1.03 | 94 MB | PASS |
| 200k | typical | null | 40.00 | 0 of 2400 | 5.63 / 7.74 / 8.63 / 10.5 | 0.37 / 0.55 / 0.67 / 1.26 | 7.39 / 9.34 / 10.0 / 12.0 | 0 | 100.00% | 14.4 / 14.7 / 14.8 / 15.9 | 0.74 | 0.26 | 77 MB | PASS |
| 200k | typical | loopback | 40.00 | 0 of 2400 | 4.45 / 7.70 / 8.30 / 9.15 | 10.6 / 13.5 / 14.9 / 21.7 | 6.66 / 9.33 / 9.94 / 11.1 | 0 | 100.00% | 22.0 / 29.9 / 32.8 / 36.9 | 2.57 | 1.58 | 78 MB | PASS |
| 200k | worst | null | 40.00 | 0 of 2400 | 5.06 / 6.70 / 7.17 / 10.6 | 0.25 / 0.40 / 0.52 / 0.70 | 6.99 / 8.45 / 9.08 / 12.9 | 0 | 100.00% | 14.2 / 14.6 / 14.7 / 14.8 | 0.81 | 0.81 | 125 MB | PASS |
| 200k | worst | loopback | 40.00 | 0 of 2400 | 5.97 / 7.01 / 7.43 / 9.21 | 11.0 / 13.3 / 14.8 / 15.8 | 7.76 / 8.99 / 9.49 / 11.7 | 0 | 100.00% | 25.4 / 31.7 / 34.1 / 37.6 | 4.55 | 2.31 | 125 MB | PASS |

| Tier | Sequence | Sink | Render (p50 / p95 / p99 / max) | Preview copies (p50 / p99) | Map (p50 / p99) | Packetize (p50 / p99) | Send (p50 / p95 / p99 / max) | Passes a second | Late passes | Draw starts after due (p50 / p95 / p99 / max) | Preview read (p50 / p99) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 20k | typical | null | 1.25 / 2.17 / 2.44 / 3.47 | 0.006 / 0.008 | 0.094 / 0.11 | 0.006 / 0.009 | 0.002 / 0.002 / 0.003 / 0.014 | 80.0 of 80 | 0 | 1.85 / 3.17 / 3.43 / 6.42 | 0.006 / 0.025 |
| 20k | typical | loopback | 0.47 / 0.79 / 0.87 / 1.15 | 0.002 / 0.003 | 0.029 / 0.058 | 0.002 / 0.006 | 1.93 / 2.44 / 2.51 / 4.10 | 80.0 of 80 | 0 | 1.18 / 2.76 / 3.29 / 3.50 | 0.003 / 0.013 |
| 20k | worst | null | 2.60 / 2.99 / 3.24 / 5.42 | 0.002 / 0.003 | 0.026 / 0.076 | 0.003 / 0.009 | 0.001 / 0.002 / 0.002 / 0.009 | 80.0 of 80 | 0 | 2.39 / 3.16 / 3.29 / 3.51 | 0.003 / 0.018 |
| 20k | worst | loopback | 2.76 / 3.20 / 3.34 / 4.18 | 0.002 / 0.003 | 0.029 / 0.059 | 0.002 / 0.006 | 1.83 / 2.00 / 2.11 / 8.76 | 80.0 of 80 | 0 | 2.11 / 3.19 / 3.39 / 3.59 | 0.003 / 0.013 |
| 100k | typical | null | 2.65 / 5.88 / 6.56 / 8.73 | 0.009 / 0.031 | 0.13 / 0.47 | 0.010 / 0.034 | 0.001 / 0.004 / 0.004 / 0.012 | 80.0 of 80 | 0 | 2.11 / 3.21 / 3.44 / 3.50 | 0.008 / 0.045 |
| 100k | typical | loopback | 1.93 / 3.19 / 3.50 / 12.0 | 0.008 / 0.029 | 0.11 / 0.17 | 0.008 / 0.019 | 4.73 / 5.94 / 7.40 / 16.9 | 80.0 of 80 | 0 | 2.03 / 3.15 / 3.42 / 3.50 | 0.006 / 0.039 |
| 100k | worst | null | 8.96 / 10.8 / 11.5 / 12.9 | 0.008 / 0.010 | 0.10 / 0.18 | 0.008 / 0.022 | 0.001 / 0.002 / 0.002 / 0.022 | 80.0 of 80 | 0 | 1.20 / 1.85 / 2.07 / 2.44 | 0.006 / 0.041 |
| 100k | worst | loopback | 9.05 / 10.1 / 10.5 / 12.2 | 0.008 / 0.010 | 0.11 / 0.15 | 0.008 / 0.014 | 4.64 / 5.06 / 6.15 / 8.12 | 80.0 of 80 | 0 | 1.21 / 2.01 / 2.37 / 2.73 | 0.006 / 0.047 |
| 200k | typical | null | 5.60 / 7.72 / 8.61 / 10.5 | 0.023 / 0.051 | 0.34 / 0.63 | 0.026 / 0.052 | 0.003 / 0.005 / 0.007 / 0.022 | 80.0 of 80 | 0 | 1.74 / 2.69 / 2.97 / 3.43 | 0.022 / 0.061 |
| 200k | typical | loopback | 4.42 / 7.68 / 8.28 / 9.09 | 0.017 / 0.055 | 0.25 / 0.37 | 0.023 / 0.056 | 10.3 / 13.2 / 14.5 / 21.4 | 79.7 of 80 | 18 | 1.94 / 2.93 / 3.21 / 3.51 | 0.012 / 0.065 |
| 200k | worst | null | 5.04 / 6.68 / 7.14 / 10.5 | 0.017 / 0.029 | 0.22 / 0.49 | 0.020 / 0.039 | 0.002 / 0.004 / 0.007 / 0.033 | 80.0 of 80 | 0 | 1.82 / 2.67 / 2.92 / 3.24 | 0.013 / 0.078 |
| 200k | worst | loopback | 5.95 / 6.99 / 7.41 / 9.19 | 0.017 / 0.052 | 0.26 / 0.37 | 0.021 / 0.054 | 10.7 / 13.0 / 14.5 / 15.5 | 79.7 of 80 | 17 | 1.74 / 2.65 / 2.98 / 3.41 | 0.012 / 0.064 |

### Flat out

| Tier | Sequence | Sink | FPS | Render (p50 / p95 / p99 / max) | Preview copies (p50 / p99) | Map (p50 / p99) | Packetize (p50 / p99) | Send (p50 / p99) | Whole frame (p50 / p95 / p99 / max) | Frames over 25 ms | CPU cores | Allocations drawing a frame | Allocations sending it |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 20k | typical | null | 2529 | 0.33 / 0.57 / 0.61 / 1.37 | 0.002 / 0.002 | 0.021 / 0.029 | 0.001 / 0.002 | 0.000 / 0.001 | 0.36 / 0.60 / 0.63 / 1.40 | 0 of 2400 | 1.00 | 37 | 0 |
| 20k | typical | loopback | 570 | 0.35 / 0.59 / 0.65 / 1.51 | 0.002 / 0.002 | 0.022 / 0.039 | 0.001 / 0.004 | 1.29 / 2.39 | 1.69 / 2.14 / 2.96 / 3.53 | 0 of 2400 | 1.25 | 37 | 0 |
| 20k | worst | null | 1308 | 0.72 / 0.91 / 1.08 / 3.21 | 0.002 / 0.005 | 0.021 / 0.044 | 0.002 / 0.003 | 0.000 / 0.001 | 0.75 / 0.93 / 1.12 / 3.24 | 0 of 2400 | 3.37 | 229 | 0 |
| 20k | worst | loopback | 450 | 0.78 / 1.03 / 1.22 / 11.1 | 0.002 / 0.003 | 0.022 / 0.045 | 0.002 / 0.004 | 1.33 / 2.11 | 2.17 / 2.56 / 3.25 / 12.9 | 0 of 2400 | 2.13 | 229 | 0 |
| 100k | typical | null | 1483 | 0.49 / 0.82 / 0.87 / 4.73 | 0.008 / 0.010 | 0.10 / 0.11 | 0.006 / 0.008 | 0.001 / 0.001 | 0.61 / 0.94 / 0.99 / 4.84 | 0 of 2400 | 3.18 | 94 | 0 |
| 100k | typical | loopback | 212 | 0.53 / 0.85 / 0.94 / 5.53 | 0.008 / 0.010 | 0.11 / 0.12 | 0.006 / 0.011 | 3.96 / 5.18 | 4.64 / 5.09 / 6.33 / 11.6 | 0 of 2400 | 1.76 | 94 | 0 |
| 100k | worst | null | 393 | 2.42 / 2.66 / 2.87 / 12.6 | 0.008 / 0.010 | 0.10 / 0.12 | 0.007 / 0.011 | 0.001 / 0.001 | 2.54 / 2.79 / 3.00 / 12.8 | 0 of 2400 | 3.61 | 589 | 0 |
| 100k | worst | loopback | 132 | 2.70 / 3.35 / 3.72 / 12.8 | 0.008 / 0.022 | 0.11 / 0.17 | 0.011 / 0.023 | 4.45 / 7.47 | 7.35 / 9.21 / 10.9 / 17.4 | 0 of 2400 | 2.23 | 589 | 0 |
| 200k | typical | null | 774 | 0.93 / 1.77 / 1.81 / 11.4 | 0.016 / 0.020 | 0.20 / 0.22 | 0.013 / 0.017 | 0.002 / 0.002 | 1.17 / 2.01 / 2.05 / 11.7 | 0 of 2400 | 3.27 | 136 | 0 |
| 200k | typical | loopback | 103 | 1.00 / 1.82 / 1.89 / 12.2 | 0.016 / 0.025 | 0.21 / 0.29 | 0.017 / 0.030 | 8.10 / 12.0 | 9.53 / 11.4 / 13.8 / 20.4 | 0 of 2400 | 1.96 | 136 | 0 |
| 200k | worst | null | 180 | 5.27 / 5.99 / 6.45 / 15.2 | 0.018 / 0.046 | 0.22 / 0.29 | 0.019 / 0.030 | 0.003 / 0.006 | 5.54 / 6.25 / 6.73 / 15.4 | 0 of 2400 | 3.60 | 841 | 0 |
| 200k | worst | loopback | 62 | 5.19 / 6.12 / 6.53 / 13.5 | 0.018 / 0.041 | 0.26 / 0.36 | 0.023 / 0.055 | 10.5 / 14.9 | 16.0 / 19.0 / 21.2 / 28.1 | 1 of 2400 | 2.29 | 841 | 0 |

### Rendering, step by step

As the renderer times its own steps ([how rendering uses cores](#how-rendering-uses-cores)), in
milliseconds. One thread means the frames were under the line for sharing out.

| Tier | Sequence | Mode | Sink | Threads drawing | Plan (p50 / p99) | Make ready (p50 / p99) | Blurs in turn (p50 / p99) | Tiles (p50 / p99) | Render (p50 / p99) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 20k | typical | asap | null | 1 | 0.006 / 0.014 | 0.068 / 0.16 | 0.000 / 0.13 | 0.23 / 0.50 | 0.33 / 0.61 |
| 20k | typical | asap | loopback | 1 | 0.007 / 0.032 | 0.071 / 0.17 | 0.000 / 0.14 | 0.24 / 0.53 | 0.35 / 0.65 |
| 20k | typical | paced | null | 1 | 0.043 / 0.078 | 0.23 / 0.82 | 0.002 / 0.60 | 0.86 / 1.92 | 1.25 / 2.44 |
| 20k | typical | paced | loopback | 1 | 0.017 / 0.058 | 0.095 / 0.24 | 0.001 / 0.17 | 0.32 / 0.71 | 0.47 / 0.87 |
| 20k | worst | asap | null | 4 | 0.020 / 0.042 | 0.18 / 0.30 | 0.069 / 0.26 | 0.45 / 0.61 | 0.72 / 1.08 |
| 20k | worst | asap | loopback | 4 | 0.028 / 0.063 | 0.20 / 0.36 | 0.072 / 0.40 | 0.47 / 0.64 | 0.78 / 1.22 |
| 20k | worst | paced | null | 1 | 0.091 / 0.17 | 0.64 / 0.97 | 0.12 / 0.40 | 1.72 / 2.11 | 2.60 / 3.24 |
| 20k | worst | paced | loopback | 1 | 0.044 / 0.15 | 0.68 / 1.09 | 0.12 / 0.47 | 1.90 / 2.39 | 2.76 / 3.34 |
| 100k | typical | asap | null | 4 | 0.017 / 0.027 | 0.11 / 0.26 | 0.004 / 0.11 | 0.33 / 0.72 | 0.49 / 0.87 |
| 100k | typical | asap | loopback | 4 | 0.020 / 0.037 | 0.12 / 0.30 | 0.004 / 0.12 | 0.34 / 0.73 | 0.53 / 0.94 |
| 100k | typical | paced | null | 1 | 0.041 / 0.13 | 0.34 / 2.97 | 0.003 / 0.22 | 2.22 / 5.79 | 2.65 / 6.56 |
| 100k | typical | paced | loopback | 1 | 0.034 / 0.13 | 0.33 / 0.96 | 0.003 / 0.30 | 1.40 / 3.17 | 1.93 / 3.50 |
| 100k | worst | asap | null | 4 | 0.054 / 0.095 | 0.49 / 0.83 | 0.049 / 0.22 | 1.73 / 2.25 | 2.42 / 2.87 |
| 100k | worst | asap | loopback | 4 | 0.11 / 0.22 | 0.58 / 0.98 | 0.073 / 0.42 | 1.93 / 2.65 | 2.70 / 3.72 |
| 100k | worst | paced | null | 1 | 0.077 / 0.20 | 1.90 / 2.71 | 0.085 / 0.51 | 7.00 / 9.07 | 8.96 / 11.5 |
| 100k | worst | paced | loopback | 1 | 0.092 / 0.22 | 1.85 / 2.68 | 0.086 / 0.47 | 6.83 / 8.65 | 9.05 / 10.5 |
| 200k | typical | asap | null | 4 | 0.025 / 0.055 | 0.22 / 0.53 | 0.011 / 0.11 | 0.63 / 1.29 | 0.93 / 1.81 |
| 200k | typical | asap | loopback | 4 | 0.036 / 0.076 | 0.23 / 0.54 | 0.011 / 0.16 | 0.66 / 1.44 | 1.00 / 1.89 |
| 200k | typical | paced | null | 1 | 0.068 / 0.15 | 1.16 / 2.72 | 0.012 / 0.43 | 4.18 / 6.96 | 5.60 / 8.61 |
| 200k | typical | paced | loopback | 1 | 0.055 / 0.17 | 0.95 / 2.65 | 0.011 / 0.40 | 3.39 / 6.94 | 4.42 / 8.28 |
| 200k | worst | asap | null | 4 | 0.19 / 0.26 | 0.99 / 1.68 | 0.060 / 0.38 | 3.89 / 5.02 | 5.27 / 6.45 |
| 200k | worst | asap | loopback | 4 | 0.19 / 0.29 | 0.96 / 1.64 | 0.059 / 0.50 | 3.90 / 4.99 | 5.19 / 6.53 |
| 200k | worst | paced | null | 4 | 0.18 / 0.31 | 1.02 / 1.68 | 0.064 / 0.47 | 3.71 / 5.65 | 5.04 / 7.14 |
| 200k | worst | paced | loopback | 4 | 0.17 / 0.31 | 1.11 / 1.98 | 0.067 / 0.71 | 4.41 / 5.57 | 5.95 / 7.41 |

### Network, memory, and warm-up

| Tier | Sequence | Mode | Sink | Packets/s | Mbit/s | Send buffer full / failed | Lost on loopback | Threads (pipeline / process) | Peak RSS | Peak heap | Allocations a frame | KB allocated a frame | First frame ms | Warm-up ms (mean / max) | Channels lit | Load average |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 20k | typical | asap | null | 257946 | 1450 | 0 / 0 | - | 1 / 1 | 20 MB | 8 MB | 37 | 71 | 0.4 | 0.3 / 0.5 | 47% | 5.1 |
| 20k | typical | asap | loopback | 58099 | 327 | 0 / 0 | 0.01% | 2 / 2 | 20 MB | 8 MB | 37 | 71 | 2.0 | 1.6 / 2.0 | 47% | 5.1 |
| 20k | typical | paced | null | 8162 | 46 | 0 / 0 | - | 3 / 3 | 22 MB | 9 MB | 37 | 93 | 0.3 | 0.8 / 1.4 | 47% | 5.2 |
| 20k | typical | paced | loopback | 8159 | 46 | 0 / 0 | 0.00% | 4 / 4 | 21 MB | 9 MB | 37 | 93 | 0.2 | 0.3 / 0.5 | 47% | 2.6 |
| 20k | typical | engine | null | 8158 | 46 | 0 / 0 | - | 3 / 3 | 25 MB | 8 MB | - | - | - | - | - | 3.3 |
| 20k | worst | asap | null | 133400 | 750 | 0 / 0 | - | 1 / 5 | 40 MB | 21 MB | 229 | 1263 | 2.9 | 0.7 / 2.9 | 79% | 3.0 |
| 20k | worst | asap | loopback | 45882 | 258 | 0 / 0 | 0.00% | 2 / 6 | 40 MB | 21 MB | 229 | 1263 | 4.9 | 2.1 / 4.9 | 79% | 3.0 |
| 20k | worst | paced | null | 8160 | 46 | 0 / 0 | - | 3 / 3 | 40 MB | 21 MB | 229 | 1286 | 2.3 | 2.3 / 2.5 | 78% | 3.0 |
| 20k | worst | paced | loopback | 8160 | 46 | 0 / 0 | 0.00% | 4 / 4 | 40 MB | 21 MB | 229 | 1287 | 2.5 | 2.7 / 3.3 | 78% | 3.6 |
| 20k | worst | engine | null | 8158 | 46 | 0 / 0 | - | 3 / 3 | 52 MB | 21 MB | - | - | - | - | - | 3.8 |
| 100k | typical | asap | null | 490944 | 3854 | 0 / 0 | - | 1 / 5 | 44 MB | 25 MB | 94 | 490 | 1.4 | 0.5 / 1.4 | 48% | 2.2 |
| 100k | typical | asap | loopback | 70052 | 550 | 0 / 0 | 0.00% | 2 / 6 | 45 MB | 25 MB | 94 | 490 | 6.8 | 4.7 / 6.8 | 48% | 2.2 |
| 100k | typical | paced | null | 26478 | 208 | 0 / 0 | - | 3 / 3 | 43 MB | 26 MB | 94 | 603 | 1.3 | 3.1 / 5.2 | 48% | 2.6 |
| 100k | typical | paced | loopback | 26482 | 208 | 0 / 0 | 0.00% | 4 / 4 | 44 MB | 26 MB | 94 | 605 | 1.3 | 1.4 / 1.8 | 48% | 4.0 |
| 100k | typical | engine | null | 26480 | 208 | 0 / 0 | - | 3 / 3 | 56 MB | 27 MB | - | - | - | - | - | 6.2 |
| 100k | worst | asap | null | 130067 | 1021 | 0 / 0 | - | 1 / 5 | 98 MB | 68 MB | 589 | 2911 | 12.2 | 2.8 / 12.2 | 79% | 3.9 |
| 100k | worst | asap | loopback | 43723 | 343 | 0 / 0 | 0.00% | 2 / 6 | 101 MB | 68 MB | 589 | 2911 | 17.9 | 7.1 / 17.9 | 79% | 4.0 |
| 100k | worst | paced | null | 26478 | 208 | 0 / 0 | - | 3 / 3 | 96 MB | 69 MB | 588 | 3039 | 10.1 | 11.3 / 27.1 | 78% | 9.5 |
| 100k | worst | paced | loopback | 26480 | 208 | 0 / 0 | 0.00% | 4 / 4 | 94 MB | 69 MB | 588 | 3042 | 10.2 | 9.9 / 11.7 | 78% | 4.5 |
| 100k | worst | engine | null | 26480 | 208 | 0 / 0 | - | 3 / 3 | 103 MB | 51 MB | - | - | - | - | - | 5.1 |
| 200k | typical | asap | null | 499330 | 4002 | 0 / 0 | - | 1 / 5 | 84 MB | 53 MB | 136 | 894 | 3.3 | 1.0 / 3.3 | 50% | 2.6 |
| 200k | typical | asap | loopback | 66228 | 531 | 0 / 0 | 0.00% | 2 / 6 | 81 MB | 53 MB | 136 | 894 | 13.3 | 9.0 / 13.3 | 50% | 2.7 |
| 200k | typical | paced | null | 51600 | 414 | 0 / 0 | - | 3 / 3 | 77 MB | 49 MB | 135 | 1126 | 4.0 | 2.9 / 4.0 | 50% | 3.9 |
| 200k | typical | paced | loopback | 51382 | 412 | 0 / 0 | 0.00% | 4 / 4 | 78 MB | 49 MB | 135 | 1127 | 3.3 | 2.9 / 3.5 | 50% | 3.8 |
| 200k | typical | engine | null | 51600 | 414 | 0 / 0 | - | 3 / 3 | 91 MB | 50 MB | - | - | - | - | - | 6.2 |
| 200k | worst | asap | null | 116359 | 933 | 0 / 0 | - | 1 / 5 | 125 MB | 76 MB | 841 | 4805 | 27.6 | 6.6 / 27.6 | 78% | 4.6 |
| 200k | worst | asap | loopback | 39815 | 319 | 0 / 0 | 0.00% | 2 / 6 | 121 MB | 76 MB | 841 | 4805 | 38.6 | 17.3 / 38.6 | 78% | 5.2 |
| 200k | worst | paced | null | 51608 | 414 | 0 / 0 | - | 3 / 7 | 125 MB | 79 MB | 840 | 5071 | 7.4 | 5.9 / 7.4 | 78% | 5.6 |
| 200k | worst | paced | loopback | 51402 | 412 | 0 / 0 | 0.00% | 4 / 8 | 125 MB | 79 MB | 840 | 5074 | 6.5 | 7.4 / 9.1 | 78% | 4.3 |
| 200k | worst | engine | null | 51611 | 414 | 0 / 0 | - | 3 / 7 | 160 MB | 75 MB | - | - | - | - | - | 6.1 |

### The engine itself

| Tier | Sequence | Sink | FPS | Dropped | Late | Drawn and handed over after due, ms (p50 / p95 / p99 / max) | Passes a second | CPU cores | Peak RSS | 40 FPS |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 20k | typical | null | 40.00 | 0 of 2400 | 0 | 0.000 / 2.00 / 2.00 / 3.00 | 80.0 of 80 | 0.07 | 25 MB | PASS |
| 20k | worst | null | 40.00 | 0 of 2400 | 0 | 3.00 / 5.00 / 5.00 / 8.00 | 80.0 of 80 | 0.14 | 52 MB | PASS |
| 100k | typical | null | 40.00 | 0 of 2400 | 0 | 2.00 / 5.00 / 6.00 / 6.00 | 80.0 of 80 | 0.14 | 56 MB | PASS |
| 100k | worst | null | 40.00 | 0 of 2400 | 0 | 9.00 / 10.0 / 11.0 / 15.0 | 80.0 of 80 | 0.40 | 103 MB | PASS |
| 200k | typical | null | 40.00 | 0 of 2400 | 0 | 5.00 / 8.00 / 9.00 / 12.0 | 80.0 of 80 | 0.23 | 91 MB | PASS |
| 200k | worst | null | 40.00 | 0 of 2400 | 0 | 5.00 / 6.00 / 7.00 / 15.0 | 80.0 of 80 | 0.75 | 160 MB | PASS |

- Played by the engine itself, without the music or the picture (it reads those from files).

"Drawn and handed over after due" is the paced runs' "handed over after due", seen from
outside. The two agree to within what the missing music and picture make and the engine's
whole-millisecond clock.

### Effects at 200k

An empty frame (clearing and writing every pixel) takes 0.28 ms; every figure below is on top of that.
Everything here is drawn on one thread: it measures the work an effect is, not how soon several
threads get through it.

#### Each kind alone

| Effect | On every prop, ms a frame | ns a pixel | Allocations a frame | KB allocated a frame | On the whole-house group, ms a frame | ns a pixel |
| --- | --- | --- | --- | --- | --- | --- |
| Fire | 10.0 | 50.0 | 168 | 168 | 0.79 | 3.9 |
| Plasma | 3.96 | 19.8 | 336 | 63 | 3.29 | 16.5 |
| Twinkle | 2.62 | 13.1 | 0 | 0 | 2.57 | 12.8 |
| Pinwheel | 2.16 | 10.8 | 336 | 3360 | 1.72 | 8.6 |
| Butterfly | 1.92 | 9.6 | 0 | 0 | 1.48 | 7.4 |
| Meteors | 1.80 | 9.0 | 168 | 63 | 0.99 | 5.0 |
| Picture | 1.33 | 6.7 | 233 | 174 | 0.88 | 4.4 |
| Spiral | 1.14 | 5.7 | 168 | 3351 | 0.71 | 3.5 |
| Dancer | 1.03 | 5.2 | 168 | 3351 | 0.64 | 3.2 |
| Text | 0.88 | 4.4 | 1344 | 88 | 0.64 | 3.2 |
| VU Meter | 0.86 | 4.3 | 224 | 3358 | 0.68 | 3.4 |
| Bars | 0.84 | 4.2 | 168 | 113 | 0.65 | 3.2 |
| Color Wash | 0.81 | 4.1 | 0 | 0 | 0.78 | 3.9 |
| Ripple | 0.73 | 3.7 | 0 | 0 | 0.58 | 2.9 |
| On | 0.69 | 3.5 | 0 | 0 | 0.65 | 3.3 |
| Pulse | 0.60 | 3.0 | 0 | 0 | 0.57 | 2.8 |

#### Blur, sparkles, and curves (on every prop)

| Effect | ms a frame | ns a pixel |
| --- | --- | --- |
| Plasma | 3.99 | 19.9 |
| Plasma with sparkles | 4.42 | 22.1 |
| Plasma with blur 2 | 5.98 | 29.9 |
| Plasma with blur 8 | 6.41 | 32.1 |
| Plasma with a curve on its speed | 3.96 | 19.8 |

#### The typical sequence, first 60 s: 3.39 ms a frame

| Effect | Effects | Layers a pixel | Alone, ms a frame | Share | ns a pixel drawn |
| --- | --- | --- | --- | --- | --- |
| Twinkle | 358 | 0.323 | 0.87 | 25% | 13.4 |
| Plasma | 32 | 0.103 | 0.41 | 12% | 19.6 |
| Meteors | 325 | 0.160 | 0.38 | 11% | 11.9 |
| Ripple | 344 | 0.311 | 0.27 | 8% | 4.4 |
| Fire | 34 | 0.056 | 0.27 | 8% | 23.9 |
| Color Wash | 364 | 0.174 | 0.26 | 7% | 7.5 |
| Pinwheel | 76 | 0.061 | 0.20 | 6% | 16.4 |
| Bars | 321 | 0.168 | 0.18 | 5% | 5.3 |
| Butterfly | 66 | 0.065 | 0.13 | 4% | 10.4 |
| Spiral | 74 | 0.090 | 0.13 | 4% | 7.0 |
| Picture | 17 | 0.063 | 0.086 | 2% | 6.9 |
| Dancer | 16 | 0.053 | 0.084 | 2% | 8.0 |
| Text | 15 | 0.072 | 0.081 | 2% | 5.6 |
| Pulse | 315 | 0.095 | 0.077 | 2% | 4.0 |
| VU Meter | 15 | 0.053 | 0.062 | 2% | 5.8 |
| On | 169 | 0.014 | 0.021 | 1% | 7.5 |
| **All kinds, added up** | 2541 | 1.859 | 3.50 | 100% |  |

Prop rows alone take 2.56 ms a frame and group rows alone 0.85 ms. Of the whole sequence's time, blur accounts for 0.36 ms a frame, sparkles for 0.065 ms, and curves on other settings for -0.022 ms (what the sequence saves without them). Drawing it allocates 134 times a frame, 886 KB in all.

#### The worst sequence, first 60 s: 16.0 ms a frame

| Effect | Effects | Layers a pixel | Alone, ms a frame | Share | ns a pixel drawn |
| --- | --- | --- | --- | --- | --- |
| Plasma | 678 | 1.108 | 4.34 | 27% | 19.6 |
| Butterfly | 678 | 1.124 | 2.29 | 14% | 10.2 |
| Meteors | 678 | 1.050 | 1.81 | 11% | 8.6 |
| Twinkle | 614 | 0.722 | 1.76 | 11% | 12.1 |
| Fire | 156 | 0.354 | 1.57 | 10% | 22.1 |
| Spiral | 186 | 0.962 | 1.00 | 6% | 5.2 |
| Ripple | 678 | 1.071 | 0.91 | 6% | 4.3 |
| Pinwheel | 156 | 0.344 | 0.73 | 5% | 10.6 |
| Picture | 64 | 0.292 | 0.39 | 2% | 6.6 |
| Dancer | 64 | 0.278 | 0.37 | 2% | 6.7 |
| Text | 64 | 0.274 | 0.29 | 2% | 5.3 |
| VU Meter | 64 | 0.264 | 0.27 | 2% | 5.1 |
| Bars | 492 | 0.052 | 0.075 | 0% | 7.2 |
| Color Wash | 492 | 0.052 | 0.066 | 0% | 6.3 |
| Pulse | 492 | 0.052 | 0.052 | 0% | 5.0 |
| **All kinds, added up** | 5556 | 8.000 | 15.9 | 100% |  |

Prop rows alone take 8.77 ms a frame and group rows alone 7.17 ms. Of the whole sequence's time, blur accounts for 0.34 ms a frame, sparkles for 1.22 ms, and curves on other settings for -0.31 ms (what the sequence saves without them). Drawing it allocates 838 times a frame, 4792 KB in all.

### Before and after

Each cell on the null sink, run with `develop`'s renderer as it was at `3adc261`, which drew a
frame on one thread, and straight afterwards with this one: flat out over 88 seconds of the
sequence, then in real time for a minute. Other builds were held off for these runs.

**Flat out**, every frame worth sharing out is, as an export's are:

| Tier | Sequence | Threads | Render before, ms (p50 / p95 / p99) | After | Times sooner (p50) | Frames a second | CPU cores | Peak RSS | Allocations drawing a frame | Load average |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 20k | typical | 1 | 0.35 / 0.55 / 0.61 | 0.36 / 0.57 / 0.64 | 1.0 | 2501 → 2388 | 1.00 → 1.00 | 20 → 21 MB | 45 → 36 | 3.5, 3.5 |
| 20k | worst | 4 | 1.98 / 2.27 / 2.33 | 0.68 / 0.82 / 1.00 | 2.9 | 499 → 1388 | 1.00 → 3.38 | 33 → 41 MB | 332 → 230 | 2.9, 2.8 |
| 100k | typical | 4 | 1.57 / 2.59 / 2.78 | 0.53 / 0.81 / 0.84 | 3.0 | 565 → 1477 | 1.00 → 3.18 | 43 → 50 MB | 120 → 97 | 1.9, 1.8 |
| 100k | worst | 4 | 8.19 / 9.19 / 9.44 | 2.37 / 2.66 / 2.83 | 3.5 | 120 → 400 | 1.00 → 3.62 | 83 → 99 MB | 850 → 587 | 1.8, 1.8 |
| 200k | typical | 4 | 3.05 / 6.03 / 7.18 | 0.94 / 1.75 / 1.96 | 3.3 | 271 → 777 | 1.00 → 3.25 | 78 → 84 MB | 172 → 138 | 2.6, 2.8 |
| 200k | worst | 4 | 15.5 / 18.1 / 19.6 | 4.35 / 4.97 / 5.39 | 3.6 | 63 → 215 | 1.00 → 3.68 | 106 → 135 MB | 1202 → 838 | 1.9, 3.1 |

Four threads have a frame 2.9 to 3.6 times as soon as one did, for 3.2 to 3.7 cores' work. The
typical sequence at 20,000 pixels is under the line for sharing and takes what it took: its
frames are drawn in tiles on one thread now, which measured 2 to 5% slower in two pairs of runs,
as much as two runs of the same build differed by. Peak memory is up by 8 to 27% at the larger
tiers, for the lists of where each buffer falls among the tiles and the working space each
effect and thread keeps. Allocations a frame are down by a fifth to a third: the lists a frame
used to build are kept from one frame to the next.

Run to run, a shared frame's time moves more than one thread's does, with which cores the
threads are given: the matrix above has the worst case at 200,000 pixels at 5.27 ms a frame at
the median, this pair at 4.35.

Where a shared frame's time goes, as the renderer times its own steps:

| Tier | Sequence | Threads | Plan (p50 / p99) | Make ready (p50 / p99) | Blurs in turn (p50 / p99) | Tiles (p50 / p99) | Render (p50 / p99) |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 20k | typical | 1 | 0.007 / 0.011 | 0.076 / 0.22 | 0.001 / 0.14 | 0.27 / 0.53 | 0.36 / 0.64 |
| 20k | worst | 4 | 0.019 / 0.060 | 0.17 / 0.27 | 0.025 / 0.18 | 0.44 / 0.62 | 0.68 / 1.00 |
| 100k | typical | 4 | 0.018 / 0.033 | 0.11 / 0.36 | 0.003 / 0.11 | 0.34 / 0.70 | 0.53 / 0.84 |
| 100k | worst | 4 | 0.051 / 0.082 | 0.48 / 0.81 | 0.046 / 0.28 | 1.75 / 2.28 | 2.37 / 2.83 |
| 200k | typical | 4 | 0.026 / 0.046 | 0.21 / 0.59 | 0.010 / 0.11 | 0.64 / 1.38 | 0.94 / 1.96 |
| 200k | worst | 4 | 0.087 / 0.16 | 0.79 / 1.30 | 0.052 / 0.22 | 3.40 / 4.27 | 4.35 / 5.39 |

On four threads the tiles are three quarters of the worst case's frame at 200,000 pixels (3.40
of 4.35 ms), making effects ready a fifth (0.79), and the two steps done on one thread 3%.

**In real time**, a frame is shared out only [when one thread would be hard put to draw
it](#played-by-a-clock-a-frame-is-shared-out-only-when-it-has-to-be):

| Tier | Sequence | Threads | Player: one frame before, ms (p50 / p95 / p99 / max) | After | Dropped, late of 2,400: before | After | Handed over after due, ms (p50 / p99): before | After | First frame, ms | CPU cores | Peak RSS | Load average |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 20k | typical | 1 | 1.06 / 2.27 / 2.39 / 4.67 | 1.10 / 2.24 / 2.42 / 2.52 | 0, 0 | 0, 0 | 2.80 / 5.38 | 2.82 / 5.44 | 0.4 → 0.2 | 0.06 → 0.06 | 21 → 21 MB | 3.6, 2.7 |
| 20k | worst | 1 | 4.51 / 6.80 / 7.97 / 17.8 | 4.63 / 6.70 / 7.64 / 10.8 | 0, 0 | 0, 0 | 6.40 / 9.86 | 6.49 / 9.68 | 3.7 → 2.3 | 0.19 → 0.19 | 37 → 41 MB | 2.9, 1.8 |
| 100k | typical | 1 | 4.53 / 6.37 / 7.51 / 13.3 | 4.21 / 6.29 / 7.27 / 8.32 | 0, 0 | 0, 0 | 6.48 / 9.22 | 6.10 / 9.17 | 1.5 → 1.2 | 0.22 → 0.19 | 47 → 43 MB | 2.0, 1.3 |
| 100k | worst | 1 | 10.5 / 11.2 / 11.8 / 12.0 | 9.20 / 10.9 / 11.3 / 14.9 | 0, 0 | 0, 0 | 11.4 / 12.7 | 10.6 / 12.6 | 17.6 → 10.5 | 0.44 → 0.39 | 82 → 96 MB | 1.9, 21.2 |
| 200k | typical | 1 | 6.16 / 8.93 / 9.23 / 19.4 | 6.21 / 8.30 / 9.17 / 13.8 | 0, 0 | 0, 0 | 7.90 / 10.8 | 7.95 / 10.6 | 4.2 → 3.2 | 0.29 → 0.29 | 75 → 77 MB | 2.9, 1.9 |
| 200k | worst | 4 | 16.0 / 18.7 / 20.7 / 29.9 | 4.77 / 6.96 / 7.35 / 24.9 | 0, 3 | 1, 1 | 18.4 / 22.3 | 6.92 / 9.23 | 37.7 → 6.1 | 0.68 → 0.81 | 104 → 121 MB | 3.3, 3.8 |

- **Five cells of six play as they did**, on the player's thread: their frames are under the
  line for sharing by a clock. (The worst case at 100,000 pixels was run again at the end, once
  the line had been put above it. The load average beside it is what the busy threads of a sync
  run had left behind a minute earlier; nothing else was running.)
- **The worst case at 200,000 pixels is shared out.** The player has a frame drawn in 4.8 ms
  where it took 16.0, in 7.4 ms at the 99th percentile where it took 20.7, and hands it over 7 ms
  after it was due where it handed it over 18 ms after: in time for the output thread's first
  pass, not its second. One frame of its 2,400 was late and one dropped, where one thread had
  three late: the slow frame that [waiting on several threads
  brings](#how-many-threads-are-worth-having), 25 ms this time.
- **The first frame no longer builds every buffer.** Playback lays a sequence's buffers out
  before it starts, so the first frame takes what any frame does: 6 ms for the worst case at
  200,000 pixels, where it took 38.

## What the baseline says

### Does it hold 40 frames a second?

Three one-minute runs of each cell: the matrix's on the null sink and on loopback, and the
[before and after](#before-and-after) run on the null sink. The time is the player's for one
frame at the 99th percentile, of 25 ms.

| Tier | Typical sequence | Worst case |
| --- | --- | --- |
| 20k | **Pass.** 0.9 to 2.4 ms: a tenth of the budget at most. | **Pass.** 3.3 to 7.6 ms. |
| 100k | **Pass.** 3.5 to 7.3 ms. | **Pass.** 10.5 to 11.5 ms on one thread; nothing dropped or late. |
| 200k | **Pass.** 8.3 to 9.2 ms on one thread, just over a third of the budget; nothing dropped or late, sending to loopback included. | **Pass.** 7.2 to 7.4 ms on four threads, where one took 20 to 22; one frame late and one dropped in 7,200. |

The worst case at 200,000 pixels was marginal on one thread (up to 8 frames late in a minute on
the null sink, and one run of three on loopback that dropped 61). It now has two thirds of its
budget to spare. On loopback at 200k the output thread's passes still overrun their 12.5 ms
period at the 99th percentile (14.8 and 14.9 ms; 17 and 18 passes of 4,800 late), without
losing a frame in these runs.

Flat out, with nothing else in the way, a frame's whole work (render, preview copies, one pass to
the null sink) takes this much of the 25 ms at the 99th percentile:

| Tier | Typical | Worst case |
| --- | --- | --- |
| 20k | 0.63 ms (3%), 2,529 frames a second | 1.12 ms (4%), 1,308 |
| 100k | 0.99 ms (4%), 1,483 | 3.00 ms (12%), 393 |
| 200k | 2.05 ms (8%), 774 | 6.73 ms (27%), 180; slowest frame 15.4 ms |

### Where the time goes

**Rendering is still most of a frame, and a heavy frame's is shared out.** Flat out on the null
sink at 200k, a typical frame is 0.93 ms of rendering on four threads, 0.20 ms of mapping,
0.013 ms of packetizing, and 0.016 ms of preview copies: rendering is 79% of the frame, where it
was 93% on one thread, and 95% in the worst case (5.27 of 5.54 ms). In real time the worst case
at 200k keeps 0.81 of a core busy, where it kept 0.66 of one.

**Inside a frame, nearly all of it is the tiles.** As the renderer times its own steps on the
worst case at 200k: on one thread, making effects ready is 3.0 ms of 16.5 and drawing the tiles
13.3; on four, 0.8 and 3.4 of 4.4. Planning the frame and the blurs that have to be done in turn
are 0.12 to 0.14 ms together, on one thread however many there are
([step by step](#how-many-threads-are-worth-having)).

**No one effect dominates; the cost is pixels times layers.** On one thread at 200k the typical
sequence draws each pixel 1.86 times a frame for 3.39 ms; the worst case draws each 8 times for
16.0 ms. That is 9 to 10 ns for each pixel of each layer either way. The effects range from 3 to
4 ns a pixel (Pulse, On, Ripple, Color Wash, Bars) to 13 (Twinkle) and 20 (Plasma). In the
typical sequence Twinkle is the largest share (25%) only because it is on the most pixels; in
the worst case Plasma is (27%). Group rows cost about what prop rows do for the same pixels: in
the worst case, with four layers of each on every pixel, the six group rows are 7.2 ms and the
168 prop rows 8.8 ms.

**Fire is the one outlier.** Alone on every prop it costs 50 ns a pixel, more than twice the next
effect, but only 4 ns on the whole-house group. Each Fire effect simulates its 32 x 32 grid from
cold for 56 steps every frame, whatever the size of what it is drawn on: about 0.06 ms an effect,
10 ms for 168 of them. It is 8% of the typical sequence's time with 0.056 layers a pixel.
Skipping the cells of the grid that are cold whatever they roll, which is as far as it can be
sped up without drawing a different fire, made no difference that shows here.

**Blur and sparkles are modest.** Blur adds 10 to 12 ns a pixel to an effect (Plasma 19.9 to
29.9 and 32.1), about a third of a millisecond a frame to either sequence. Sparkles add 2 ns a
pixel, 0.07 ms and 1.2 ms. Curves cost nothing measurable.

**The output thread is cheap until it reaches the socket, and sending is now the largest thing
the engine does.** A pass over 200,000 pixels (17 controllers, 343 universes, 645 packets,
631 KB) takes a quarter of a millisecond to map and packetize, and allocates nothing. Sending
those 645 packets to loopback takes 8 to 11 ms: 12 to 17 microseconds a packet, over thirty
times the rest of the pass. A bare `sendto` in a loop costs as much on this machine, so that is
what a send costs here, not something PixelFlow adds, and a network card will differ either
way. But the shape matters: playback sends every frame twice, so at 200k the output thread
spends 16 to 21 ms of every 25 sending, three to four times what the player spends drawing the
frame, and its passes overrun their 12.5 ms period at the 99th percentile. On one thread, two of
three typical runs lost frames this way (5 and 14 of 2,400, replaced before a slow pass could
send them); none was lost in these runs.

**Memory is small.** Peak RSS is 22 MB at 20k, 44 MB at 100k, and 78 MB at 200k for the typical
sequence in real time (125 MB for the worst case, 104 on one thread; 160 MB when the engine
itself plays it). Rendering allocates 136 times a frame for the typical 200k sequence (0.9 MB)
and 841 times for the worst case (4.8 MB): Text allocates eight times an effect, and Pinwheel,
Spiral, VU Meter, and Dancer each take a fresh canvas of 16 bytes a pixel every frame.

**The first frame is a frame like any other.** Playback lays out every buffer a sequence draws
on before it starts: the worst case's first frame at 200k takes 6 to 7 ms, where it took 37 and
the show started with a stutter. Drawn flat out with nothing laid out ahead it takes 28 ms.

**The preview is free on the engine's side.** Copying the frame for the app takes 0.02 ms at
200k, and the app's read of it (a lock and a 600 KB copy) 0.01 to 0.02 ms. What the browser then
does to draw it was not measured.

### One fix made here

The benchmark found that playback ran late long before it ran out of time. After drawing a frame
the player slept until the next frame boundary, working that out from the clock it had read
*before* drawing. Once a frame took more than 15 ms to draw (the 25 ms period less the 10 ms the
player sleeps at most), the sleep ran past the boundary, every frame started as late as the one
before had taken to draw, and frames were skipped well before drawing took 25 ms.

The time spent drawing now comes off the sleep (`until_next_frame` in `pf-engine`'s
`playback.rs`). The 200k worst case, 60 seconds a run, two runs each way, measured when a frame
was drawn on one thread and this sequence took 15 to 16 ms a frame:

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

Two of the first baseline's recommendations are done. Rendering is [shared out among
cores](#how-rendering-uses-cores): four threads bring the worst case at 200k from 15.5 ms a frame
to 4.4 to 5.3 flat out, as predicted. And a sequence's pixel buffers are built before it plays,
which takes the 38 ms first frame out of the worst case (6 ms now). A third could not be done:
Fire's grid can't be kept from frame to frame without changing what Fire draws, because each
frame works its grid out from cold with that frame's settings. Its cold cells are no longer
rolled for, which changes nothing it draws and, as it turned out, nothing that shows in what it
costs.

What is left, in rough order of what it would buy:

1. **Have the thread that asks for a frame help draw it.** Today it hands the frame to the
   shared threads and sleeps, and a thread that waits for a piece another took sleeps too; each
   has to be woken, and on a busy machine a woken thread sometimes waits a time slice or two for
   a core. That is why a frame is shared out by a clock [only when it has to
   be](#played-by-a-clock-a-frame-is-shared-out-only-when-it-has-to-be) and why the shared set
   is small. With the asking thread drawing and pieces handed out one at a time, a frame should
   never be later than one thread alone has it, bar a helper stopped in the middle of a piece;
   the experiment there took the slow frames from 28 in 2,400 to 2. The line for sharing in real
   time could then come down and the typical sequences get the cores too.
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
4. **Give the player, output, and render threads a higher priority.** The frames that are late
   are threads waiting their turn behind other programs: a worst-case run dropped nothing one
   time and 61 frames the next on one thread, and a frame shared out waits on several. Playback
   of a show should not lose to a compiler.
5. **Make a blur independent of the blurs before it.** A blur on a grid with gaps takes in what
   the last blur left in their shared working space
   ([above](#the-same-frames-on-any-number-of-threads)), so a frame reached by a jump can differ
   from the same frame reached by playing, and those blurs have to be done one after another.
   Clearing the cells past each run first would make a frame depend on the document alone and
   let every blur be made ready at once. It changes some pixels of some blurred effects, once,
   which is why it was not done with a change that had to draw the same frames.
6. **Give an export more threads than playback has.** Flat out, twelve threads draw the worst
   case 6.7 times as fast as one and four 3.7 times. `Renderer::set_pool` is there for it; what
   else is running during an export is the question.
7. **Take making effects ready further apart.** Past four threads it is what limits a frame, and
   why was not looked into. Fire's grids are a known part of it: 0.06 ms for each Fire effect
   each frame. Keeping a grid from frame to frame would save most of that and draw a different
   fire.
8. **Reuse the drawn effects' canvases and Text's buffers** rather than allocating them every
   frame. Its worth was not measured and is probably small (rendering is arithmetic, not
   allocation); it is here because 2.9 MB a frame is avoidable.
9. **Measure 1,200 universes end to end.** These shows put 200,000 pixels on 343 universes and
   302 DDP packets. An all-sACN show of the same size is 1,177 packets a pass, nearly twice the
   sends.

### What can be claimed

On this evidence:

- **Yes:** "On an Apple M5 Pro, PixelFlow's engine renders, maps, and packetizes a typical
  200,000-pixel sequence in 1.2 ms a frame (2.1 ms at the 99th percentile) on four threads, and a
  worst case of eight effect layers on every pixel in 5.5 ms (6.7)." And: "it plays the typical
  sequence in real time for a minute without dropping or delaying a frame, on one thread and in
  under 80 MB" (three runs of three, one of them sending to loopback).
- **Yes, since frames are shared out:** "It holds 40 frames a second at 200,000 pixels", of the
  worst case too: 7.4 ms of the 25 at the 99th percentile, in step with the music's clock, one
  frame late and one dropped in 7,200. On one thread that case was marginal.
- **With the qualifier:** a frame shared among threads is now and then 10 or 20 ms slower than
  the rest (up to once a minute here), which one thread's frames rarely are; that is why only
  frames that need it are shared out in real time. And on a machine whose every core is busy
  with something else, the typical sequence keeps in step and the worst case does not.
- **Not yet:** anything about a real network or controllers (only loopback was measured, and its
  cost here belongs to the operating system); "1,200 universes" end to end; other computers (the
  threads were tuned on one machine with two kinds of core; on one thread, a core half as fast
  would still have room for the typical 200k sequence, at 18 ms for the 99th percentile); the
  app's own cost of drawing the preview; sequences that are not these.

## Lights against the music

Everything above asks whether frames arrive. This asks whether they arrive *with the music*:
whether the frame on the wire is the one for the moment being heard, and stays so through a
jump, a round of a loop, and a busy machine.

Two things answer it, and they are kept apart on purpose.

**The guarantees are tests.** `crates/pf-engine/src/playback/simulation.rs` runs the player's own
loop against a made-up machine: time moves only when the player sleeps or draws, the sound device
is a stopwatch on that time whose clock can run fast or slow or stop calling back, a frame costs
whatever the test says to draw, and the user's clicks arrive when scripted. Nothing reads the
machine's clock, so ten simulated minutes take milliseconds, every run is identical, and a limit
can be tight without ever failing by accident. They run with `cargo test`, in CI. What they hold
the player to, with sleeps that overrun by up to 1.5 ms:

| What | Held to |
| --- | --- |
| Ten minutes of 25 ms frames, each taking 8 to 21 ms to draw, on a device 0.05% fast or slow | Every frame drawn once, in order, begun within 3.6 ms of its time; the last minute no further behind than the first; the lights follow the music, which by then is 0.3 s from the machine's clock |
| A frame that takes 60 ms | The frame due is drawn the moment it is done; the frames missed are never shown late; the next is back on time |
| A jump (a thousand of them, with four offsets) | Its frame is drawn in the same turn the player hears of it, the music is sent to the same place less the offset, and every frame follows in order |
| A pause, and playing again | Nothing is drawn while paused; music and lights stand still; no frame is repeated or missed |
| A loop (three hundred rounds) | The music is taken back within 3.6 ms of the lights' end and the first frame drawn in that same turn; each round lasts the sequence's length and no more than that over |
| A stop | Nothing more is drawn; the player is gone within one sleep |
| An offset | Lights ahead play their first frames before the song starts; lights behind hold their first frame until the music is that far in; a change moves the lights at once |
| A sound device that stops calling back | The lights wait a second, then keep time on their own, never ahead of where the music would be; a pause, a jump, a new speed, and the end of the sequence each keep the lights' place; a device that comes back is followed at once |

**What a real machine gives the player is measured.** A sleep overruns by however much the machine
feels like, and threads wait for cores. `--mode sync` plays the synthetic sequence in the engine
itself, on a loop, and times every frame:

```sh
cargo run --release -p pf-bench -- --mode sync --tier 200k --seconds 120 \
    --sequence-seconds 30 --seek-seconds 15 --calm 4
# The same with every core kept busy by something else:
cargo run --release -p pf-bench -- --mode sync --tier 200k --seconds 120 \
    --sequence-seconds 30 --seek-seconds 15 --load-threads 30
```

- **The music** is a clock the benchmark makes: a silent stopwatch on the machine's clock that any
  thread can read. There is no sound device, so nothing here says how long a sound card or a
  Bluetooth link takes to make a sound; that is what a sequence's offset is for.
- **Began** is where the music was, past the start of a frame's own time, when the player first
  read a time in that frame, which is when it begins to draw it. The clock notes this itself, so
  watching adds nothing. It is the player's own lateness: its sleep's overrun, or a frame before
  it that took too long to draw.
- **Sent** is the same when the frame first went out to a controller. Two marker props on a
  controller of their own, sent after every other, show the frame's number as colors, and the
  transport reads the number back out of that controller's packet
  (`the_marker_comes_through_the_engines_own_drawing_and_wiring`). So it is the last controller's
  view of a frame: began, plus drawing it, plus the wait for the output thread's next pass (up to
  12.5 ms), plus that pass.
- **Drift** is the slope of a straight line through the median of *began* in each five seconds of
  the run, in ms a minute, with its standard error. The median leaves out the frames that began
  late because the one before took too long to draw, so what is left is the player's own
  timekeeping and not what was being drawn. Two minutes settle it to about a tenth of a
  millisecond a minute; a short or jittery run says so in the "±".
- **Missed** frames never went out (the player skipped them, or the output thread never got to
  them). **Sent again** counts frames that went out after a later one had, which playback in
  order never does. **In flight** are frames from the old place that went out after a jump was
  asked for: they were already on their way.
- **Jump to wire** is from the benchmark asking for a jump (every `--seek-seconds`: on seven
  seconds, then back three) to a frame from the new place going out. **New round to wire** is
  the same from the music being taken back to its top. **Round late** is how far past the
  lights' end the music was when it was taken back.

A run is **in step** when all of these hold:

| | Limit | Why that |
| --- | --- | --- |
| Sent, 99th percentile | The frame time plus one send: 37.5 ms | A frame is on the wire while it is still the frame being heard, or within one pass of the output thread after |
| Drift | 1 ms a minute, beyond three standard errors | Drift is a steady gain; a millisecond a minute is a frame over a song. The simulations hold it to nothing at all |
| Frames that went out | 99 in 100 | The same rule as a paced run |
| Sent again | None | Playback in order never sends a frame after a later one |
| Jump or new round to wire, at worst | 100 ms | The player looks for a jump every 10 ms, draws a frame, and the output thread sends within 12.5 ms; the rest is room for a busy machine |

CI runs a few seconds of it on a small show to check that the harness works
(`the_lights_are_timed_against_the_music_in_the_engine`) and asserts none of these: a shared
runner's timing means nothing.

### On one machine

Measured on 11 October 2026 on the machine of the [baseline](#baseline), with the two commands
above and the same two again with `--variant worst`: 200,000 pixels, two minutes each after two
seconds of warm-up, a 30-second sequence on a loop, a jump every 15 seconds. "30 busy threads" is
twice the machine's cores doing nothing useful. The machine was not idle to begin with (an
endpoint security scanner and an editor ran throughout); the load averages say how busy it was.

| Sequence | Added load | Began p50 / p95 / p99 / max (ms) | Sent p50 / p95 / p99 / max (ms) | Drift (ms/min) | Load average, before → after | In step |
| --- | --- | --- | --- | --- | --- | --- |
| typical | none | 1.91 / 2.91 / 3.23 / 6.70 | 13.0 / 18.1 / 22.5 / 23.7 | +0.14 ± 0.09 | 3.4 → 1.9 | yes |
| typical | 30 busy threads | 1.83 / 3.21 / 3.93 / 6.10 | 14.3 / 20.3 / 22.3 / 24.1 | +0.03 ± 0.10 | 3.3 → 42.0 | yes |
| worst | none | 1.96 / 2.79 / 3.04 / 3.50 | 14.2 / 19.1 / 20.5 / 31.1 | +0.12 ± 0.05 | 1.9 → 3.3 | yes |
| worst | 30 busy threads | 8.65 / 22.7 / 24.6 / 25.0 | 52.0 / 102.1 / 139.5 / 251.2 | -0.36 ± 0.58 | 42.0 → 51.3 | NO |

| Sequence | Added load | Frames due | Missed | Sent again | In flight at a jump | Jump to wire p50 / max ms (jumps) | New round to wire p50 / max ms (rounds) | Round late p50 / max ms (rounds) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| typical | none | 4800 | 0 | 0 | 0 | 17.8 / 19.0 (7) | 9.78 / 14.0 (3) | 3.13 / 3.22 (3) |
| typical | 30 busy threads | 4798 | 0 | 0 | 1 | 11.9 / 24.3 (7) | 14.2 / 16.0 (3) | 3.74 / 3.79 (3) |
| worst | none | 4800 | 0 | 0 | 1 | 12.4 / 23.1 (7) | 6.96 / 17.8 (3) | 2.18 / 2.36 (3) |
| worst | 30 busy threads | 4769 | 1809 | 0 | 10 | 84.8 / 109.9 (7) | 64.8 / 84.1 (3) | 28.3 / 39.0 (3) |

What they say:

- **The typical sequence keeps in step, busy machine or not.** Its frames are drawn on the
  player's thread. The player begins a frame 1.9 ms into its time at the median and 3.2 ms at the
  99th percentile (the simulations hold it to 3.6); the frame is on the wire 13 ms into its own
  25 at the median and 22.5 ms at the 99th percentile. With thirty threads fighting it for
  fifteen cores nothing moves by more than a millisecond or two: a thread that sleeps most of
  each frame is given a core when it wakes.
- **The worst case keeps in step too, on a machine with cores to spare.** Its frames are shared
  out among four threads and drawn in 5 to 7 ms where one thread took 16 to 22. They go out
  14 ms into their time at the median and 20.5 ms at the 99th percentile, and none is missed. On
  one thread it was out of step: 30 ms at the median, 56 at the 99th percentile, 20 frames
  missed.
- **With every core taken, the worst case still falls out of step.** The threads its frames are
  shared among wait for cores that thirty busy threads want too, and 1,809 of 4,769 frames are
  skipped. One thread did worse (2,656 of 4,755), having no time to spare to begin with.
  Skipping is what keeps the lights with the music: the player draws the frame that is due,
  however many it missed, so the lateness stays the time one frame takes to draw and send. This
  is the case that [having the asking thread help draw](#recommendations), and a higher
  priority for the threads that play a show, are for.
- **Nothing builds up.** Over two minutes the line through *began* moves by a seventh of a
  millisecond a minute at most in the three runs that keep up, and by less than its own
  uncertainty in the one that can't. No frame went out again after a later one.
- **A jump is on the wire within 25 ms** and a new round of the loop within 18 ms, and the music
  is taken back within 4 ms of the lights' end, in the runs that keep up.

What these runs do not show: a sound device (the clock here is the machine's own, exact and never
late), a real network, or how the estimate of what is being heard behaves on a device whose
buffers differ from what it reports. The simulations cover a device whose clock runs fast or slow
or stalls; a real sound card's delay is still set by ear, with the sequence's offset.

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
  doing other things. Single runs disagree, most of all in their slowest frames.
- **One kind of machine for the threads.** How many threads to share a frame out among, and from
  what size of frame, were chosen on a machine with five fast cores and ten slower ones that was
  busy with other work. A machine with eight cores of one kind and nothing else to do may want
  different numbers; `PIXELFLOW_RENDER_THREADS` and `--render-threads` are there to find out.
- **The paced runs are playback rebuilt, not playback.** They use the engine's renderer, mapping,
  packets, transport, and frame clock, with the player's and output thread's loops written again
  around a stopwatch. `--mode engine` and the byte-for-byte test are the checks that they match.
- **Timings on a shared CI runner mean nothing.** The CI smoke test asserts that the harness
  works, never how fast.
