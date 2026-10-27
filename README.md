# gh-activity-report

A small Rust CLI that turns a GitHub user's recent activity into a report that
is easy to scan before a daily standup. The report is organized around pull
requests and the issues they move forward—not around repositories. Pull
requests are ranked by their activity, then each one is shown in this order:

1. commits
2. reviews
3. comments
4. pull request progress
5. related issue progress
6. other GitHub activity

PR state, ownership, branches, and related issues are included. Commits pushed
to a PR branch and merge commits that reference a known PR are grouped under
that PR. Issues from GitHub's closing-issue relationship are labeled `Closes`
and linked with their current state; description references are retained as
`Related`. Standalone issues and work that cannot be linked to a PR appear in
smaller fallback sections. The report subject is always inferred from
`GITHUB_TOKEN`, including private activity the token is allowed to read.

## Install

```shell
cargo install --path .
```

`GITHUB_TOKEN` is required. A fine-grained token needs read access to the
activity and private repositories you want included.

```shell
export GITHUB_TOKEN=github_pat_...
```

## Usage

Yesterday for the token owner (the default and only report subject):

```shell
gh-activity-report
```

Other useful periods:

```shell
gh-activity-report today
gh-activity-report this-week
gh-activity-report last-week
gh-activity-report last-7-days
```

Use an inclusive custom date range:

```shell
gh-activity-report --since 2026-10-01 --until 2026-10-15
```

Output defaults to colored console text when run interactively and plain text
when piped or redirected:

```shell
gh-activity-report last-week
gh-activity-report last-week | less
gh-activity-report last-week > standup.txt
```

You can also select the format explicitly:

```shell
gh-activity-report last-week --format console
gh-activity-report last-week --format text
gh-activity-report last-week --format markdown
gh-activity-report last-week --format json
gh-activity-report last-week --format html > report.html
```

`text` never emits ANSI sequences. `console` uses ANSI styling when stdout is
an interactive terminal, except when `NO_COLOR` is set or `TERM=dumb`. HTML
output includes its own responsive light/dark styling and print rules; it does
not require external assets.

Run `gh-activity-report --help` for all options. Calendar ranges and displayed
times use the machine's local timezone. `last-week` means the previous Monday
through Sunday; `last-7-days` means the seven complete days before today.

## Data source and limitations

The tool reads [GitHub's Events API](https://docs.github.com/en/rest/activity/events).
That is a good fit for recent standup recall because it includes pushes,
reviews, comments, pull requests, issues, and other actions in one feed.
GitHub notes that events can be delayed and the API only exposes a limited
recent history. The tool warns when it reaches its pagination limit, so use it
for recent reports rather than long-term auditing.

## Development

```shell
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

## License

Licensed under the [MIT License](LICENSE).
