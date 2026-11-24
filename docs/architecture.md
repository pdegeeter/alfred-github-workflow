# Architecture

## Overview

The workflow is a single Rust binary, `alfred-gh`, with one subcommand per
object in the Alfred workflow graph:

| Subcommand             | Alfred object                | Keyword | Responsibility |
| ---------------------- | ---------------------------- | ------- | -------------- |
| `alfred-gh run <arg>`  | Run Script (`action.script`) | —       | Decode the selected item's `arg` and perform it. |

Every script filter is connected to the single Run Script, itself connected to
a Post Notification that shows whatever `run` prints (copies, clones, errors).

## The `arg` protocol

Script filter items do not carry raw data: their `arg` — and the `arg` of each
modifier (`mods`) — is an encoded [`Action`](../src/action.rs):
`"<verb> <payload>"`. `run` decodes it with `Action::parse` and performs it
with `action::execute`.

| Verb   | Payload | Effect |
| ------ | ------- | ------ |
| `open` | `<url>` | Open the URL with macOS `open`. |
| `copy` | `<url>` | Copy the URL. |

Fields are ordered so that only the last one may contain spaces; URLs never
do. A round-trip test covers every verb.

Each script filter declares connections for every modifier used
(<kbd>⌘</kbd>, <kbd>⌥</kbd>, <kbd>⌃</kbd>, <kbd>⇧⌘</kbd>), all to the same Run
Script, so a modifier's `arg` always reaches `run`.

## Design decisions

- **Everything is a pure function except one trait.** The only side-effecting
  boundary — spawning `gh`, `osascript` and `open` — is
  `runner::CommandRunner` (`SystemRunner` in prod, `FakeRunner` in tests).
- **Failures keep stdout.** `gh api graphql` prints GraphQL errors (`FORBIDDEN`,
  SAML enforcement, partial data) as JSON on stdout and exits 1. A failed
  `run` therefore returns a `CommandError { status, stdout, stderr }` rather
  than a plain message.
- **Background work is detached.** `CommandRunner::spawn_detached` starts a
  process with all three streams closed and returns at once: Alfred waits for
  a script filter's stdout to close, so an inherited stdout would block it.
- **No `uid` on items**: Alfred would re-sort by its own usage knowledge.
  Filtering and ordering happen in Rust (`alfredfiltersresults = false`).
- **Script filters never hard-fail**: errors become an invalid Alfred item.
- **Generic defaults**: owners and the Jira base URL come from Alfred's user
  configuration, never from code.
