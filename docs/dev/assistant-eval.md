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
  verses are against the choruses. Each scenario lists the kinds of effect it used, and
  `results.json` has every color too.
- **What it sketched before drafting**: everything the Assistant wrote before its first call that
  changes the sequence, quoted under the scenario, and its proposal's summary. On a whole show
  that is the two or three concepts it sketched and the one it chose, and the summary names the
  one it built (see [the guides](assistant-guides.md#concepts-before-a-whole-show)). A small job
  usually writes nothing first.
- **How runs differ** (with `--runs` above 1): for each request, the kinds of effect and the
  colors every run used, those only some runs did, and how much each pair of runs has in common;
  then, run by run, the concepts it sketched before drafting and what its summary says it built.
  It is information, not a check: no show is marked down for its choice of effects, its colors,
  or its concept. Read the concepts and watch the videos to judge the variety.
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

The report opens with what a person waits through, a row per scenario: the requests (each a
round trip to the model), the tool calls, and the tokens sent with the first request and with a
request on average (as billed when the provider reported them; marked `~` when estimated from
sizes, as for the stand-in), and the largest request. Those columns compare directly with an
earlier run's.

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
- `--runs 3` answers each scenario three times: models vary from one run to the next, and one
  run is an anecdote.
- `--out <folder>` writes `report.md` (what is printed) and `results.json` (the model, the prices,
  the ceiling, what was spent and the tokens billed, the scenarios the budget kept from starting,
  and for each scenario every measure, its tokens billed, what it wrote before drafting
  (`concepts`) and its proposal's `summary`, and every tool call the model made with the start
  of each answer), and the shows themselves (see [Seeing the shows](#seeing-the-shows)).
  Keep that folder outside the repository: results are not committed.
- `--no-render`: with `--out`, save the drafts but draw no videos or contact sheets.

The key is read from the `PIXELFLOW_EVAL_KEY` environment variable, by the example binary, only
on a run with `--send`, and only once the ceiling and prices have been accepted. It is not read
from the app's saved key, and it is not written anywhere.

## Seeing the shows

Scores say how a show measures up; only looking says what it is like. With `--out`, every run
(the stand-in's too) leaves each scenario's final draft to look at, in three forms:

- `show/Paper Lanterns stage.pixelflow.json`: the made-up stage as a PixelFlow show, with its
  picture (`show/images/snowman.png`) and the song's sound (`show/paper-lanterns.wav`) beside
  it, and `show/<scenario>.pfseq.json`, each scenario's draft as a sequence that plays that
  sound. Open the show in PixelFlow, then any of the sequences. The song was never recorded: its
  sound is what the effects that follow the music hear in the harness, a thump on every beat and
  a steady tone as loud as each part, silent through the stop. With `--runs 3` the drafts are
  `whole-song-1`, `whole-song-2`, and so on.
- `videos/<scenario>.mp4`: the whole draft as a video of the 2D preview (1280 × 448, 25 frames a
  second, with that sound), drawn by `pf-video` from the saved files as the app would open them.
- `sheets/<scenario>.png`: a contact sheet of a frame from every section of the song (a beat into
  its middle bar), each labelled with the section, the time, and the effects playing then, under
  a line with the scenario, the model, its checks, its review score, and how many kinds of effect
  it used.

Everything is drawn here, offline; nothing is sent anywhere. Drawing all nine takes under a minute
in a release build. `--no-render` keeps the drafts and the stage and draws nothing.

## What a real run sends

To the provider you name, over HTTPS, with your key as the sign-in, exactly what the app sends
for a chat:

- the Assistant's instructions and core tool definitions (about 20 KB, with every request), and
  the rest of the tools, a group at a time, when the model asks for them with `use_tools`;
- each scenario's request, as written above, with the made-up show, sequence, and song in
  outline in its context block, as the app sends them with a first message;
- whatever the model looks up with its tools: PixelFlow's own guides, and the made-up show,
  sequence, song analysis, and words described above.

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

**Before every request** it works out the most that request could cost: every byte of the body it
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
The `whole-song` answer sketches three concepts in the reply that reads its effects' guides, as
the Assistant is asked to, so they cost it no request. When this was written they took 40
requests for all nine scenarios (`whole-song` 6) and sent about 493,000 tokens in all, receiving
about 3,900 (the dry run prints today's figures as the guides grow); the concepts added about
6,700 of those tokens sent and 250 received, all in `whole-song`, and no request. Nearly all of
the cost is the conversation being sent again with each request,
which is what a provider's cache makes cheap. A model takes more steps than a script that
already knows what it will do, reads more guides, and writes more: allow two to four times that.
At the placeholder prices, that is roughly:

| | The scripted answers | A model, without caching | A model, with the repeated part cached |
|---|---|---|---|
| All nine scenarios, once | $1.54 ($0.53 cached) | $3 to $6 | $1 to $2 |
| `whole-song` alone | $0.48 ($0.16) | $0.95 to $1.90 | $0.32 to $0.64 |
| A small one (`dancers`, `halloween`) | $0.10 ($0.04) | $0.20 to $0.40 | $0.07 to $0.14 |

Those are estimates from sizes, not bills; a run that sends reports the tokens actually billed
(see [Spending](#spending)). The dry run (`--provider` and `--model` without `--send`) prints the
estimate for the scenarios and prices you give it.

## What it has and hasn't been run against

The harness is tested with the scripted provider the crate's other tests use
(`crates/pf-ai/tests/evaluation.rs`): a careful scripted answer passes every scenario's checks,
an idle one fails them, a refused tool call and a failing provider are reported rather than
fatal, and the command line makes no provider and reads no key unless it is given `--send`.
The spending ceiling is tested with that provider reporting usage: `--send` refuses to start
without `--max-usd` and explicit prices; a request whose worst case would pass the ceiling is
not sent, and later scenarios don't start; the true total never passes the ceiling; a reply
without usage is counted at its worst case; the report and `results.json` carry the tokens
billed and their cost. The drafts a run keeps are tested the same way: the stage and each draft
open as the app opens them, every draft's rows light props of that saved stage, a question's
draft is what was open, and a draft is drawn as a video and a contact sheet with a frame for
every section. The providers' reading of usage is tested against recorded replies
(`crates/pf-ai/tests/openai.rs`, `anthropic.rs`). It has not been run against a real model as
part of building it.
