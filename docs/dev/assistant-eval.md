# Evaluating the Assistant

How good are the light shows the Assistant makes? The tests in `crates/pf-ai/tests` say whether
it *can* do everything and whether its knowledge is complete. This harness says how well a given
model *does*, on requests a person would really make.

It never touches your show. Everything it works on is made up for the purpose: a display that
doesn't exist, a song that was never recorded, words nobody wrote down anywhere else.

## What it runs

Nine requests, each typed into the real chat loop (`ChatSession::run_turn`) exactly as the app
would send it, with the Assistant's real instructions, tools, and guides:

| Scenario | The request | What is open |
|---|---|---|
| `whole-song` | "Make a light show for this song." | a new sequence, nothing on it |
| `dancers` | "Put dancing skeletons on the matrix for the first chorus." | a simple red, green, and white sequence |
| `sing-along` | "Have the singing face sing the words, and put the words of each chorus up on the matrix as they are sung." | the same |
| `build` | "Build up into the first chorus over the last two bars of the verse before it." | the same |
| `calm-and-big` | "It all looks the same from start to finish. Make the verses calmer and the choruses bigger." | one look held for the whole song |
| `halloween` | "Recolor this for Halloween." | the red, green, and white sequence |
| `review-and-fix` | "Review this sequence and fix what's wrong with it." | a sequence with faults: a shimmer over the whole house, a rainbow, tiny words, a wrong effect on a string, props left out, a dark last chorus |
| `scroll-words` | "Scroll LANTERNS HIGH across the matrix through the last chorus." | the red, green, and white sequence |
| `question` | "What's the difference between the Chase and Bars effects on my arches? Don't change anything." | the same |

The display is `pf_ai::stage`: a roofline, two windows, a 40×20 matrix, a mega tree with a star,
four arches, candy canes, and a singing face, with three groups. The song is "Paper Lanterns",
96 seconds at 120 beats a minute, whose sections, loudness, and moments (a build, a drop, a stop
before the last chorus) are written by hand in the same file, as are its eight lines of words.
No audio is analyzed: `analyze_song` is answered with that hand-written analysis.

## How an answer is scored

- **The design review** (`review_draft`'s own): the score out of 100, each criterion, and the
  fixes it still wants.
- **Plain measures**, taken from the frames the engine draws for the finished draft: how many of
  the 12 props have a part, how many kinds of effect, the share of the song with nothing lit and
  with three quarters of the props lit, at how many of the 7 section starts the look changes,
  how many of the biggest moments are hit, whether anything flashes unsafely, how bright the
  verses are against the choruses.
- **Checks for the request itself**: the skeletons are skeletons and are on the matrix for the
  first chorus; the rest of the song was left alone; a recolor kept every effect where it was;
  the words can be read; a question was answered without changing anything; a small job took few
  steps; the method guide was read before the first effect was placed.
- **What it took**: model requests, tool calls by name, tool calls refused (with the reason),
  guides read, and the size of everything sent and received.

`crates/pf-ai/src/eval/scenarios.rs` has each scenario's checks; adding a scenario is adding an
entry there (and a scripted answer in `stand_in.rs`, which the tests require to pass it).

## Running it

Without a model (nothing is sent; every answer is a script written by hand, which proves the
harness and shows what a report looks like, and says nothing about any model):

```
cargo run --release -p pf-ai --example assistant_eval
cargo run --release -p pf-ai --example assistant_eval -- --list
```

Against a model, with your own key. First see what would be sent and roughly what it would cost
(this still sends nothing):

```
cargo run --release -p pf-ai --example assistant_eval -- --provider anthropic --model <model id>
```

Then run it:

```
PIXELFLOW_EVAL_KEY=<your key> cargo run --release -p pf-ai --example assistant_eval -- \
    --provider anthropic --model <model id> --send --out ~/pixelflow-eval/first-run
```

- `--provider` is `anthropic` or `openai`; `--model` is the model's id as that company names it.
- `--send` is what makes it real. Without it no provider is contacted and the key is not read.
- `--scenario whole-song,halloween` runs only those.
- `--runs 3` answers each scenario three times: models vary from one run to the next, and one
  run is an anecdote.
- `--out <folder>` writes `report.md` (what is printed) and `results.json` (every measure, and
  every tool call the model made with the start of each answer). Keep that folder outside the
  repository: results are not committed.

The key is read from the `PIXELFLOW_EVAL_KEY` environment variable, by the example binary, only
on a run with `--send`. It is not read from the app's saved key, and it is not written anywhere.

## What a real run sends

To the provider you name, over HTTPS, with your key as the sign-in, exactly what the app sends
for a chat:

- the Assistant's instructions and tool definitions (about 51 KB, with every request);
- each scenario's request, as written above;
- whatever the model looks up with its tools: PixelFlow's own guides, and the made-up show,
  sequence, song analysis, and words described above.

Nothing of yours is read or sent: no show, sequence, song, lyrics, picture, file, or device.
Requests to Anthropic ask for prompt caching; requests to OpenAI ask it not to store the
conversation, as in the app.

## What it costs, roughly

The harness counts the text it sends and receives and turns that into tokens at about 3.5 bytes
each. It does not know anyone's prices: `--price-in` and `--price-out` (dollars for a million
tokens) and `--price-cached` (a cached token's price as a share of `--price-in`) are yours to
set from the provider's price list, and the defaults (3, 15, and 0.1) are placeholders.

When this was written, the scripted answers to all nine scenarios took 45 requests and sent
about 990,000 tokens in all, receiving about 4,000 (the dry run prints today's figures as the
guides grow): nearly all of the cost is the conversation being sent again with
each request, which is what a provider's cache makes cheap. A model takes more steps than a
script that already knows what it will do, reads more guides, and writes more: allow two to four
times that. At the placeholder prices, that is roughly:

| | The scripted answers | A model, without caching | A model, with the repeated part cached |
|---|---|---|---|
| All nine scenarios, once | $3.03 ($0.92 cached) | $6 to $12 | $2 to $4 |
| `whole-song` alone | $0.97 ($0.23) | $2 to $4 | $0.50 to $1 |
| A small one (`dancers`, `halloween`) | $0.21 ($0.07) | $0.40 to $0.85 | $0.15 to $0.30 |

Those are estimates from sizes, not bills: the provider's own usage page has the real figure, and
what a provider charges to write to its cache isn't counted.
The dry run (`--provider` and `--model` without `--send`) prints the estimate for the scenarios
and prices you give it.

## What it has and hasn't been run against

The harness is tested with the scripted provider the crate's other tests use
(`crates/pf-ai/tests/evaluation.rs`): a careful scripted answer passes every scenario's checks,
an idle one fails them, a refused tool call and a failing provider are reported rather than
fatal, and the command line makes no provider and reads no key unless it is given `--send`. It
has not been run against a real model as part of building it.
