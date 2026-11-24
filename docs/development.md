# Development

## Contributing

`main` is a protected branch:

- Changes land via pull requests — direct pushes are rejected (enforced for
  admins too).
- The CI checks `test` and `package` must be green before a PR can be merged.
- **All commits must be signed** (GitHub "require signed commits"). Configure
  SSH commit signing so your commits verify:

  ```sh
  git config gpg.format ssh
  git config user.signingkey ~/.ssh/<your-key>.pub
  git config commit.gpgsign true
  ```

  The signing key must be registered on your GitHub account as a *signing* key,
  and your commit email must be a verified GitHub email, otherwise the signature
  will not verify and the push/merge will be blocked.

## Prerequisites

- Rust (stable) — `rustup` recommended.
- [`powerpack-cli`](https://crates.io/crates/powerpack-cli): `cargo install powerpack-cli`.
- The GitHub CLI (`brew install gh`), authenticated, for manual testing.
- Alfred 5 with Powerpack, to actually run the workflow.
- `rsvg-convert` (`brew install librsvg`), only to regenerate the icon.

## Common tasks

```sh
cargo test --all                          # run the test suite (offline, no GitHub access needed)
cargo clippy --all-targets -- -D warnings # lint (warnings are errors)
cargo fmt --all                           # format

./scripts/build.sh    # build release binary + package the .alfredworkflow
./scripts/deploy.sh   # build + update the installed workflow in place (imports it the first time)
./scripts/deploy.sh --import  # build + open the .alfredworkflow to import it
./scripts/icon.sh     # regenerate workflow/icon.png from scripts/icon.svg
```

The workflow icon (`workflow/icon.png`) is a committed asset rendered from
`scripts/icon.svg`. It is not regenerated at build time, so CI needs no image
tooling.

`scripts/deploy.sh` finds the installed workflow in Alfred's preferences
(`current` in `~/Library/Application Support/Alfred/prefs.json`, or
`$ALFRED_PREFERENCES`) by its bundle id and copies the new binary,
`info.plist` and icon over it; Alfred reloads the folder, and the user
configuration (`prefs.plist`) is kept. When the workflow is not installed yet,
it opens the `.alfredworkflow` for import instead. It does not symlink
(`powerpack link`): Alfred 6 turns the link into a stale copy, and re-importing
over an existing install fails.

`scripts/build.sh` syncs `workflow/info.plist`'s `<version>` from `Cargo.toml`
before packaging, so the version shown in Alfred always matches the crate.

## Manual smoke test (without Alfred)

The binary speaks Alfred's JSON on stdout, so you can exercise it directly:

```sh
cargo build
B=./target/debug/alfred-gh
export alfred_workflow_cache=$(mktemp -d) GH_OWNERS=my-org,my-user

$B repos "" | python3 -m json.tool        # gho (first run fetches, next ones hit the cache)
$B repos "m:api" | python3 -m json.tool   # gho, one owner
$B prs "api" | python3 -m json.tool       # ghp: candidates, or the PRs if unique
$B prs "my-org/api state:merged fix" | python3 -m json.tool
$B workflows "my-org/api " | python3 -m json.tool          # gha
$B workflows "my-org/api runs:12345" | python3 -m json.tool  # gha, one workflow's runs
$B review "" | python3 -m json.tool       # ghr
$B mine "" | python3 -m json.tool         # ghm

# Actions take the `arg` of an item (see docs/architecture.md):
$B run "copy https://github.com"
$B run "clone-open my-org/api Zed"
$B run "reopen ghp my-org/api "
JIRA_URL=https://example.atlassian.net/browse/ $B run "jira ABC-123"
$B run "refresh all"
```

To see the stale path, set `fetched_at` to `1` in a cache file: the next run
prints `"rerun": 0.5`, a `.lock` appears while the detached refresh runs, then
the entry is rewritten and the lock removed.

## Testing strategy

All logic is pure and unit-tested; every process spawn — `gh`, `osascript`,
`open` — goes through the `CommandRunner` trait and is replaced in tests by
`FakeRunner` (`tests/common/mod.rs`), which returns queued canned outputs per
program (successes, or failures with stdout and stderr), records every call and
every detached spawn. The whole suite runs offline. Canned `gh` responses live
in `tests/fixtures/`: real responses with every name rewritten to the
fictitious owners `acme-org` (organization) and `octo-user` (user).

| Test file               | Covers |
| ----------------------- | ------ |
| `tests/runner_test.rs`  | `SystemRunner` against harmless system programs: stdout, `CommandError` keeping stdout/stderr/exit code, detached spawns returning at once. |
| `tests/gh_test.rs`      | `gh` path resolution, exact GraphQL/REST argv (variables as raw `-f` fields), classification of every error (missing, not authenticated, SAML, rate limit with reset lookup, other). |
| `tests/owners_test.rs`  | `GH_OWNERS` parsing, owner kinds, search qualifiers, owner prefixes and collisions. |
| `tests/cache_test.rs`   | Fresh / stale / missing / corrupted entries with an injected clock, atomic writes, locks (exclusive, expiring), clearing, keys and TTLs. |
| `tests/repos_test.rs`   | Repository page parsing, pagination merge (cursor in the argv), the `gho` listing: per-owner cache entries, fresh/stale serving, background refresh spawn and lock, failed refresh back-off, one owner failing, SSO (error and empty organization), missing `gh`, not authenticated, rate limit, default owner. |
| `tests/prs_test.rs`     | Pull request parsing (checks, review state, deleted authors), one fixed query per state with its exact argv, the `ghp` listing: `owner/name` without listing repos, unique bare name, ambiguous/partial/empty names as candidates, unknown repository, filters, `state:`, stale refresh, errors and empty lists. |
| `tests/workflows_test.rs` | Workflow and run parsing, last run per workflow, run status and duration, branch percent-encoding, the exact REST paths, invalid repositories never reaching `gh`, the `gha` listing (cached default branch, status icons, ⏸, filters), `runs:<id>`, stale refresh, errors, cache keys. |
| `tests/search_test.rs`  | `ghr` / `ghm`: search query text, exact argv with the qualifiers built from owner kinds, results across repositories, filtering, a failing owner left out of the qualifiers, empty results, cache keys per qualifiers. |
| `tests/jira_test.rs`    | Ticket key extraction and title-over-branch priority. |
| `tests/ranking_test.rs` | Ranking tiers, push-date ties, archived last, empty query, repository resolution (exact / bare / ambiguous / partial / unknown), pull request filters. |
| `tests/query_test.rs`   | Query parsing of every keyword. |
| `tests/local_test.rs`   | Clone paths and `owner/repo` validation. |
| `tests/time_test.rs`    | ISO 8601 parsing, relative ages, durations. |
| `tests/action_test.rs`  | `arg` round-trip of every action, invalid args, execution (arguments passed to `open`, `osascript` and `gh`, never interpolated; clone then open; clearing the cache). |
| `tests/alfred_test.rs`  | JSON shape of every item and modifier, decodability of every modifier `arg`, error items, `rerun` only when asked. |
| `tests/plist_test.rs`   | `workflow/info.plist`: every keyword and the subcommand it runs, every modifier connected to the action, the user configuration variables and their generic defaults, the workflow readme (uses macOS `plutil`). |

## Project layout

```
.
├── src/                    Rust sources (see docs/architecture.md)
├── tests/                  Integration tests + shared FakeRunner
├── workflow/
│   ├── info.plist          Alfred workflow definition (the binary is added at build time)
│   └── icon.png            Workflow icon, rendered from scripts/icon.svg
├── scripts/
│   ├── build.sh            Build + package the .alfredworkflow
│   ├── deploy.sh           Build + install into Alfred
│   ├── icon.sh             Regenerate workflow/icon.png
│   └── icon.svg            Icon source
├── docs/                   This documentation
├── .github/workflows/      CI and release-please pipelines
├── release-please-config.json
└── .release-please-manifest.json
```

## Editing the Alfred graph

`workflow/info.plist` defines the Script Filters (one per keyword), each
connected — for every modifier used — to one Run Script, connected to a Post
Notification. If you edit the workflow in Alfred's UI, copy the updated
`info.plist` from the installed workflow folder back into `workflow/` before
committing (the next `./scripts/deploy.sh` overwrites the installed one).
Validate it with:

```sh
plutil -lint workflow/info.plist
```
