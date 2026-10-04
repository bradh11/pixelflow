# Contributing

## Branches

| Branch | Purpose |
|---|---|
| `main` | Production. Only receives merges from `develop` (releases) or `hotfix/*`. Every release is a tag on `main`. |
| `develop` | Integration branch and the default branch. All feature PRs target `develop`. |
| `feature/<issue>-<slug>` | New work, branched from `develop`. Also `fix/`, `docs/`, `chore/`. |
| `hotfix/<issue>-<slug>` | Urgent production fixes, branched from `main`, merged to `main` and `develop`. |

## Workflow

1. Every change starts with a GitHub issue.
2. Branch from `develop`: `git switch -c feature/12-sacn-sender develop`.
3. Open a PR into `develop` that references the issue (`Closes #12`).
4. CI must pass before merging.

## Releases

1. Open a PR from `develop` to `main`.
2. After merging, tag `main` with a semantic version (`vX.Y.Z`) and publish a GitHub Release with notes.

## Commit messages

Use [Conventional Commits](https://www.conventionalcommits.org/): `feat:`, `fix:`, `docs:`, `chore:`, `refactor:`, `test:`, `perf:`.

## Secrets

Never commit secrets: API keys, tokens, passwords, private keys, or `.env` files. PixelFlow stores user API keys in the OS keychain, never in files.

Enable the secret-scanning pre-commit hook once per clone (requires [gitleaks](https://github.com/gitleaks/gitleaks)):

```sh
git config core.hooksPath .githooks
```

CI also scans every push and PR, and GitHub push protection is enabled.
