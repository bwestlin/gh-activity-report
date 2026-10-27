use std::collections::BTreeSet;
use std::fmt::Write;

use crate::activity::{Activity, Category, PullRequestWork, Report, local_time};

pub fn markdown(report: &Report) -> String {
    let mut out = String::new();
    let related_issue_count = report
        .pull_requests
        .iter()
        .flat_map(|pull| {
            pull.related_issues
                .iter()
                .map(|issue| (issue.repository.as_str(), issue.number))
        })
        .collect::<BTreeSet<_>>()
        .len();
    let issue_count = related_issue_count + report.issues.len();

    writeln!(out, "# GitHub progress for @{}", report.user).unwrap();
    writeln!(
        out,
        "\n**{}** ({}) — {} pull requests and {} issues moved forward ({} activity signals)",
        capitalize(&report.period),
        report.dates,
        report.pull_requests.len(),
        issue_count,
        report.total
    )
    .unwrap();

    if report.pull_requests.is_empty()
        && report.issues.is_empty()
        && report.other_activity.is_empty()
    {
        writeln!(out, "\nNo GitHub activity found in this timeframe.").unwrap();
    } else {
        write_pull_requests(&mut out, report);
        write_standalone_issues(&mut out, report);
        write_other_activity(&mut out, report);
    }

    if report.may_be_truncated {
        writeln!(out, "\n> Warning: GitHub's Events API page limit was reached; this report may be incomplete.").unwrap();
    }
    out
}

fn write_pull_requests(out: &mut String, report: &Report) {
    if report.pull_requests.is_empty() {
        return;
    }
    writeln!(out, "\n## Pull requests moved forward\n").unwrap();
    for (index, pull) in report.pull_requests.iter().enumerate() {
        writeln!(
            out,
            "{}. [{}#{} — {}]({}) — **{}** — {}",
            index + 1,
            pull.repository,
            pull.number,
            escape_link_text(&pull.title),
            pull.html_url,
            capitalize(&pull.status),
            count_summary(pull)
        )
        .unwrap();
    }

    for (index, pull) in report.pull_requests.iter().enumerate() {
        writeln!(
            out,
            "\n## {}. [{}]({})\n",
            index + 1,
            escape_link_text(&pull.title),
            pull.html_url
        )
        .unwrap();
        let relationship = if pull.relationship == "authored" {
            "Authored by you"
        } else {
            "Contributed to"
        };
        write!(
            out,
            "`{}#{}` · **{}** · {}",
            pull.repository,
            pull.number,
            capitalize(&pull.status),
            relationship
        )
        .unwrap();
        if let (Some(head), Some(base)) = (&pull.head, &pull.base) {
            write!(out, " · `{head}` → `{base}`").unwrap();
        }
        writeln!(out, "\n\n**Progress:** {}", count_summary(pull)).unwrap();

        if !pull.related_issues.is_empty() {
            writeln!(out, "\n### Related issues\n").unwrap();
            for issue in &pull.related_issues {
                writeln!(
                    out,
                    "- **{}:** [{}#{} — {}]({}) — **{}**",
                    if issue.relationship == "closes" {
                        "Closes"
                    } else {
                        "Related"
                    },
                    issue.repository,
                    issue.number,
                    escape_link_text(&issue.title),
                    issue.html_url,
                    capitalize(&issue.state)
                )
                .unwrap();
                for activity in &issue.activities {
                    writeln!(
                        out,
                        "  - {} _({})_",
                        activity.summary,
                        local_time(activity.occurred_at)
                    )
                    .unwrap();
                }
            }
        }

        for category in Category::ALL {
            if category == Category::Issues {
                continue;
            }
            let activities: Vec<_> = pull
                .activities
                .iter()
                .filter(|activity| activity.category == category)
                .collect();
            if activities.is_empty() {
                continue;
            }
            let count = pull.counts.get(&category).copied().unwrap_or(0);
            writeln!(out, "\n### {} ({count})\n", category.label()).unwrap();
            for activity in activities {
                write_activity(out, activity, false);
            }
        }
    }
}

fn write_standalone_issues(out: &mut String, report: &Report) {
    if report.issues.is_empty() {
        return;
    }
    writeln!(out, "\n## Issues moved forward outside a pull request\n").unwrap();
    for issue in &report.issues {
        writeln!(
            out,
            "### [{}#{} — {}]({}) — {}\n",
            issue.repository,
            issue.number,
            escape_link_text(&issue.title),
            issue.html_url,
            capitalize(&issue.state)
        )
        .unwrap();
        for activity in &issue.activities {
            write_activity(out, activity, false);
        }
        out.push('\n');
    }
}

fn write_other_activity(out: &mut String, report: &Report) {
    if report.other_activity.is_empty() {
        return;
    }
    writeln!(out, "\n## Other activity not linked to a pull request\n").unwrap();
    for activity in &report.other_activity {
        write_activity(out, activity, true);
    }
}

fn write_activity(out: &mut String, activity: &Activity, show_repository: bool) {
    let summary = match &activity.url {
        Some(url) => format!("[{}]({url})", escape_link_text(&activity.summary)),
        None => activity.summary.clone(),
    };
    let repository = if show_repository {
        format!("**{}** — ", activity.repository)
    } else {
        String::new()
    };
    let detail = activity
        .detail
        .as_ref()
        .filter(|detail| !detail.is_empty())
        .map(|detail| format!(" — {}", detail.replace('\n', " ")))
        .unwrap_or_default();
    writeln!(
        out,
        "- {repository}{summary}{detail} _({})_",
        local_time(activity.occurred_at)
    )
    .unwrap();
}

fn count_summary(pull: &PullRequestWork) -> String {
    let parts: Vec<_> = Category::ALL
        .iter()
        .filter_map(|category| {
            pull.counts
                .get(category)
                .copied()
                .filter(|count| *count > 0)
                .map(|count| format!("{count} {}", count_label(*category, count)))
        })
        .collect();
    parts.join(", ")
}

fn count_label(category: Category, count: usize) -> &'static str {
    match category {
        Category::Commits if count == 1 => "commit",
        Category::Commits => "commits",
        Category::Reviews if count == 1 => "review",
        Category::Reviews => "reviews",
        Category::Comments if count == 1 => "comment",
        Category::Comments => "comments",
        Category::PullRequests if count == 1 => "PR update",
        Category::PullRequests => "PR updates",
        Category::Issues if count == 1 => "issue update",
        Category::Issues => "issue updates",
        Category::Other => "other",
    }
}

fn escape_link_text(value: &str) -> String {
    value.replace('[', "\\[").replace(']', "\\]")
}

fn capitalize(value: &str) -> String {
    let mut chars = value.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::build_report;

    #[test]
    fn empty_report_is_clear_and_pull_request_focused() {
        let report = build_report(
            "octocat".into(),
            "yesterday".into(),
            "2026-10-26".into(),
            vec![],
            false,
        );
        let output = markdown(&report);
        assert!(output.contains("No GitHub activity found"));
        assert!(output.contains("0 pull requests and 0 issues moved forward"));
    }
}
