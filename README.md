# Alfred GitHub Workflow

An [Alfred](https://www.alfredapp.com/) workflow, written in Rust, to navigate
GitHub from Alfred: the repositories of the owners you care about, their pull
requests and Actions, your review queue and your own pull requests.

## Usage

Keywords are added one by one; none is available yet.

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
