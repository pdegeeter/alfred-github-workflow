# Alfred GitHub Workflow

An [Alfred](https://www.alfredapp.com/) workflow, written in Rust, to navigate
GitHub from Alfred: the repositories of the owners you care about, their pull
requests and Actions, your review queue, your own pull requests and a public
repository search.

| Keyword | What it lists |
| ------- | ------------- |
| `gh`    | Public GitHub repositories (live search) |
| `gho`   | Repositories of your owners (`GH_OWNERS`) |
| `ghp`   | Pull requests of a repository |
| `gha`   | Actions workflows and runs of a repository |
| `ghr`   | Pull requests awaiting your review |
| `ghm`   | Your open pull requests |

## Usage

### Public search — `gh`

`gh <query>` searches every public repository on GitHub, live (any GitHub
search qualifier works: `gh alfred language:rust stars:>50`). Results use the
same modifiers as `gho`.

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
- <kbd>⌃</kbd><kbd>↩</kbd> its pull requests (reopens Alfred on `ghp owner/repo `)
- <kbd>⇧</kbd><kbd>⌘</kbd><kbd>↩</kbd> its Actions workflows (reopens Alfred on `gha owner/repo `)
- the last item, **Refresh cache**, forgets everything cached

If an organization's repositories are missing because `gh` is not authorized
for its SAML SSO, an item says so and <kbd>↩</kbd> opens
[the authorization page](https://github.com/settings/connections/applications).

### Pull requests — `ghp`

`ghp <repo> [query]` lists the open pull requests of a repository, most
recently updated first. `<repo>` is `owner/name`, or a bare name when only one
of your owners has it; otherwise matching repositories are listed and
<kbd>↩</kbd> completes the query. The query filters on title, author, head
branch and number (`#42`); `state:merged` or `state:all` switch state.

Each item shows `#number · author · head→base · draft · CI ✅❌🟡 · review state · updated …`.

- <kbd>↩</kbd> open the pull request
- <kbd>⌘</kbd><kbd>↩</kbd> open “Files changed”
- <kbd>⌥</kbd><kbd>↩</kbd> copy the URL
- <kbd>⌃</kbd><kbd>↩</kbd> open the Jira ticket whose key (`ABC-123`) is in the
  title or the head branch, under `JIRA_URL` (a notification says when there
  is none, or when `JIRA_URL` is not set)

### Actions — `gha`

`gha <repo> [query]` lists a repository's workflows with the status of their
last run on the default branch: ✅ success, ❌ failure, 🟡 in progress,
⏸ disabled (⚪ cancelled or skipped, “no recent run” when none of the latest
100 runs is theirs). The query filters by name or path. `<repo>` resolves as
for `ghp`.

- <kbd>↩</kbd> open the workflow's runs page
- <kbd>⌘</kbd><kbd>↩</kbd> open the last run
- <kbd>⌥</kbd><kbd>↩</kbd> copy the runs page URL
- <kbd>⌃</kbd><kbd>↩</kbd> list its runs (reopens Alfred on `gha owner/repo runs:<id>`)

`gha <repo> runs:<workflow id> [query]` lists the last 10 runs of one workflow
(branch, event, actor, duration, status); the query filters by title or
branch. <kbd>↩</kbd> opens a run, <kbd>⌥</kbd><kbd>↩</kbd> copies its URL.

### Review queue and my pull requests — `ghr`, `ghm`

- `ghr [query]` — open pull requests where your review is requested
- `ghm [query]` — your open pull requests

Both search every owner of `GH_OWNERS` (`org:` or `user:` qualifiers, picked
from what GitHub says each owner is), show the repository in the title and
use the same modifiers as `ghp`. The query filters on title, author, branch
and number.

## How it works

- Every call goes through the `gh` CLI (`gh api graphql`, `gh api`): it
  handles authentication, token refresh and SSO.
- Results are cached under Alfred's workflow cache folder. A fresh cache is
  shown at once; a stale one too, while a background process refreshes it and
  Alfred reloads the list. Repositories are cached for 1 hour per owner, pull
  requests (and the `ghr` / `ghm` lists) for 1 minute, workflows for 10 minutes
  and runs for 30 seconds. The public search (`gh`) is never cached.

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
