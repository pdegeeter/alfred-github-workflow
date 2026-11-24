# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Keep everything in sync (always)

Every code change must update its tests, the docs under `docs/`, the `README.md`,
and this `CLAUDE.md` in the **same** change — never as a follow-up. If you change
behaviour (a keyword, a modifier, an action verb, a GraphQL query, a cache TTL,
the plist, the release flow), find and fix every reference to the old behaviour
across `README.md`, `docs/`, `CLAUDE.md`, `workflow/info.plist`'s `readme`, and
doc comments before committing. `cargo test --all`,
`cargo clippy --all-targets -- -D warnings`, and `cargo fmt --all` must all pass.

## What this is

An Alfred (macOS) workflow written in Rust to navigate GitHub. A single binary,
`alfred-gh`, backs every object of the Alfred graph:

- **Script Filters**, one per keyword: `repos` (`gho`), `prs` (`ghp`),
  `workflows` (`gha`), `review` (`ghr`), `mine` (`ghm`). They print Alfred
  JSON.
- One **Run Script**, `alfred-gh run <arg>`, connected to every script filter
  for every modifier used. `arg` is an encoded `action::Action`
  (`"<verb> <payload>"`, see `docs/architecture.md`). Whatever it prints feeds a
  Post Notification.
- `alfred-gh refresh <key>` is not in the graph: script filters spawn it
  detached to rewrite a stale cache entry.

The binary ships inside `workflow/` (with `info.plist`) and is packaged into a
`.alfredworkflow` file for distribution.

## Commands

```sh
cargo test --all                          # full suite (offline, no GitHub access needed)
cargo test --test action_test             # one test file
cargo clippy --all-targets -- -D warnings # lint; warnings are errors (CI gate)
cargo fmt --all                           # format (CI checks with --check)

./scripts/build.sh            # sync plist version + build release + package .alfredworkflow
./scripts/deploy.sh           # build + update the installed workflow in place (imports it the first time)
./scripts/deploy.sh --import  # build + open the .alfredworkflow to import it
./scripts/icon.sh             # regenerate workflow/icon.png from scripts/icon.svg

# Exercise the binary directly (it speaks Alfred JSON on stdout):
cargo build && alfred_workflow_cache=/tmp/agh GH_OWNERS=my-org,my-user \
  ./target/debug/alfred-gh repos api | python3 -m json.tool
```

`powerpack-cli` is required for build/deploy/packaging: `cargo install powerpack-cli`.

## Architecture

**Everything is a pure function except one trait.** Every process spawn —
`gh`, `osascript` (clipboard, Alfred) and `/usr/bin/open` — goes through
`runner::CommandRunner` (`SystemRunner` in prod). Tests inject `FakeRunner`
(`tests/common/mod.rs`), which returns queued canned output per program and
records invocations and detached spawns, so the suite runs offline. Keep new
logic pure and route any new spawn through it.

`CommandRunner` differs from the Brave workflow it is modelled on in two ways:
a failed `run` returns a `runner::CommandError` that keeps **stdout** as well as
stderr (gh prints GraphQL errors as JSON on stdout and exits 1), reachable with
`err.downcast_ref::<CommandError>()`; and `spawn_detached` starts a background
process without waiting (stale-cache refreshes).

- `runner.rs` — the process boundary (`CommandRunner`, `SystemRunner`, `CommandError`).
- `gh.rs` — `gh` resolved by absolute path (`/opt/homebrew/bin/gh`, then
  `/usr/local/bin/gh`: Alfred has no shell `PATH`); `gh api graphql` with fixed
  query constants and variables as `-f name=value` (never `-F`, which reads
  `@file` values); `gh api <path>` for REST; `GhError` classification
  (missing, not authenticated, rate limited + reset from `gh api rate_limit`,
  SSO from `FORBIDDEN` / SAML errors, other).
- `owners.rs` — `GH_OWNERS` parsing, `OwnerKind` from `__typename`, search
  qualifiers (`org:x user:y`), owner prefixes (`a:` / `acme-org:`).
- `cache.rs` — stale-while-revalidate JSON entries under
  `$alfred_workflow_cache` (`Key` ⇒ file stem, TTL, `refresh` argument),
  freshness from the stored `fetched_at` and an injected clock, refresh locks.
- `app.rs` — orchestration: `Settings` (workflow variables), `App::load`
  (fresh ⇒ serve; stale ⇒ serve + spawn `refresh` + `rerun`; missing ⇒ fetch),
  `App::refresh`, one method per script filter.
- `repos.rs` — `repositoryOwner(login:)` pages (`ownerAffiliations: [OWNER]`),
  pagination, `Repo` / `OwnerRepos`.
- `prs.rs` — `Pr`, one fixed query per state set (`OPEN_PRS_QUERY`…), CI
  rollup and review state, the cross-owner `Search` (`ghr` / `ghm`). `jira.rs` — ticket key extraction.
- `workflows.rs` — Actions over REST: workflows, the last 100 runs on the
  default branch in one call grouped by workflow, one workflow's last 10 runs.
- `ranking.rs` — repository ranking, repository resolution (`Resolution`),
  pull request filtering. `query.rs` — keyword query parsing.
- `local.rs` — clone path `~/Workspace/github.com/<owner>/<repo>` and
  `owner/repo` validation. `time.rs` — ISO 8601 parsing, relative ages.
- `action.rs` — the `arg` protocol (`Action::to_arg` / `Action::parse`) and
  `execute` (with a `Context`: runner, gh path, home, cache, `exists`).
- `alfred.rs` — item builders, `gh` error items and the script filter output
  (`rerun`).

## Conventions that matter here

- **No employer or private names in this public repo.** Owners and the Jira
  URL are user configuration (Alfred's *Configure Workflow…*, stored in the
  git-ignored `workflow/prefs.plist`); code defaults stay generic. Test fixtures
  use the fictitious owners `acme-org` (organization) and `octo-user` (user).
- **The `arg` protocol**: only the last payload field may contain spaces.
  Adding a verb means updating `to_arg`, `parse`, `execute`, the round-trip test
  in `tests/action_test.rs` and the table in `docs/architecture.md`.
- **Modifier connections**: every script filter has one connection per
  modifier used (⌘, ⌥, ⌃, ⇧⌘) to the Run Script. Adding a new modifier
  combination means adding its connection in `workflow/info.plist`.
- **No `uid` on Alfred items** — Alfred would re-sort them. Filtering and
  ordering are done in Rust (`alfredfiltersresults = false`).
- **Script filters never hard-fail**: errors become an invalid Alfred item.
  One owner failing never hides another owner's results.
- **Cache**: each owner (and later each repository's PRs, workflows, runs)
  has its own entry and TTL (`Key::ttl`). A failed background refresh keeps
  its lock until it expires (60 s), which is the back-off. An organization
  returning no repository is never cached (likely SSO; shows the SSO item).
- **Keywords are fixed.** Drill-downs (`reopen`) build queries from
  `alfred::PRS_KEYWORD` and `alfred::WORKFLOWS_KEYWORD`. `tests/plist_test.rs` pins the keyword → subcommand
  map, the modifier connections and the user configuration variables: update it
  with any keyword, modifier or variable change.
- **Deploying**: `scripts/deploy.sh` copies the build over the installed
  workflow found by bundle id (never `powerpack link`: Alfred 6 turns symlinks
  into stale copies and fails to re-import over an install).
- **Editing `workflow/info.plist`**: if you edit the workflow in Alfred's UI,
  copy the updated `info.plist` from the installed folder back into `workflow/`
  before committing (the next deploy overwrites it), and validate with
  `plutil -lint workflow/info.plist`. The binary in `workflow/` is a build
  artifact (gitignored); `info.plist` and `icon.png` are tracked.
- **Workflow icon**: `workflow/icon.png` is rendered from `scripts/icon.svg` by
  `scripts/icon.sh` (`rsvg-convert`). It is committed and not regenerated at
  build time.
- **Protected `main`**: work via PRs only (enforced for admins). CI `test` and
  `package` must be green, and **all commits must be signed** (SSH signing) to
  merge. See `docs/development.md`.

## Releases

Automated via release-please (`release-type: rust`). Land Conventional Commits
on `main` (`feat:`/`fix:`/`chore:`…); release-please opens a Release PR that
bumps `Cargo.toml`/`Cargo.lock` and `CHANGELOG.md`. Merging it tags `vX.Y.Z`,
creates the GitHub Release, and the `build-and-upload` job attaches the
`.alfredworkflow` (arm64) as a release asset. Do not bump versions by hand.
See `docs/release.md`.

Target platform is **macOS arm64 only**; CI and release build on `macos-latest`.
