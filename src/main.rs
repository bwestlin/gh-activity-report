use std::io::IsTerminal;
use std::process::ExitCode;

use anyhow::{Context, Result};
use chrono::NaiveDate;
use clap::{Parser, ValueEnum};
use gh_activity_report::activity::build_report;
use gh_activity_report::github::GitHubClient;
use gh_activity_report::output;
use gh_activity_report::timeframe::{Period, Timeframe};

#[derive(Debug, Parser)]
#[command(
    author,
    version,
    about = "Turn GitHub activity into a standup-ready report"
)]
struct Args {
    /// Timeframe preset. Defaults to yesterday.
    #[arg(value_enum, conflicts_with = "since")]
    period: Option<Period>,

    /// First local calendar date to include (YYYY-MM-DD).
    #[arg(long, value_parser = parse_date)]
    since: Option<NaiveDate>,

    /// Last local calendar date to include, inclusive (requires --since).
    #[arg(long, requires = "since", value_parser = parse_date)]
    until: Option<NaiveDate>,

    /// Output format. Defaults to console on a terminal and plain text when piped.
    #[arg(long, short, value_enum)]
    format: Option<Format>,

    /// GitHub personal access token.
    #[arg(long, env = "GITHUB_TOKEN", hide_env_values = true)]
    token: String,

    /// GitHub API base URL (useful for GitHub Enterprise Server).
    #[arg(long, env = "GITHUB_API_URL", default_value = "https://api.github.com")]
    api_url: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum Format {
    Console,
    Text,
    Markdown,
    Json,
    Html,
}

impl Format {
    fn automatic(stdout_is_terminal: bool) -> Self {
        if stdout_is_terminal {
            Self::Console
        } else {
            Self::Text
        }
    }
}

fn parse_date(value: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| format!("invalid date '{value}'; expected YYYY-MM-DD"))
}

#[tokio::main]
async fn main() -> ExitCode {
    match run(Args::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run(args: Args) -> Result<()> {
    let format = args
        .format
        .unwrap_or_else(|| Format::automatic(std::io::stdout().is_terminal()));
    let timeframe = Timeframe::resolve(args.period, args.since, args.until)?;
    let github = GitHubClient::new(&args.token, &args.api_url)?;
    let user = github
        .viewer_login()
        .await
        .context("could not identify the token owner")?;
    let result = github
        .events(&user, &timeframe)
        .await
        .with_context(|| format!("could not load activity for @{user}"))?;
    let report = build_report(
        user,
        timeframe.label.clone(),
        timeframe.display_dates(),
        result.events,
        result.may_be_truncated,
    );

    match format {
        Format::Console => print!("{}", output::text(&report, ansi_colors_available())),
        Format::Text => print!("{}", output::text(&report, false)),
        Format::Markdown => print!("{}", output::markdown(&report)),
        Format::Json => println!("{}", serde_json::to_string_pretty(&report)?),
        Format::Html => print!("{}", output::html(&report)),
    }
    Ok(())
}

fn ansi_colors_available() -> bool {
    std::io::stdout().is_terminal()
        && std::env::var_os("NO_COLOR").is_none()
        && std::env::var("TERM").map_or(true, |term| term != "dumb")
}

#[cfg(test)]
mod tests {
    use super::Format;

    #[test]
    fn automatic_format_uses_console_for_a_terminal() {
        assert_eq!(Format::automatic(true), Format::Console);
    }

    #[test]
    fn automatic_format_uses_plain_text_for_a_pipe() {
        assert_eq!(Format::automatic(false), Format::Text);
    }
}
