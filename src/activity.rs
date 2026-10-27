use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, HashSet};

use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::github::GitHubEvent;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Commits,
    Reviews,
    Comments,
    PullRequests,
    Issues,
    Other,
}

impl Category {
    pub const ALL: [Category; 6] = [
        Category::Commits,
        Category::Reviews,
        Category::Comments,
        Category::PullRequests,
        Category::Issues,
        Category::Other,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Category::Commits => "Commits",
            Category::Reviews => "Reviews",
            Category::Comments => "Comments",
            Category::PullRequests => "Pull request progress",
            Category::Issues => "Related issue progress",
            Category::Other => "Other",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Activity {
    pub category: Category,
    pub repository: String,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub occurred_at: DateTime<Utc>,
    pub private: bool,
    /// A summarized push can represent more than one commit.
    pub count: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RelatedIssue {
    pub repository: String,
    pub number: u64,
    pub title: String,
    pub html_url: String,
    pub state: String,
    #[serde(default = "default_issue_relationship")]
    pub relationship: String,
    #[serde(default, skip_deserializing, skip_serializing_if = "Vec::is_empty")]
    pub activities: Vec<Activity>,
}

fn default_issue_relationship() -> String {
    "related".to_owned()
}

#[derive(Clone, Debug, Serialize)]
pub struct PullRequestWork {
    pub repository: String,
    pub number: u64,
    pub title: String,
    pub html_url: String,
    pub status: String,
    pub relationship: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    pub total: usize,
    pub counts: BTreeMap<Category, usize>,
    pub related_issues: Vec<RelatedIssue>,
    pub activities: Vec<Activity>,
    pub last_activity_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize)]
pub struct IssueWork {
    pub repository: String,
    pub number: u64,
    pub title: String,
    pub html_url: String,
    pub state: String,
    pub total: usize,
    pub activities: Vec<Activity>,
    pub last_activity_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize)]
pub struct BranchWork {
    pub repository: String,
    pub branch: String,
    pub html_url: String,
    pub total: usize,
    pub activities: Vec<Activity>,
    pub last_activity_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Report {
    pub user: String,
    pub period: String,
    pub dates: String,
    pub total: usize,
    pub pull_requests: Vec<PullRequestWork>,
    pub branches: Vec<BranchWork>,
    pub issues: Vec<IssueWork>,
    pub other_activity: Vec<Activity>,
    pub may_be_truncated: bool,
}

#[derive(Clone)]
struct PullRequestMetadata {
    repository: String,
    number: u64,
    title: String,
    html_url: String,
    status: String,
    author: Option<String>,
    head: Option<String>,
    base: Option<String>,
    related_issues: Vec<RelatedIssue>,
}

#[derive(Clone)]
struct IssueMetadata {
    repository: String,
    number: u64,
    title: String,
    html_url: String,
    state: String,
}

pub fn build_report(
    user: String,
    period: String,
    dates: String,
    events: Vec<GitHubEvent>,
    may_be_truncated: bool,
) -> Report {
    let mut pull_requests: BTreeMap<(String, u64), PullRequestWork> = BTreeMap::new();
    let mut branches: BTreeMap<(String, String), BranchWork> = BTreeMap::new();
    let mut issues: BTreeMap<(String, u64), IssueWork> = BTreeMap::new();
    let mut other_activity = Vec::new();
    let mut seen_commits = HashSet::new();

    for event in events {
        let pull_request = pull_request_metadata(&event);
        let issue = issue_metadata(&event);
        let pushed_branch = (event.kind == "PushEvent" && pull_request.is_none())
            .then(|| event_branch(&event))
            .flatten();
        let activities = parse_event(&event, &mut seen_commits);

        for activity in activities {
            if let Some(metadata) = &pull_request {
                let key = (metadata.repository.clone(), metadata.number);
                let entry = pull_requests.entry(key).or_insert_with(|| PullRequestWork {
                    repository: metadata.repository.clone(),
                    number: metadata.number,
                    title: metadata.title.clone(),
                    html_url: metadata.html_url.clone(),
                    status: metadata.status.clone(),
                    relationship: relationship(metadata.author.as_deref(), &user),
                    head: metadata.head.clone(),
                    base: metadata.base.clone(),
                    total: 0,
                    counts: BTreeMap::new(),
                    related_issues: metadata.related_issues.clone(),
                    activities: Vec::new(),
                    last_activity_at: activity.occurred_at,
                });
                merge_pull_request_metadata(entry, metadata, &user);
                entry.last_activity_at = entry.last_activity_at.max(activity.occurred_at);
                entry.activities.push(activity);
            } else if let Some(branch) = &pushed_branch {
                let key = (activity.repository.clone(), branch.clone());
                let entry = branches.entry(key).or_insert_with(|| BranchWork {
                    repository: activity.repository.clone(),
                    branch: branch.clone(),
                    html_url: branch_url(&activity.repository, branch),
                    total: 0,
                    activities: Vec::new(),
                    last_activity_at: activity.occurred_at,
                });
                entry.last_activity_at = entry.last_activity_at.max(activity.occurred_at);
                entry.activities.push(activity);
            } else if let Some(metadata) = &issue {
                let key = (metadata.repository.clone(), metadata.number);
                let entry = issues.entry(key).or_insert_with(|| IssueWork {
                    repository: metadata.repository.clone(),
                    number: metadata.number,
                    title: metadata.title.clone(),
                    html_url: metadata.html_url.clone(),
                    state: metadata.state.clone(),
                    total: 0,
                    activities: Vec::new(),
                    last_activity_at: activity.occurred_at,
                });
                entry.state.clone_from(&metadata.state);
                entry.last_activity_at = entry.last_activity_at.max(activity.occurred_at);
                entry.activities.push(activity);
            } else {
                other_activity.push(activity);
            }
        }
    }

    // Activity on an issue explicitly linked from a PR belongs with that PR.
    // Keep it out of the standalone issue section to avoid saying the same
    // thing twice.
    let related_keys: BTreeSet<_> = pull_requests
        .values()
        .flat_map(|pull| {
            pull.related_issues
                .iter()
                .map(|issue| (issue.repository.clone(), issue.number))
        })
        .collect();
    for pull in pull_requests.values_mut() {
        for related in &mut pull.related_issues {
            if let Some(issue) = issues.get(&(related.repository.clone(), related.number)) {
                related.state.clone_from(&issue.state);
                related.title.clone_from(&issue.title);
                related.html_url.clone_from(&issue.html_url);
                related.activities.clone_from(&issue.activities);
            }
        }
    }
    issues.retain(|key, _| !related_keys.contains(key));

    let mut pull_requests: Vec<_> = pull_requests
        .into_values()
        .map(finalize_pull_request)
        .collect();
    pull_requests.sort_by_key(|pull| {
        (
            Reverse(pull.total),
            Reverse(pull.last_activity_at),
            pull.repository.clone(),
            pull.number,
        )
    });

    let mut branches: Vec<_> = branches
        .into_values()
        .map(|mut branch| {
            branch
                .activities
                .sort_by_key(|activity| Reverse(activity.occurred_at));
            branch.total = branch
                .activities
                .iter()
                .map(|activity| activity.count)
                .sum();
            branch
        })
        .collect();
    branches.sort_by_key(|branch| {
        (
            Reverse(branch.total),
            Reverse(branch.last_activity_at),
            branch.repository.clone(),
            branch.branch.clone(),
        )
    });

    let mut issues: Vec<_> = issues
        .into_values()
        .map(|mut issue| {
            issue
                .activities
                .sort_by_key(|activity| Reverse(activity.occurred_at));
            issue.total = issue.activities.iter().map(|activity| activity.count).sum();
            issue
        })
        .collect();
    issues.sort_by_key(|issue| (Reverse(issue.total), Reverse(issue.last_activity_at)));
    other_activity.sort_by_key(|activity| Reverse(activity.occurred_at));

    let total = pull_requests.iter().map(|pull| pull.total).sum::<usize>()
        + branches.iter().map(|branch| branch.total).sum::<usize>()
        + issues.iter().map(|issue| issue.total).sum::<usize>()
        + other_activity
            .iter()
            .map(|activity| activity.count)
            .sum::<usize>();

    Report {
        user,
        period,
        dates,
        total,
        pull_requests,
        branches,
        issues,
        other_activity,
        may_be_truncated,
    }
}

fn merge_pull_request_metadata(
    pull: &mut PullRequestWork,
    metadata: &PullRequestMetadata,
    user: &str,
) {
    pull.title.clone_from(&metadata.title);
    pull.html_url.clone_from(&metadata.html_url);
    pull.status.clone_from(&metadata.status);
    pull.relationship = relationship(metadata.author.as_deref(), user);
    if metadata.head.is_some() {
        pull.head.clone_from(&metadata.head);
    }
    if metadata.base.is_some() {
        pull.base.clone_from(&metadata.base);
    }
    for issue in &metadata.related_issues {
        if !pull.related_issues.iter().any(|existing| {
            existing.repository == issue.repository && existing.number == issue.number
        }) {
            pull.related_issues.push(issue.clone());
        }
    }
}

fn finalize_pull_request(mut pull: PullRequestWork) -> PullRequestWork {
    pull.activities
        .sort_by_key(|activity| (activity.category, Reverse(activity.occurred_at)));
    for activity in &pull.activities {
        *pull.counts.entry(activity.category).or_insert(0) += activity.count;
    }
    for issue in &mut pull.related_issues {
        issue
            .activities
            .sort_by_key(|activity| Reverse(activity.occurred_at));
        let count: usize = issue.activities.iter().map(|activity| activity.count).sum();
        if count > 0 {
            *pull.counts.entry(Category::Issues).or_insert(0) += count;
        }
    }
    pull.related_issues
        .sort_by_key(|issue| (issue.repository.clone(), issue.number));
    pull.total = pull.counts.values().sum();
    pull
}

fn relationship(author: Option<&str>, user: &str) -> String {
    if author.is_some_and(|author| author.eq_ignore_ascii_case(user)) {
        "authored".to_owned()
    } else {
        "contributed".to_owned()
    }
}

fn parse_event(event: &GitHubEvent, seen_commits: &mut HashSet<String>) -> Vec<Activity> {
    match event.kind.as_str() {
        "PushEvent" => parse_push(event, seen_commits),
        "PullRequestReviewEvent" => vec![review(event)],
        "IssueCommentEvent"
        | "PullRequestReviewCommentEvent"
        | "CommitCommentEvent"
        | "DiscussionCommentEvent" => vec![comment(event)],
        "PullRequestEvent" => vec![pull_request(event)],
        "IssuesEvent" => vec![issue(event)],
        _ => vec![other(event)],
    }
}

fn parse_push(event: &GitHubEvent, seen_commits: &mut HashSet<String>) -> Vec<Activity> {
    let branch = event_branch(event).unwrap_or_else(|| "unknown branch".to_owned());
    let commits = event.payload.pointer("/commits").and_then(Value::as_array);
    let mut result = Vec::new();

    if let Some(commits) = commits {
        for commit in commits {
            let sha = text(commit, "/sha").unwrap_or("unknown");
            if sha != "unknown" && !seen_commits.insert(sha.to_owned()) {
                continue;
            }
            let message = first_line(text(commit, "/message").unwrap_or("Commit"));
            let short_sha = sha.chars().take(7).collect::<String>();
            let url = repo_web_url(&event.repo.name).map(|base| format!("{base}/commit/{sha}"));
            result.push(activity(
                event,
                Category::Commits,
                format!("{short_sha} {message}"),
                url,
                Some(branch.clone()),
                1,
            ));
        }
    }
    if commits.is_some_and(|commits| !commits.is_empty()) {
        return result;
    }

    let count = event
        .payload
        .get("distinct_size")
        .or_else(|| event.payload.get("size"))
        .and_then(Value::as_u64)
        .unwrap_or(1) as usize;
    let head = text(&event.payload, "/_activity_report_commit/sha")
        .or_else(|| text(&event.payload, "/head"))
        .unwrap_or("");
    if !head.is_empty() && !seen_commits.insert(head.to_owned()) {
        return result;
    }
    let short_sha = head.chars().take(7).collect::<String>();
    let message = text(&event.payload, "/_activity_report_commit/message").map(first_line);
    let summary = match (count, message) {
        (1, Some(message)) => format!("{short_sha} {message}"),
        (1, None) => "Pushed 1 commit".to_owned(),
        (count, Some(message)) => {
            format!("Pushed {count} commits; latest: {short_sha} {message}")
        }
        (count, None) => format!("Pushed {count} commits"),
    };
    let url = text(&event.payload, "/_activity_report_commit/html_url")
        .map(str::to_owned)
        .or_else(|| {
            (!head.is_empty())
                .then(|| repo_web_url(&event.repo.name).map(|base| format!("{base}/commit/{head}")))
                .flatten()
        });
    result.push(activity(
        event,
        Category::Commits,
        summary,
        url,
        Some(branch),
        count.max(1),
    ));
    result
}

fn review(event: &GitHubEvent) -> Activity {
    let state = text(&event.payload, "/review/state")
        .unwrap_or("reviewed")
        .to_ascii_lowercase();
    let summary = match state.as_str() {
        "approved" => "Approved",
        "changes_requested" => "Requested changes",
        "commented" => "Reviewed",
        _ => "Reviewed",
    };
    activity(
        event,
        Category::Reviews,
        summary.to_owned(),
        text(&event.payload, "/review/html_url")
            .or_else(|| text(&event.payload, "/_activity_report_pr/html_url"))
            .map(str::to_owned),
        text(&event.payload, "/review/body")
            .filter(|body| !body.trim().is_empty())
            .map(excerpt),
        1,
    )
}

fn comment(event: &GitHubEvent) -> Activity {
    activity(
        event,
        Category::Comments,
        "Commented".to_owned(),
        text(&event.payload, "/comment/html_url").map(str::to_owned),
        text(&event.payload, "/comment/body").map(excerpt),
        1,
    )
}

fn pull_request(event: &GitHubEvent) -> Activity {
    let pr = event
        .payload
        .pointer("/pull_request")
        .unwrap_or(&Value::Null);
    let mut action = text(&event.payload, "/action")
        .unwrap_or("updated")
        .replace('_', " ");
    if action == "closed"
        && (pr.get("merged").and_then(Value::as_bool) == Some(true)
            || event
                .payload
                .pointer("/_activity_report_pr/merged_at")
                .is_some_and(|value| !value.is_null()))
    {
        action = "merged".to_owned();
    }
    activity(
        event,
        Category::PullRequests,
        capitalize(&action),
        text(&event.payload, "/_activity_report_pr/html_url")
            .or_else(|| text(pr, "/html_url"))
            .map(str::to_owned),
        None,
        1,
    )
}

fn issue(event: &GitHubEvent) -> Activity {
    let issue = event.payload.pointer("/issue").unwrap_or(&Value::Null);
    let action = text(&event.payload, "/action")
        .unwrap_or("updated")
        .replace('_', " ");
    activity(
        event,
        Category::Issues,
        capitalize(&action),
        text(issue, "/html_url").map(str::to_owned),
        None,
        1,
    )
}

fn other(event: &GitHubEvent) -> Activity {
    let summary = match event.kind.as_str() {
        "CreateEvent" => format!(
            "Created {} {}",
            text(&event.payload, "/ref_type").unwrap_or("reference"),
            text(&event.payload, "/ref").unwrap_or("(unknown)")
        ),
        "DeleteEvent" => format!(
            "Deleted {} {}",
            text(&event.payload, "/ref_type").unwrap_or("reference"),
            text(&event.payload, "/ref").unwrap_or("(unknown)")
        ),
        "ReleaseEvent" => format!(
            "Published release {}",
            text(&event.payload, "/release/name")
                .or_else(|| text(&event.payload, "/release/tag_name"))
                .unwrap_or("(unnamed)")
        ),
        "DiscussionEvent" => format!(
            "Started discussion — {}",
            text(&event.payload, "/discussion/title").unwrap_or("(untitled)")
        ),
        "ForkEvent" => "Forked the repository".to_owned(),
        "WatchEvent" => "Starred the repository".to_owned(),
        _ => {
            let action = text(&event.payload, "/action")
                .map(|value| format!(" ({})", value.replace('_', " ")))
                .unwrap_or_default();
            let friendly = event.kind.strip_suffix("Event").unwrap_or(&event.kind);
            format!("{friendly}{action}")
        }
    };
    activity(
        event,
        Category::Other,
        summary,
        repo_web_url(&event.repo.name),
        None,
        1,
    )
}

fn activity(
    event: &GitHubEvent,
    category: Category,
    summary: String,
    url: Option<String>,
    detail: Option<String>,
    count: usize,
) -> Activity {
    Activity {
        category,
        repository: event.repo.name.clone(),
        summary,
        url,
        detail,
        occurred_at: event.created_at,
        private: !event.public,
        count,
    }
}

fn pull_request_metadata(event: &GitHubEvent) -> Option<PullRequestMetadata> {
    let enriched = event.payload.pointer("/_activity_report_pr");
    let number = enriched
        .and_then(|value| number(value, "/number"))
        .or_else(|| number(&event.payload, "/pull_request/number"))
        .or_else(|| number(&event.payload, "/number"))
        .or_else(|| {
            event
                .payload
                .pointer("/issue/pull_request")
                .is_some()
                .then(|| number(&event.payload, "/issue/number"))
                .flatten()
        })?;
    let is_pull_request = enriched.is_some()
        || matches!(
            event.kind.as_str(),
            "PullRequestEvent" | "PullRequestReviewEvent" | "PullRequestReviewCommentEvent"
        )
        || event.payload.pointer("/issue/pull_request").is_some();
    if !is_pull_request {
        return None;
    }

    let title = enriched
        .and_then(|value| text(value, "/title"))
        .or_else(|| text(&event.payload, "/pull_request/title"))
        .or_else(|| text(&event.payload, "/issue/title"))
        .unwrap_or("Pull request")
        .to_owned();
    let html_url = enriched
        .and_then(|value| text(value, "/html_url"))
        .or_else(|| text(&event.payload, "/pull_request/html_url"))
        .or_else(|| text(&event.payload, "/issue/html_url"))
        .unwrap_or("")
        .to_owned();
    let status = if event
        .payload
        .pointer("/pull_request/merged")
        .and_then(Value::as_bool)
        == Some(true)
        || enriched
            .and_then(|value| value.get("merged_at"))
            .is_some_and(|value| !value.is_null())
    {
        "merged"
    } else if enriched
        .and_then(|value| value.get("draft"))
        .and_then(Value::as_bool)
        == Some(true)
    {
        "draft"
    } else {
        enriched
            .and_then(|value| text(value, "/state"))
            .unwrap_or("open")
    }
    .to_owned();
    let related_issues = enriched
        .and_then(|value| value.get("related_issues"))
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default();

    Some(PullRequestMetadata {
        repository: event.repo.name.clone(),
        number,
        title,
        html_url,
        status,
        author: enriched
            .and_then(|value| text(value, "/author"))
            .map(str::to_owned),
        head: enriched
            .and_then(|value| text(value, "/head"))
            .map(str::to_owned),
        base: enriched
            .and_then(|value| text(value, "/base"))
            .map(str::to_owned),
        related_issues,
    })
}

fn issue_metadata(event: &GitHubEvent) -> Option<IssueMetadata> {
    if !matches!(event.kind.as_str(), "IssuesEvent" | "IssueCommentEvent")
        || event.payload.pointer("/issue/pull_request").is_some()
    {
        return None;
    }
    let issue = event.payload.pointer("/issue")?;
    let number = issue.get("number").and_then(Value::as_u64)?;
    Some(IssueMetadata {
        repository: event.repo.name.clone(),
        number,
        title: text(issue, "/title").unwrap_or("Issue").to_owned(),
        html_url: text(issue, "/html_url").unwrap_or("").to_owned(),
        state: text(issue, "/state").unwrap_or("open").to_owned(),
    })
}

fn event_branch(event: &GitHubEvent) -> Option<String> {
    event
        .payload
        .get("ref")
        .and_then(Value::as_str)
        .and_then(|reference| reference.strip_prefix("refs/heads/").or(Some(reference)))
        .map(str::to_owned)
}

fn text<'a>(value: &'a Value, pointer: &str) -> Option<&'a str> {
    value.pointer(pointer).and_then(Value::as_str)
}

fn number(value: &Value, pointer: &str) -> Option<u64> {
    value.pointer(pointer).and_then(Value::as_u64)
}

fn first_line(value: &str) -> String {
    value.lines().next().unwrap_or(value).trim().to_owned()
}

fn excerpt(value: &str) -> String {
    let single_line = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if single_line.chars().count() <= 120 {
        single_line
    } else {
        format!("{}…", single_line.chars().take(119).collect::<String>())
    }
}

fn capitalize(value: &str) -> String {
    let mut chars = value.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}

fn repo_web_url(repository: &str) -> Option<String> {
    repository
        .contains('/')
        .then(|| format!("https://github.com/{repository}"))
}

fn branch_url(repository: &str, branch: &str) -> String {
    let branch = branch
        .split('/')
        .map(urlencoding::encode)
        .collect::<Vec<_>>()
        .join("/");
    repo_web_url(repository)
        .map(|base| format!("{base}/tree/{branch}"))
        .unwrap_or_default()
}

pub fn local_time(timestamp: DateTime<Utc>) -> String {
    timestamp.with_timezone(&Local).format("%H:%M").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::{Actor, Repository};
    use chrono::TimeZone;
    use serde_json::json;

    fn event(kind: &str, repo: &str, payload: Value, hour: u32) -> GitHubEvent {
        GitHubEvent {
            id: format!("{kind}-{hour}"),
            kind: kind.to_owned(),
            actor: Actor {
                login: "octocat".to_owned(),
            },
            repo: Repository {
                name: repo.to_owned(),
            },
            payload,
            public: true,
            created_at: Utc.with_ymd_and_hms(2026, 10, 26, hour, 0, 0).unwrap(),
        }
    }

    fn pr(number: u64, title: &str) -> Value {
        json!({
            "number": number,
            "title": title,
            "html_url": format!("https://github.com/acme/api/pull/{number}"),
            "state": "open",
            "draft": false,
            "merged_at": null,
            "author": "octocat",
            "head": "feature",
            "base": "main",
            "related_issues": []
        })
    }

    #[test]
    fn groups_activity_around_pull_requests() {
        let events = vec![
            event(
                "PullRequestReviewEvent",
                "acme/api",
                json!({"_activity_report_pr": pr(7, "Cache"), "pull_request":{"number":7}, "review":{"state":"approved"}}),
                9,
            ),
            event(
                "IssueCommentEvent",
                "acme/api",
                json!({"_activity_report_pr": pr(7, "Cache"), "issue":{"number":7,"pull_request":{}}, "comment":{"body":"Looks good"}}),
                10,
            ),
        ];
        let report = build_report(
            "octocat".into(),
            "yesterday".into(),
            "2026-10-26".into(),
            events,
            false,
        );
        assert_eq!(report.pull_requests.len(), 1);
        assert_eq!(report.pull_requests[0].title, "Cache");
        assert_eq!(report.pull_requests[0].relationship, "authored");
        assert_eq!(report.pull_requests[0].total, 2);
        assert!(report.issues.is_empty());
    }

    #[test]
    fn moves_linked_issue_activity_under_the_pull_request() {
        let mut metadata = pr(7, "Cache");
        metadata["related_issues"] = json!([{
            "repository": "acme/api", "number": 3, "title": "Slow cache",
            "html_url": "https://github.com/acme/api/issues/3", "state": "closed"
        }]);
        let events = vec![
            event(
                "PullRequestEvent",
                "acme/api",
                json!({"_activity_report_pr": metadata, "number":7, "action":"closed", "pull_request":{"merged":true}}),
                9,
            ),
            event(
                "IssuesEvent",
                "acme/api",
                json!({"action":"closed", "issue":{"number":3,"title":"Slow cache","html_url":"https://github.com/acme/api/issues/3","state":"closed"}}),
                10,
            ),
        ];
        let report = build_report(
            "octocat".into(),
            "yesterday".into(),
            "2026-10-26".into(),
            events,
            false,
        );
        assert_eq!(report.pull_requests[0].status, "merged");
        assert_eq!(
            report.pull_requests[0].related_issues[0].activities.len(),
            1
        );
        assert_eq!(report.pull_requests[0].total, 2);
        assert!(report.issues.is_empty());
    }

    #[test]
    fn groups_pushes_without_pull_requests_by_branch() {
        let first = json!({"ref":"refs/heads/feature/header","commits":[{"sha":"1234567890","message":"Fix header"}]});
        let second = json!({"ref":"refs/heads/feature/header","commits":[{"sha":"abcdef1234","message":"Add header test"}]});
        let report = build_report(
            "octocat".into(),
            "yesterday".into(),
            "2026-10-26".into(),
            vec![
                event("PushEvent", "acme/web", first, 8),
                event("PushEvent", "acme/web", second, 9),
            ],
            false,
        );
        assert!(report.pull_requests.is_empty());
        assert_eq!(report.branches.len(), 1);
        assert_eq!(report.branches[0].repository, "acme/web");
        assert_eq!(report.branches[0].branch, "feature/header");
        assert_eq!(report.branches[0].total, 2);
        assert_eq!(report.branches[0].activities.len(), 2);
        assert!(
            report.branches[0]
                .html_url
                .ends_with("/tree/feature/header")
        );
        assert!(report.other_activity.is_empty());
        assert_eq!(report.total, 2);
    }

    #[test]
    fn keeps_pushes_with_pull_request_metadata_under_the_pull_request() {
        let payload = json!({
            "ref":"refs/heads/feature/cache",
            "commits":[{"sha":"1234567890","message":"Tune cache"}],
            "_activity_report_pr": pr(7, "Cache")
        });
        let report = build_report(
            "octocat".into(),
            "yesterday".into(),
            "2026-10-26".into(),
            vec![event("PushEvent", "acme/api", payload, 8)],
            false,
        );
        assert_eq!(report.pull_requests.len(), 1);
        assert!(report.branches.is_empty());
        assert_eq!(report.pull_requests[0].total, 1);
    }
}
