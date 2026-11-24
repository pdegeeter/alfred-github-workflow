# Alfred GitHub Workflow

An [Alfred](https://www.alfredapp.com/) workflow, written in Rust, to navigate
GitHub from Alfred: the repositories of the owners you care about, their pull
requests and Actions, your review queue and your own pull requests.

## Usage

### Repositories — `gho`

Lists the repositories of every owner in `GH_OWNERS` (organizations and
users). Exact name matches come first, then name prefixes, name substrings,
topics and descriptions; ties go to the most recently pushed. Archived
repositories come last and say so.

- `gho a:api` — only the owner starting with `a` (use the full login,
  `gho acme-org:api`, when two owners share a first letter)
- <kbd>↩</kbd> open on GitHub
- <kbd>⌘</kbd><kbd>↩</kbd> open the local clone
  (`~/Workspace/github.com/<owner>/<repo>`) in `EDITOR_APP`, cloning it with
  `gh repo clone` first if it is missing
- <kbd>⌥</kbd><kbd>↩</kbd> copy the URL
- the last item, **Refresh cache**, forgets everything cached

If an organization's repositories are missing because `gh` is not authorized
for its SAML SSO, an item says so and <kbd>↩</kbd> opens
[the authorization page](https://github.com/settings/connections/applications).

## How it works

- Every call goes through the `gh` CLI (`gh api graphql`, `gh api`): it
  handles authentication, token refresh and SSO.
- Results are cached under Alfred's workflow cache folder. A fresh cache is
  shown at once; a stale one too, while a background process refreshes it and
  Alfred reloads the list. Repositories are cached for 1 hour per owner.

## Configuration

Set these in Alfred → Workflows → GitHub → *Configure Workflow…*:

| Variable     | Default              | Meaning |
| ------------ | -------------------- | ------- |
| `GH_OWNERS`  | your `gh` login      | Comma-separated organizations and users whose repositories are listed. |
| `EDITOR_APP` | `Visual Studio Code` | Application that opens local clones. |
| `JIRA_URL`   | empty (disabled)     | Base URL that ticket keys found in pull requests are appended to. |

## Requirements

- macOS (Apple Silicon / arm64)
- [Alfred 5](https://www.alfredapp.com/) with the **Powerpack** license
  (workflows are a paid feature)
- The [GitHub CLI](https://cli.github.com/) (`brew install gh`), authenticated
  with `gh auth login`. The workflow never stores or logs a token: every call
  goes through `gh`, which handles authentication and SSO.

## Install

Download the latest `alfred-gh.alfredworkflow` from the
[Releases](https://github.com/pdegeeter/alfred-github-workflow/releases)
page and double-click it to import into Alfred.

Or build and deploy from source:

```sh
cargo install powerpack-cli   # one-time
./scripts/deploy.sh           # build + install (or update) in Alfred
```

## Documentation

- [Architecture](docs/architecture.md) — components, data flow, design
  decisions.
- [Development](docs/development.md) — build, test, lint, deploy locally.
- [Release process](docs/release.md) — release-please and the CI/CD pipeline.

## License

MIT
