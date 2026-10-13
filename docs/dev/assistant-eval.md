# Evaluating the Assistant

How good are the light shows the Assistant makes? The tests in `crates/pf-ai/tests` say whether
it *can* do everything and whether its knowledge is complete. This harness says how well a given
model *does*, on requests a person would really make.

It never touches your show. Everything it works on is made up for the purpose: a display that
doesn't exist, a song that was never recorded, words nobody wrote down anywhere else.

## What it runs

Nine requests, each typed into the real chat loop (`ChatSession::run_turn`) exactly as the app
would send it, with the Assistant's real instructions, tools, and guides. By default big jobs
are built in stages, as the app does with **Plan big jobs first** on (a brief, a checked design,
the plan's steps built at once in small conversations, a review and a refine: see
[assistant-guides.md](assistant-guides.md#big-jobs-in-stages)); `--flow single` runs every job
in one conversation instead, as before:

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
before the last chorus) are written by hand in the same file, as are its ten lines of words
(two in each verse and each chorus, the last chorus too).
No audio is analyzed: the first message's context block and `analyze_song` give that
hand-written analysis.

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
  the words of every chorus are up as they are sung, and can be read (by the review's rule: 300
  ms and 60 ms a letter); a build's second bar is brighter, or lights more props, than its
  first; a question was answered without changing anything; a small job took few steps; the
  method guide was read before the first effect was placed; the song was listened to (its
  analysis came with the request, or was asked for).
- **What it took**: model requests, tool calls by name, tool calls refused (with the reason),
  guides read, the size of everything sent and received, and (on a real run) the tokens the
  provider billed and what they cost.

The report opens with what a person waits through, a row per scenario: whether it was built in
stages, the requests (each a round trip to the model, every stage's counted), the **critical
path** (the most requests that had to happen one after another: a stage waits for the one
before it, but the steps of a wave run at once, so this is what a person waits through), the
tool calls (every stage's), the tokens sent with the first request and with a request on average
(as billed when the provider reported them; marked `~` when estimated from sizes, as for the
stand-in), the largest request, and on a real run the wall-clock time from the first request to
the last reply. Those columns compare directly with an earlier run's. In a staged run the
transcript holds every stage's calls in turn, each with its `stage` ("chat", "design",
`build step "verses"`, "refine", "refine 2") and the `request` of that conversation that made
it (calls with the same request were sent together), and the guides a stage was given with its
request show as a `get_guide` marked `given` (counted as read, not as a call); "It reviewed its
draft" passes for a staged build, which reviews every draft it builds.

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
cargo run --release -p pf-ai --example assistant_eval -- --provider openai --model <model id>
```

Then run it, with a ceiling on what it may spend and the model's prices from the provider's
price list:

```
PIXELFLOW_EVAL_KEY=<your key> cargo run --release -p pf-ai --example assistant_eval -- \
    --provider openai --model <model id> --send --max-usd 10 \
    --price-in <$ per million tokens sent> --price-out <$ per million received> \
    --price-cached <cached input price as a share of --price-in> \
    --out ~/pixelflow-eval/first-run
```

For Anthropic, add `--price-cache-write <share of --price-in>` (what writing to its cache costs).

- `--provider` is `anthropic` or `openai`; `--model` is the model's id as that company names it.
- `--send` is what makes it real. Without it no provider is contacted and the key is not read.
- `--max-usd <dollars>` is the most the whole run may spend; `--send` won't start without it
  (see [Spending](#spending)). `--max-requests <n>` is a second, plainer brake: at most that many
  requests in the whole run.
- `--scenario whole-song,halloween` runs only those.
- `--flow staged` (the default) or `--flow single`: how big jobs are done (see above). Run both
  to compare the two on the same model.
- `--runs 3` answers each scenario three times: models vary from one run to the next, and one
  run is an anecdote.
- `--out <folder>` writes `report.md` (what is printed) and `results.json` (the model, the prices,
  the ceiling, what was spent and the tokens billed, the scenarios the budget kept from starting,
  and for each scenario every measure, its tokens billed, and every tool call the model made with
  the start of each answer). Keep that folder outside the repository: results are not committed.

The key is read from the `PIXELFLOW_EVAL_KEY` environment variable, by the example binary, only
on a run with `--send`, and only once the ceiling and prices have been accepted. It is not read
from the app's saved key, and it is not written anywhere.

## What a real run sends

To the provider you name, over HTTPS, with your key as the sign-in, exactly what the app sends
for a chat:

- the Assistant's instructions and core tool definitions (about 20 KB, with every request), and
  the rest of the tools, a group at a time, when the model asks for them with `use_tools`;
- each scenario's request, as written above, with the made-up show, sequence, and song in
  outline in its context block, as the app sends them with a first message;
- whatever the model looks up with its tools: PixelFlow's own guides, and the made-up show,
  sequence, song analysis, and words described above;
- for a job built in stages: a design request (the stages' instructions, the catalogue of tools
  and effects, and the show-design guides, the same every time, with the brief and the made-up
  show and song in outline), a request for each step of its plan (only that step's tools,
  stretch of the song, props, and effects' guides, and what its part still lacks when it is
  asked again), and a refine request or two with what of the plan is still dark and the review's
  fixes. Several steps' requests may be on their way at once.

Nothing of yours is read or sent: no show, sequence, song, lyrics, picture, file, or device.
Requests to Anthropic ask for prompt caching; requests to OpenAI ask it not to store the
conversation, as in the app.

## Spending

A run with `--send` spends real money, so it keeps to a hard ceiling that it can't pass.

**It won't start without one.** `--send` needs `--max-usd`, and the prices given explicitly:
`--price-in` and `--price-out` (dollars for a million tokens), `--price-cached` (a token read from
the provider's cache, as a share of `--price-in`), and for Anthropic `--price-cache-write` (a
token written to its cache, as a share of `--price-in`; OpenAI doesn't charge for that). With
any of them missing it stops before making a provider or reading the key, and says which. It
prints the prices and ceiling it will use before the first request.

**Before every request** it works out the most that request could cost (and holds that against
the ceiling, beside the worst cases of any requests still on their way, until the request is
charged what it really cost: a staged build sends several at once): every byte of the body it
would send counted as a token (no token is shorter than a byte, so this is more than the provider
will count), plus 4,096 tokens for what the provider wraps around it, all at the full input price
(or the cache-write price, where that is dearer); plus the request's own output limit (32,000
tokens, the chat loop's `MAX_OUTPUT_TOKENS`, which both providers are sent and which counts
reasoning and thinking) at the output price. An Anthropic model that takes server-side fallbacks
could be billed for a second model's answer too, so its worst case counts twice. If what has been
spent plus that worst case would pass the ceiling, the request is not sent: the scenario stops
there, is marked "stopped: budget" in the report, and no further scenario starts. `--max-requests`
stops the run the same way ("stopped: request limit").

**After every request** it adds what the request really cost, from the tokens the provider
reports for it: OpenAI's `usage` on the final `response.completed` (or `response.incomplete`)
event, with the cached part of `input_tokens` at the cached price (the Responses API reports it on
every streamed reply; it needs no stream option); Anthropic's `usage` on `message_start` and the
running totals on `message_delta`: input, cache reads, cache writes, and output. A reply that
reports nothing (a failed request, say) is counted at its worst case.

It prints what has been spent after each scenario and the final total at the end, and the report
gives the tokens billed and their cost for the run and for each scenario, beside the sizes below.
The ceiling holds against the prices you give it: if they are lower than the provider's, so is
the real bill's limit. The provider's own usage page is still the last word.

One request to the chat loop can be sent more than once only when the earlier try ran nothing:
it never reached the provider, the provider said "come back later" (429, 503, or 529 with a
`retry-after`), or it refused an option with a 400 and PixelFlow sends it once more without that
option. None of those is billed, so each request is counted once.

## What it costs, roughly

Besides the tokens billed, the harness counts the text it sends and receives and turns that into
tokens at about 3.5 bytes each: that is the estimate the dry run gives before anything is sent.
`--price-in`, `--price-out`, and `--price-cached` set the prices it uses; without them (and only
without `--send`) the defaults (3, 15, and 0.1) are placeholders.

The scripted answers work the way the Assistant is asked to: they look up nothing the first
message's context block already gives, read what a step needs in one `get_guide` call (an
effect's guide in parts where that is enough), and lay a whole step in one call with `items`.
With `--flow staged`, the whole song and the calmer-and-bigger rework are handed to the staged
build (a brief, a plan of each part's base built at once and then the moments and singing, a
refine with nothing left to do); the rest are done in the chat as before. When this was written:

| Scripted answers | Requests (all nine) | Tokens sent | `whole-song`: requests, critical path, mean request | `calm-and-big`: the same |
|---|---|---|---|---|
| `--flow single` | 40 | about 484,000 | 6, 6, ~24,300 tokens | 5, 5, ~15,400 |
| `--flow staged` | 42 | about 359,000 | 6, 4, ~7,000 tokens | 7, 5, ~6,500 |

A script already knows what it will do, so it gains least from stages; a model that reads, places,
reviews, and fixes a call at a time gains most: each request carries only its own stage, and
the steps run at once. Nearly all of the single conversation's cost is the conversation being
sent again with each request, which is what a provider's cache makes cheap (the dry run prints
today's figures as the guides grow). A model takes more steps than a script that
already knows what it will do, reads more guides, and writes more: allow two to four times that.
At the placeholder prices, that is roughly:

| | The scripted answers | A model, without caching | A model, with the repeated part cached |
|---|---|---|---|
| All nine scenarios, once | $1.51 ($0.52 cached) | $3 to $6 | $1 to $2 |
| `whole-song` alone | $0.45 ($0.15) | $0.90 to $1.80 | $0.30 to $0.60 |
| A small one (`dancers`, `halloween`) | $0.10 ($0.04) | $0.20 to $0.40 | $0.07 to $0.14 |

Those are estimates from sizes, not bills; a run that sends reports the tokens actually billed
(see [Spending](#spending)). The dry run (`--provider` and `--model` without `--send`) prints the
estimate for the scenarios and prices you give it.

## What it has and hasn't been run against

The harness is tested with the scripted provider the crate's other tests use
(`crates/pf-ai/tests/evaluation.rs`, and `crates/pf-ai/tests/staged.rs` for the staged build,
whose stages each have their own script so steps built at once each get their own answers): a
careful scripted answer passes every scenario's checks in both flows,
an idle one fails them, a refused tool call and a failing provider are reported rather than
fatal, and the command line makes no provider and reads no key unless it is given `--send`.
The spending ceiling is tested with that provider reporting usage: `--send` refuses to start
without `--max-usd` and explicit prices; a request whose worst case would pass the ceiling is
not sent, and later scenarios don't start; the true total never passes the ceiling; a reply
without usage is counted at its worst case; the report and `results.json` carry the tokens
billed and their cost. The providers' reading of usage is tested against recorded replies
(`crates/pf-ai/tests/openai.rs`, `anthropic.rs`). It has not been run against a real model as
part of building it.

A real run's failures come back as tests: `crates/pf-ai/tests/staged_run4.rs` replays the two
staged builds that failed a check in run 4 (gpt-6.1-sol: `review-and-fix`'s last chorus left
dark, `calm-and-big`'s choruses on 8.4 props), every call the model made, a request's calls
together, from `tests/fixtures/run4` (row ids written as `@` and what the row lights). Replayed
as run 4's build ran them, they fail as run 4 did, with the same requests and measures; what a
step does when it is told what its part still lacks is added after the model's own turns, from
the plan's own words for that step, and runs only when the build asks for it.
