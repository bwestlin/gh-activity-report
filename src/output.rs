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
        "\n**{}** ({}) — {} pull requests, {}, and {} issues moved forward ({} activity signals)",
        capitalize(&report.period),
        report.dates,
        report.pull_requests.len(),
        branch_count(report.branches.len()),
        issue_count,
        report.total
    )
    .unwrap();

    if report.pull_requests.is_empty()
        && report.branches.is_empty()
        && report.issues.is_empty()
        && report.other_activity.is_empty()
    {
        writeln!(out, "\nNo GitHub activity found in this timeframe.").unwrap();
    } else {
        write_pull_requests(&mut out, report);
        write_branches(&mut out, report);
        write_standalone_issues(&mut out, report);
        write_other_activity(&mut out, report);
    }

    if report.may_be_truncated {
        writeln!(out, "\n> Warning: GitHub's Events API page limit was reached; this report may be incomplete.").unwrap();
    }
    out
}

fn write_branches(out: &mut String, report: &Report) {
    if report.branches.is_empty() {
        return;
    }
    writeln!(out, "\n## Branches without pull requests\n").unwrap();
    for branch in &report.branches {
        writeln!(
            out,
            "### [`{}:{}`]({}) — {}\n",
            branch.repository,
            escape_link_text(&branch.branch),
            branch.html_url,
            commit_count(branch.total)
        )
        .unwrap();
        for activity in &branch.activities {
            write_branch_activity(out, activity);
        }
        out.push('\n');
    }
}

fn write_branch_activity(out: &mut String, activity: &Activity) {
    let summary = match &activity.url {
        Some(url) => format!("[{}]({url})", escape_link_text(&activity.summary)),
        None => activity.summary.clone(),
    };
    writeln!(out, "- {summary} _({})_", local_time(activity.occurred_at)).unwrap();
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

pub fn text(report: &Report, colors: bool) -> String {
    let style = Ansi { enabled: colors };
    let mut out = String::new();
    let issue_count = related_issue_count(report) + report.issues.len();
    writeln!(
        out,
        "{}",
        style.heading(&format!("GitHub progress for @{}", report.user))
    )
    .unwrap();
    writeln!(
        out,
        "{} ({}) · {} pull requests · {} · {} issues · {} activity signals",
        style.bold(&capitalize(&report.period)),
        report.dates,
        report.pull_requests.len(),
        branch_count(report.branches.len()),
        issue_count,
        report.total
    )
    .unwrap();

    if report.pull_requests.is_empty()
        && report.branches.is_empty()
        && report.issues.is_empty()
        && report.other_activity.is_empty()
    {
        writeln!(out, "\nNo GitHub activity found in this timeframe.").unwrap();
        return out;
    }

    if !report.pull_requests.is_empty() {
        writeln!(out, "\n{}", style.section("PULL REQUESTS MOVED FORWARD")).unwrap();
        for (index, pull) in report.pull_requests.iter().enumerate() {
            writeln!(
                out,
                "{}. {} {} — {}",
                index + 1,
                style.link(&format!("{}#{}", pull.repository, pull.number)),
                pull.title,
                style.status(&pull.status)
            )
            .unwrap();
            writeln!(out, "   {}", style.dim(&count_summary(pull))).unwrap();
        }

        for (index, pull) in report.pull_requests.iter().enumerate() {
            writeln!(
                out,
                "\n{}",
                style.heading(&format!("{}. {}", index + 1, pull.title))
            )
            .unwrap();
            write!(
                out,
                "{} · {} · {}",
                style.link(&format!("{}#{}", pull.repository, pull.number)),
                style.status(&pull.status),
                if pull.relationship == "authored" {
                    "authored by you"
                } else {
                    "contributed to"
                }
            )
            .unwrap();
            if let (Some(head), Some(base)) = (&pull.head, &pull.base) {
                write!(out, " · {head} -> {base}").unwrap();
            }
            writeln!(out, "\nProgress: {}", count_summary(pull)).unwrap();
            writeln!(out, "{}", style.dim(&pull.html_url)).unwrap();

            if !pull.related_issues.is_empty() {
                writeln!(out, "\n{}", style.subheading("Related issues")).unwrap();
                for issue in &pull.related_issues {
                    let relationship = if issue.relationship == "closes" {
                        "Closes"
                    } else {
                        "Related"
                    };
                    writeln!(
                        out,
                        "  - {}: {}#{} — {} — {}",
                        style.bold(relationship),
                        issue.repository,
                        issue.number,
                        issue.title,
                        style.status(&issue.state)
                    )
                    .unwrap();
                    writeln!(out, "    {}", style.dim(&issue.html_url)).unwrap();
                    for activity in &issue.activities {
                        write_text_activity(&mut out, activity, false, &style, 4);
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
                writeln!(
                    out,
                    "\n{}",
                    style.subheading(&format!("{} ({count})", category.label()))
                )
                .unwrap();
                for activity in activities {
                    write_text_activity(&mut out, activity, false, &style, 2);
                }
            }
        }
    }

    if !report.branches.is_empty() {
        writeln!(out, "\n{}", style.section("BRANCHES WITHOUT PULL REQUESTS")).unwrap();
        for branch in &report.branches {
            writeln!(
                out,
                "\n{} — {}",
                style.link(&format!("{}:{}", branch.repository, branch.branch)),
                commit_count(branch.total)
            )
            .unwrap();
            writeln!(out, "{}", style.dim(&branch.html_url)).unwrap();
            for activity in &branch.activities {
                write_text_branch_activity(&mut out, activity, &style, 2);
            }
        }
    }

    if !report.issues.is_empty() {
        writeln!(
            out,
            "\n{}",
            style.section("ISSUES MOVED FORWARD OUTSIDE A PULL REQUEST")
        )
        .unwrap();
        for issue in &report.issues {
            writeln!(
                out,
                "\n{}#{} — {} — {}",
                issue.repository,
                issue.number,
                issue.title,
                style.status(&issue.state)
            )
            .unwrap();
            writeln!(out, "{}", style.dim(&issue.html_url)).unwrap();
            for activity in &issue.activities {
                write_text_activity(&mut out, activity, false, &style, 2);
            }
        }
    }

    if !report.other_activity.is_empty() {
        writeln!(
            out,
            "\n{}",
            style.section("OTHER ACTIVITY NOT LINKED TO A PULL REQUEST")
        )
        .unwrap();
        for activity in &report.other_activity {
            write_text_activity(&mut out, activity, true, &style, 0);
        }
    }

    if report.may_be_truncated {
        writeln!(
            out,
            "\n{}",
            style.warning("Warning: GitHub's Events API page limit was reached; this report may be incomplete.")
        )
        .unwrap();
    }
    out
}

fn write_text_branch_activity(out: &mut String, activity: &Activity, style: &Ansi, indent: usize) {
    let padding = " ".repeat(indent);
    writeln!(
        out,
        "{padding}- {} {}",
        activity.summary,
        style.dim(&format!("({})", local_time(activity.occurred_at)))
    )
    .unwrap();
    if let Some(url) = &activity.url {
        writeln!(out, "{padding}  {}", style.dim(url)).unwrap();
    }
}

fn write_text_activity(
    out: &mut String,
    activity: &Activity,
    show_repository: bool,
    style: &Ansi,
    indent: usize,
) {
    let padding = " ".repeat(indent);
    let repository = if show_repository {
        format!("{} — ", style.bold(&activity.repository))
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
        "{padding}- {repository}{}{detail} {}",
        activity.summary,
        style.dim(&format!("({})", local_time(activity.occurred_at)))
    )
    .unwrap();
    if let Some(url) = &activity.url {
        writeln!(out, "{padding}  {}", style.dim(url)).unwrap();
    }
}

struct Ansi {
    enabled: bool,
}

impl Ansi {
    fn paint(&self, code: &str, value: &str) -> String {
        if self.enabled {
            format!("\x1b[{code}m{value}\x1b[0m")
        } else {
            value.to_owned()
        }
    }

    fn bold(&self, value: &str) -> String {
        self.paint("1", value)
    }

    fn heading(&self, value: &str) -> String {
        self.paint("1;36", value)
    }

    fn section(&self, value: &str) -> String {
        self.paint("1;35", value)
    }

    fn subheading(&self, value: &str) -> String {
        self.paint("1;34", value)
    }

    fn link(&self, value: &str) -> String {
        self.paint("4;34", value)
    }

    fn dim(&self, value: &str) -> String {
        self.paint("2", value)
    }

    fn status(&self, value: &str) -> String {
        let value = capitalize(value);
        match value.to_ascii_lowercase().as_str() {
            "merged" | "closed" => self.paint("1;32", &value),
            "draft" => self.paint("1;33", &value),
            _ => self.paint("1;36", &value),
        }
    }

    fn warning(&self, value: &str) -> String {
        self.paint("1;33", value)
    }
}

pub fn html(report: &Report) -> String {
    let mut out = String::new();
    let issue_count = related_issue_count(report) + report.issues.len();
    write!(
        out,
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\n<title>GitHub progress for @{}</title>\n<style>{}</style>\n</head>\n<body>\n<main>\n",
        escape_html(&report.user),
        HTML_CSS
    )
    .unwrap();
    writeln!(
        out,
        "<header><h1>GitHub progress for <span>@{}</span></h1><p><strong>{}</strong> ({}) · {} pull requests · {} · {} issues · {} activity signals</p></header>",
        escape_html(&report.user),
        escape_html(&capitalize(&report.period)),
        escape_html(&report.dates),
        report.pull_requests.len(),
        branch_count(report.branches.len()),
        issue_count,
        report.total
    )
    .unwrap();

    if report.pull_requests.is_empty()
        && report.branches.is_empty()
        && report.issues.is_empty()
        && report.other_activity.is_empty()
    {
        out.push_str("<p class=\"empty\">No GitHub activity found in this timeframe.</p>\n");
    } else {
        write_html_pull_requests(&mut out, report);
        write_html_branches(&mut out, report);
        write_html_issues(&mut out, report);
        write_html_other(&mut out, report);
    }
    if report.may_be_truncated {
        out.push_str("<aside class=\"warning\">GitHub's Events API page limit was reached; this report may be incomplete.</aside>\n");
    }
    out.push_str("</main>\n</body>\n</html>\n");
    out
}

fn write_html_branches(out: &mut String, report: &Report) {
    if report.branches.is_empty() {
        return;
    }
    out.push_str("<section><h2>Branches without pull requests</h2>\n");
    for branch in &report.branches {
        writeln!(
            out,
            "<article class=\"branch\"><h3><a href=\"{}\"><code>{}:{}</code></a></h3><p class=\"meta\">{}</p><ul class=\"activity\">",
            escape_html(&branch.html_url),
            escape_html(&branch.repository),
            escape_html(&branch.branch),
            commit_count(branch.total)
        )
        .unwrap();
        for activity in &branch.activities {
            write_html_branch_activity(out, activity);
        }
        out.push_str("</ul></article>\n");
    }
    out.push_str("</section>\n");
}

fn write_html_pull_requests(out: &mut String, report: &Report) {
    if report.pull_requests.is_empty() {
        return;
    }
    out.push_str("<section><h2>Pull requests moved forward</h2><ol class=\"summary-list\">\n");
    for pull in &report.pull_requests {
        writeln!(
            out,
            "<li><a href=\"{}\"><code>{}#{}</code> — {}</a> <span class=\"status {}\">{}</span><small>{}</small></li>",
            escape_html(&pull.html_url),
            escape_html(&pull.repository),
            pull.number,
            escape_html(&pull.title),
            status_class(&pull.status),
            escape_html(&capitalize(&pull.status)),
            escape_html(&count_summary(pull))
        )
        .unwrap();
    }
    out.push_str("</ol></section>\n");

    for (index, pull) in report.pull_requests.iter().enumerate() {
        writeln!(
            out,
            "<article class=\"pull-request\"><h2><span>{}.</span> <a href=\"{}\">{}</a></h2>",
            index + 1,
            escape_html(&pull.html_url),
            escape_html(&pull.title)
        )
        .unwrap();
        write!(
            out,
            "<p class=\"meta\"><code>{}#{}</code> · <span class=\"status {}\">{}</span> · {}",
            escape_html(&pull.repository),
            pull.number,
            status_class(&pull.status),
            escape_html(&capitalize(&pull.status)),
            if pull.relationship == "authored" {
                "Authored by you"
            } else {
                "Contributed to"
            }
        )
        .unwrap();
        if let (Some(head), Some(base)) = (&pull.head, &pull.base) {
            write!(
                out,
                " · <code>{}</code> → <code>{}</code>",
                escape_html(head),
                escape_html(base)
            )
            .unwrap();
        }
        writeln!(
            out,
            "</p><p><strong>Progress:</strong> {}</p>",
            escape_html(&count_summary(pull))
        )
        .unwrap();

        if !pull.related_issues.is_empty() {
            out.push_str("<h3>Related issues</h3><ul class=\"issues\">\n");
            for issue in &pull.related_issues {
                writeln!(
                    out,
                    "<li><strong>{}:</strong> <a href=\"{}\"><code>{}#{}</code> — {}</a> <span class=\"status {}\">{}</span>",
                    if issue.relationship == "closes" { "Closes" } else { "Related" },
                    escape_html(&issue.html_url),
                    escape_html(&issue.repository),
                    issue.number,
                    escape_html(&issue.title),
                    status_class(&issue.state),
                    escape_html(&capitalize(&issue.state))
                )
                .unwrap();
                if !issue.activities.is_empty() {
                    out.push_str("<ul>");
                    for activity in &issue.activities {
                        write_html_activity(out, activity, false);
                    }
                    out.push_str("</ul>");
                }
                out.push_str("</li>\n");
            }
            out.push_str("</ul>\n");
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
            writeln!(
                out,
                "<h3>{} ({count})</h3><ul class=\"activity\">",
                category.label()
            )
            .unwrap();
            for activity in activities {
                write_html_activity(out, activity, false);
            }
            out.push_str("</ul>\n");
        }
        out.push_str("</article>\n");
    }
}

fn write_html_issues(out: &mut String, report: &Report) {
    if report.issues.is_empty() {
        return;
    }
    out.push_str("<section><h2>Issues moved forward outside a pull request</h2>\n");
    for issue in &report.issues {
        writeln!(
            out,
            "<article class=\"issue\"><h3><a href=\"{}\"><code>{}#{}</code> — {}</a> <span class=\"status {}\">{}</span></h3><ul class=\"activity\">",
            escape_html(&issue.html_url),
            escape_html(&issue.repository),
            issue.number,
            escape_html(&issue.title),
            status_class(&issue.state),
            escape_html(&capitalize(&issue.state))
        )
        .unwrap();
        for activity in &issue.activities {
            write_html_activity(out, activity, false);
        }
        out.push_str("</ul></article>\n");
    }
    out.push_str("</section>\n");
}

fn write_html_other(out: &mut String, report: &Report) {
    if report.other_activity.is_empty() {
        return;
    }
    out.push_str(
        "<section><h2>Other activity not linked to a pull request</h2><ul class=\"activity\">\n",
    );
    for activity in &report.other_activity {
        write_html_activity(out, activity, true);
    }
    out.push_str("</ul></section>\n");
}

fn write_html_activity(out: &mut String, activity: &Activity, show_repository: bool) {
    out.push_str("<li>");
    if show_repository {
        write!(
            out,
            "<strong>{}</strong> — ",
            escape_html(&activity.repository)
        )
        .unwrap();
    }
    if let Some(url) = &activity.url {
        write!(
            out,
            "<a href=\"{}\">{}</a>",
            escape_html(url),
            escape_html(&activity.summary)
        )
        .unwrap();
    } else {
        out.push_str(&escape_html(&activity.summary));
    }
    if let Some(detail) = activity.detail.as_ref().filter(|detail| !detail.is_empty()) {
        write!(out, " — {}", escape_html(&detail.replace('\n', " "))).unwrap();
    }
    writeln!(
        out,
        " <time>{}</time></li>",
        escape_html(&local_time(activity.occurred_at))
    )
    .unwrap();
}

fn write_html_branch_activity(out: &mut String, activity: &Activity) {
    out.push_str("<li>");
    if let Some(url) = &activity.url {
        write!(
            out,
            "<a href=\"{}\">{}</a>",
            escape_html(url),
            escape_html(&activity.summary)
        )
        .unwrap();
    } else {
        out.push_str(&escape_html(&activity.summary));
    }
    writeln!(
        out,
        " <time>{}</time></li>",
        escape_html(&local_time(activity.occurred_at))
    )
    .unwrap();
}

fn related_issue_count(report: &Report) -> usize {
    report
        .pull_requests
        .iter()
        .flat_map(|pull| {
            pull.related_issues
                .iter()
                .map(|issue| (issue.repository.as_str(), issue.number))
        })
        .collect::<BTreeSet<_>>()
        .len()
}

fn status_class(status: &str) -> &'static str {
    match status.to_ascii_lowercase().as_str() {
        "merged" | "closed" => "done",
        "draft" => "draft",
        _ => "open",
    }
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

const HTML_CSS: &str = r#"
:root { color-scheme: light dark; --bg:#f6f8fa; --card:#fff; --text:#1f2328; --muted:#636c76; --border:#d0d7de; --link:#0969da; --accent:#8250df; --done:#1a7f37; --open:#0969da; --draft:#9a6700; }
@media (prefers-color-scheme:dark) { :root { --bg:#0d1117; --card:#161b22; --text:#e6edf3; --muted:#8b949e; --border:#30363d; --link:#58a6ff; --accent:#a371f7; --done:#3fb950; --open:#58a6ff; --draft:#d29922; } }
* { box-sizing:border-box; } body { margin:0; background:var(--bg); color:var(--text); font:15px/1.55 system-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif; }
main { max-width:960px; margin:0 auto; padding:40px 24px 72px; } header { margin-bottom:32px; } h1 { margin:0; font-size:2rem; } h1 span { color:var(--accent); } h2 { margin-top:32px; } h3 { margin:24px 0 8px; }
a { color:var(--link); text-decoration:none; } a:hover { text-decoration:underline; } code { padding:.12em .35em; border-radius:4px; background:color-mix(in srgb,var(--muted) 14%,transparent); }
article.pull-request, article.issue, article.branch, section { margin:20px 0; padding:20px 24px; border:1px solid var(--border); border-radius:10px; background:var(--card); } article.pull-request>h2 { margin-top:0; } .meta, small, time { color:var(--muted); }
.summary-list li { margin:.65rem 0; } .summary-list small { display:block; margin-left:1rem; } .status { display:inline-block; margin-left:.35rem; padding:.08rem .45rem; border:1px solid currentColor; border-radius:999px; font-size:.78rem; font-weight:700; text-transform:uppercase; } .status.done { color:var(--done); } .status.open { color:var(--open); } .status.draft { color:var(--draft); }
ul.activity, ul.issues { padding-left:1.3rem; } ul.activity li, ul.issues li { margin:.45rem 0; } .warning { margin-top:24px; padding:14px 18px; border-left:4px solid var(--draft); background:var(--card); } .empty { padding:24px; border:1px dashed var(--border); border-radius:10px; }
@media print { :root { --bg:#fff; --card:#fff; --text:#000; } main { max-width:none; padding:0; } article.pull-request, article.issue, article.branch, section { break-inside:avoid; } }
"#;

fn branch_count(count: usize) -> String {
    if count == 1 {
        "1 branch without PR".to_owned()
    } else {
        format!("{count} branches without PRs")
    }
}

fn commit_count(count: usize) -> String {
    format!("{count} {}", count_label(Category::Commits, count))
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
    use crate::github::{Actor, GitHubEvent, Repository};
    use chrono::{TimeZone, Utc};
    use serde_json::json;

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
        assert!(
            output.contains("0 pull requests, 0 branches without PRs, and 0 issues moved forward")
        );
    }

    #[test]
    fn branches_without_pull_requests_appear_in_every_format() {
        let event = GitHubEvent {
            id: "push-1".to_owned(),
            kind: "PushEvent".to_owned(),
            actor: Actor {
                login: "octocat".to_owned(),
            },
            repo: Repository {
                name: "acme/web".to_owned(),
            },
            payload: json!({
                "ref": "refs/heads/feature/header",
                "commits": [{"sha": "1234567890", "message": "Fix header"}]
            }),
            public: true,
            created_at: Utc.with_ymd_and_hms(2026, 10, 26, 8, 0, 0).unwrap(),
        };
        let report = build_report(
            "octocat".into(),
            "yesterday".into(),
            "2026-10-26".into(),
            vec![event],
            false,
        );

        let markdown = markdown(&report);
        assert!(markdown.contains("## Branches without pull requests"));
        assert!(markdown.contains("acme/web:feature/header"));

        let text = text(&report, false);
        assert!(text.contains("BRANCHES WITHOUT PULL REQUESTS"));
        assert!(text.contains("acme/web:feature/header — 1 commit"));

        let html = html(&report);
        assert!(html.contains("Branches without pull requests"));
        assert!(html.contains("acme/web:feature/header"));

        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"branches\":[{"));
        assert!(json.contains("\"branch\":\"feature/header\""));
    }

    #[test]
    fn html_is_a_standalone_document_and_escapes_user_content() {
        let report = build_report(
            "octo<&cat".into(),
            "yesterday".into(),
            "2026-10-26".into(),
            vec![],
            false,
        );
        let output = html(&report);
        assert!(output.starts_with("<!doctype html>"));
        assert!(output.contains("@octo&lt;&amp;cat"));
        assert!(!output.contains("@octo<&cat"));
    }

    #[test]
    fn text_only_emits_ansi_when_enabled() {
        let report = build_report(
            "octocat".into(),
            "yesterday".into(),
            "2026-10-26".into(),
            vec![],
            false,
        );
        assert!(!text(&report, false).contains("\x1b["));
        assert!(text(&report, true).contains("\x1b["));
    }
}
