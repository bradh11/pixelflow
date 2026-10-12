# Contributing

Thank you for wanting to help. PixelFlow is maintained by one person in spare time, so small,
focused changes that come with their tests are the ones that get merged soonest.

Ways to help that need no code: report a bug, share an xLights show or sequence that imports
badly (with anything personal taken out), try a build on your own controllers and fill in the
[hardware acceptance checklist](docs/hardware-acceptance.md), or improve a page of the docs.

By taking part you agree to the [code of conduct](CODE_OF_CONDUCT.md).

## Your contribution's licence

PixelFlow is licensed under the [GNU General Public License, version 3](LICENSE). By opening a
pull request you agree that your contribution is offered under that same licence, and you
confirm that you wrote it or otherwise have the right to offer it. There is no contributor
agreement to sign.

If your change draws on someone else's code or data, say so in the pull request: what it is,
where it came from, and its licence. Code that follows another project's source is credited in
[`NOTICE`](NOTICE) and listed in [docs/licensing/provenance.md](docs/licensing/provenance.md);
code whose licence cannot be combined with the GPL cannot be accepted. New work should be
PixelFlow's own design: describe what an effect or a feature should do and build that, without
translating another program's source.

## Build and test

What you need is in the [README](README.md#what-you-need). Then:

```sh
make setup   # install the app's dependencies (once, and after pulling)
make run     # the desktop app, with live reload
make ui      # the UI alone in a browser, with a sample show: http://localhost:1420/?demo
make test    # every test
make lint    # formatting and lints, as CI runs them
```

The repository has three parts, each with its own tests:

| Part | Where | Check it with |
|---|---|---|
| The engine (Rust crates) | `crates/` | `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace --locked` |
| The desktop shell (Tauri, Rust) | `app/src-tauri/` (its own Cargo workspace) | from that folder: `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked` |
| The window (React, TypeScript) | `app/src/` | from `app/`: `pnpm typecheck`, `pnpm test`, `pnpm build` |

CI runs exactly those, on Linux and Windows for the engine and on macOS and Windows for the app,
and must pass before a change is merged. Two more workflows run on every push and pull request:
a scan for committed secrets, and a check of the dependencies against the advisory databases
(`make audit`).

Some files are generated and committed. A test fails when one is stale, and says how to
regenerate it:

- **The contracts between the window and the shell** (`app/src/api/contracts/`): after adding
  or changing a command, a reply, or an event. See [docs/dev/contracts.md](docs/dev/contracts.md).
- **The effect catalog the browser stand-in uses** (`app/src/api/effectCatalog.json`): after
  changing an effect's settings. `PIXELFLOW_UPDATE_CATALOG=1 cargo test` in `app/src-tauri`.
- **The licence inventory and the notices the app bundles**: after a lockfile changes.
  `make licenses`.
- **The version**, which several files state: `make bump VERSION=X.Y.Z`.

## House rules

- **No `unsafe`.** Both Cargo workspaces forbid it (`unsafe_code = "forbid"`), and a change that
  needs it will not be merged.
- **Rendering is deterministic.** Frame N depends only on the sequence, the show, the music, and
  the pictures it uses: never on the clock, the machine, or how many threads drew it. An effect
  that needs chance takes it from its own seed and the frame number
  (`crates/pf-render/src/effects.rs`), so seeking, the preview, the lights, and an exported video
  all show the same picture. The one known exception is written down at the top of
  `crates/pf-render/src/lib.rs`; do not add another.
- **No new dependency without a reason.** Each one is more to audit, to license, and to build.
  Say in the pull request why it is needed, and run `make licenses`: its check fails on a licence
  that cannot be combined with the GPL.
- **PixelFlow only changes a controller when the user asks.** Finding and reading are read-only.
  Anything that writes to a device shows what will change first.
- **Nothing is fetched behind the user's back.** What the app sends, and when, is listed in
  [docs/privacy-and-data-flow.md](docs/privacy-and-data-flow.md); a change that adds to it
  updates that page. The window itself makes no network requests: links are handed to the
  system's browser by the shell, from a fixed list.
- **Plain words in the UI.** Say what happened and what to do next, in the words the screens
  already use. Work screens give their room to the canvas: no crowding, no wasted space.
- **Tests come with the change**: one per behaviour someone could notice, named for it.
- **Nothing personal in fixtures.** Before committing a recording from a real controller or a
  real show, replace every name, address, and serial with a made-up one
  (`crates/pf-devices/fixtures/README.md`).

## Adding or changing an effect

An effect has four parts, and a pull request that adds one brings all four:

1. **Its settings**, in `crates/pf-sequence/src/effect.rs`: the kind, each setting's range and
   default, and a one-line description of each. The UI's controls and the Assistant's reference
   are generated from this table.
2. **Its drawing**, in `crates/pf-render/`, deterministic as above, with tests that assert what
   it shows (which cells light, where, in what color), not how it computes it.
3. **Its guide for the Assistant**, in `crates/pf-ai/guides/effects/<kind>.md`. The Assistant can
   only use well what it has been taught: the build stops until a new kind has a guide, and
   `cargo test -p pf-ai --test knowledge` says what the guide must still cover.
   [docs/dev/assistant-guides.md](docs/dev/assistant-guides.md) has the steps, and
   [docs/dev/assistant-capabilities.md](docs/dev/assistant-capabilities.md) the rule behind
   them: anything a person can do by hand, the Assistant can draft.
4. **Its import from xLights**, if xLights has an effect it should stand in for
   (`crates/pf-xlights/src/sequence/effects.rs`), judged by whether the import looks the same
   ([docs/fidelity.md](docs/fidelity.md)).

Then regenerate the effect catalog (above), and run the scale benchmark if the effect may be
slow on a large display ([docs/benchmarks.md](docs/benchmarks.md)).

## Branches

| Branch | Purpose |
|---|---|
| `main` | Production. Only receives merges from `develop` (releases) or `hotfix/*`. Every release is a tag on `main`. |
| `develop` | Integration branch and the default branch. All feature PRs target `develop`. |
| `feature/<issue>-<slug>` | New work, branched from `develop`. Also `fix/`, `docs/`, `chore/`. |
| `hotfix/<issue>-<slug>` | Urgent production fixes, branched from `main`, merged to `main` and `develop`. |

## Workflow

1. Every change starts with a GitHub issue. For anything larger than a fix, wait for a reply on
   the issue before writing much: it saves work that would not be merged.
2. Fork the repository, and branch from `develop`: `git switch -c feature/12-sacn-sender develop`.
3. Open a PR into `develop` that references the issue (`Closes #12`).
4. CI must pass before merging.

## Releases

1. Set the version (`make bump VERSION=X.Y.Z`) and open a PR from `develop` to `main`.
2. After merging, tag `main` with that version (`vX.Y.Z`). The release workflow builds the installers and drafts a GitHub Release; read it over and publish it.

The whole procedure, what must be true first, and how signing is set up are in [docs/releasing.md](docs/releasing.md).

## Commit messages

Use [Conventional Commits](https://www.conventionalcommits.org/): `feat:`, `fix:`, `docs:`, `chore:`, `refactor:`, `test:`, `perf:`.

## Secrets

Never commit secrets: API keys, tokens, passwords, private keys, or `.env` files. PixelFlow stores user API keys in the OS keychain, never in files.

Enable the secret-scanning pre-commit hook once per clone (requires [gitleaks](https://github.com/gitleaks/gitleaks)):

```sh
git config core.hooksPath .githooks
```

CI also scans every push and PR, and GitHub push protection is enabled.

## Security

Please report a vulnerability privately, not in an issue or a pull request: see
[SECURITY.md](SECURITY.md).

## Donation links

The ways to support the project are listed in two files, and nowhere else:

- `.github/FUNDING.yml`: GitHub's own file, which puts the **Sponsor** button on the
  repository's page.
- `app/support.json`: what the app's **Support PixelFlow** entry lists.

Each holds an account name per service. A name left empty shows nothing anywhere, and a test
(`app/src/lib/support.test.ts`) fails if the two files disagree. Only the maintainer sets them;
a pull request that changes either will be declined.
