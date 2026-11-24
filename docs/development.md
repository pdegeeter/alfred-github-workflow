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

# Actions take the `arg` of an item (see docs/architecture.md):
$B run "copy https://github.com"
$B run "open https://github.com"
```

## Testing strategy

All logic is pure and unit-tested; every process spawn — `gh`, `osascript`,
`open` — goes through the `CommandRunner` trait and is replaced in tests by
`FakeRunner` (`tests/common/mod.rs`), which returns queued canned outputs per
program (successes, or failures with stdout and stderr), records every call and
every detached spawn. The whole suite runs offline.

| Test file               | Covers |
| ----------------------- | ------ |
| `tests/runner_test.rs`  | `SystemRunner` against harmless system programs: stdout, `CommandError` keeping stdout/stderr/exit code, detached spawns returning at once. |
| `tests/action_test.rs`  | `arg` round-trip of every action, invalid args, execution (arguments passed to `open` and `osascript`, never interpolated). |
| `tests/alfred_test.rs`  | JSON shape of items, `rerun` only when asked. |
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
