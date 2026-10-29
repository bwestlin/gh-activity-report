use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use regex::Regex;
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::task::JoinSet;

use crate::timeframe::Timeframe;

const PER_PAGE: usize = 100;
// GitHub limits event timelines to 300 entries and rejects later pages.
const MAX_PAGES: usize = 3;
const MAX_COMMIT_PAGES: usize = 10;

#[derive(Clone)]
pub struct GitHubClient {
    client: Client,
    api_url: String,
}

#[derive(Debug, Deserialize)]
pub struct GitHubEvent {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub actor: Actor,
    pub repo: Repository,
    #[serde(default)]
    pub payload: Value,
    #[serde(default)]
    pub public: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct Actor {
    pub login: String,
}

#[derive(Debug, Deserialize)]
pub struct Repository {
    pub name: String,
}

#[derive(Debug)]
pub struct EventsResult {
    pub events: Vec<GitHubEvent>,
    pub may_be_truncated: bool,
}

#[derive(Deserialize)]
struct Viewer {
    login: String,
}

#[derive(Debug, Deserialize)]
struct PullRequestSummary {
    node_id: String,
    number: u64,
    title: String,
    html_url: String,
    body: Option<String>,
    state: String,
    #[serde(default)]
    draft: bool,
    merged_at: Option<DateTime<Utc>>,
    user: UserSummary,
    head: BranchSummary,
    base: BranchSummary,
}

#[derive(Debug, Deserialize)]
struct UserSummary {
    login: String,
}

#[derive(Debug, Deserialize)]
struct BranchSummary {
    #[serde(rename = "ref")]
    name: String,
}

#[derive(Debug, Deserialize)]
struct IssueSummary {
    number: u64,
    title: String,
    html_url: String,
    state: String,
    pull_request: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct GraphQlResponse<T> {
    data: Option<T>,
}

#[derive(Debug, Deserialize)]
struct ClosingIssuesData {
    nodes: Vec<Option<ClosingIssuesPullRequest>>,
}

#[derive(Debug, Deserialize)]
struct ClosingIssuesPullRequest {
    id: String,
    #[serde(rename = "closingIssuesReferences")]
    closing_issues: ClosingIssuesConnection,
}

#[derive(Debug, Deserialize)]
struct ClosingIssuesConnection {
    nodes: Vec<ClosingIssue>,
}

#[derive(Clone, Debug, Deserialize)]
struct ClosingIssue {
    number: u64,
    title: String,
    url: String,
    state: String,
    repository: ClosingIssueRepository,
}

#[derive(Clone, Debug, Deserialize)]
struct ClosingIssueRepository {
    #[serde(rename = "nameWithOwner")]
    name_with_owner: String,
}

#[derive(Debug, Deserialize)]
struct CommitSummary {
    sha: String,
    html_url: String,
    commit: CommitData,
}

#[derive(Debug, Deserialize)]
struct CommitData {
    message: String,
    author: CommitAuthor,
}

#[derive(Debug, Deserialize)]
struct CommitAuthor {
    date: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct ContributedRepositoriesData {
    viewer: ContributedRepositoriesViewer,
}

#[derive(Debug, Deserialize)]
struct ContributedRepositoriesViewer {
    #[serde(rename = "repositoriesContributedTo")]
    repositories_contributed_to: ContributedRepositoryConnection,
}

#[derive(Debug, Deserialize)]
struct ContributedRepositoryConnection {
    nodes: Vec<ContributedRepository>,
    #[serde(rename = "pageInfo")]
    page_info: PageInfo,
}

#[derive(Debug, Deserialize)]
struct PageInfo {
    #[serde(rename = "hasNextPage")]
    has_next_page: bool,
    #[serde(rename = "endCursor")]
    end_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct ContributedRepository {
    #[serde(rename = "nameWithOwner")]
    name_with_owner: String,
    #[serde(rename = "isPrivate")]
    is_private: bool,
    #[serde(rename = "pushedAt")]
    pushed_at: Option<DateTime<Utc>>,
    #[serde(rename = "defaultBranchRef")]
    default_branch_ref: Option<BranchName>,
}

#[derive(Clone, Debug, Deserialize)]
struct BranchName {
    name: String,
}

impl GitHubClient {
    pub fn new(token: &str, api_url: &str) -> Result<Self> {
        let client = Client::builder()
            .user_agent(format!("gh-activity-report/{}", env!("CARGO_PKG_VERSION")))
            .default_headers({
                let mut headers = reqwest::header::HeaderMap::new();
                headers.insert(
                    reqwest::header::ACCEPT,
                    "application/vnd.github+json".parse().unwrap(),
                );
                headers.insert("X-GitHub-Api-Version", "2022-11-28".parse().unwrap());
                headers.insert(
                    reqwest::header::AUTHORIZATION,
                    format!("Bearer {token}")
                        .parse()
                        .context("invalid token header")?,
                );
                headers
            })
            .build()
            .context("failed to build GitHub client")?;
        Ok(Self {
            client,
            api_url: api_url.trim_end_matches('/').to_owned(),
        })
    }

    pub async fn viewer_login(&self) -> Result<String> {
        let url = format!("{}/user", self.api_url);
        Ok(self.get_json::<Viewer>(&url).await?.login)
    }

    pub async fn events(
        &self,
        user: &str,
        timeframe: &Timeframe,
        organization: Option<&str>,
    ) -> Result<EventsResult> {
        // TODO: Supplement reviews, comments, and PR/issue changes with contribution
        // and timeline queries; the current fallback recovers default-branch commits.
        let encoded_user = urlencoding::encode(user);
        let mut events = Vec::new();
        let mut exhausted = false;
        let mut oldest_event_at: Option<DateTime<Utc>> = None;

        for page in 1..=MAX_PAGES {
            let url = format!(
                "{}/users/{encoded_user}/events?per_page={PER_PAGE}&page={page}",
                self.api_url
            );
            let page_events = self.get_json::<Vec<GitHubEvent>>(&url).await?;
            if page_events.is_empty() {
                exhausted = true;
                break;
            }

            let page_len = page_events.len();
            for event in page_events
                .iter()
                .filter(|event| event_is_in_scope(event, user, organization))
            {
                oldest_event_at = Some(
                    oldest_event_at.map_or(event.created_at, |oldest| oldest.min(event.created_at)),
                );
            }
            events.extend(matching_events(page_events, user, timeframe, organization));

            if page_len < PER_PAGE {
                exhausted = true;
                break;
            }
        }

        let fallback_events = self
            .default_branch_commit_events(user, timeframe, organization)
            .await;
        append_missing_commit_events(&mut events, fallback_events);

        self.enrich_commits(&mut events).await;
        self.enrich_pull_requests(&mut events).await;

        Ok(EventsResult {
            events,
            may_be_truncated: timeline_may_be_truncated(exhausted, oldest_event_at, timeframe),
        })
    }

    /// The Events API often returns abbreviated pull request objects without
    /// titles. Fetch each referenced PR once so report lines remain useful as
    /// a memory aid. This is deliberately best-effort: missing repository
    /// permissions should not hide the activity event itself.
    async fn enrich_pull_requests(&self, events: &mut [GitHubEvent]) {
        let references: BTreeSet<_> = events.iter().filter_map(pull_request_reference).collect();
        let push_branches: BTreeSet<_> = events
            .iter()
            .filter(|event| event.kind == "PushEvent")
            .filter_map(|event| {
                event_branch(event)
                    .filter(|branch| branch != "main" && branch != "master")
                    .map(|branch| (event.repo.name.clone(), branch))
            })
            .collect();
        let mut tasks = JoinSet::new();
        let mut summaries = self.open_pull_requests_for_branches(&push_branches).await;

        for (repository, number) in references {
            let client = self.clone();
            tasks.spawn(async move {
                let result = match repository.split_once('/') {
                    Some((owner, repo)) => {
                        let url = format!(
                            "{}/repos/{}/{}/pulls/{number}",
                            client.api_url,
                            urlencoding::encode(owner),
                            urlencoding::encode(repo)
                        );
                        client.get_json::<PullRequestSummary>(&url).await
                    }
                    None => Err(anyhow::anyhow!("invalid repository name {repository}")),
                };
                ((repository, number), result)
            });

            // Bound concurrency so a busy week does not burst requests.
            if tasks.len() >= 12
                && let Some(result) = tasks.join_next().await
            {
                collect_summary(result, &mut summaries);
            }
        }
        while let Some(result) = tasks.join_next().await {
            collect_summary(result, &mut summaries);
        }

        let closing_issues = self.closing_issues(&summaries).await;
        let mut issue_references: BTreeSet<_> = summaries
            .iter()
            .flat_map(|((repository, _), summary)| {
                related_issue_references(repository, summary.body.as_deref().unwrap_or(""))
            })
            .collect();
        issue_references.extend(
            closing_issues
                .values()
                .flatten()
                .map(|issue| (issue.repository.name_with_owner.clone(), issue.number)),
        );
        let mut issues = self.fetch_issues(issue_references).await;
        for issue in closing_issues.values().flatten() {
            issues
                .entry((issue.repository.name_with_owner.clone(), issue.number))
                .or_insert_with(|| IssueSummary {
                    number: issue.number,
                    title: issue.title.clone(),
                    html_url: issue.url.clone(),
                    state: issue.state.to_ascii_lowercase(),
                    pull_request: None,
                });
        }
        let branches: BTreeMap<_, _> = summaries
            .iter()
            .map(|(key, summary)| ((key.0.clone(), summary.head.name.clone()), key.clone()))
            .collect();

        for event in events {
            let key = pull_request_reference(event)
                .or_else(|| commit_pull_request_reference(event))
                .filter(|key| summaries.contains_key(key))
                .or_else(|| {
                    event_branch(event).and_then(|branch| {
                        branches.get(&(event.repo.name.clone(), branch)).cloned()
                    })
                });
            let Some(key) = key else {
                continue;
            };
            let Some(summary) = summaries.get(&key) else {
                continue;
            };
            let mut related_references =
                related_issue_references(&key.0, summary.body.as_deref().unwrap_or(""));
            let closing_references: BTreeSet<_> = closing_issues
                .get(&key)
                .into_iter()
                .flatten()
                .map(|issue| (issue.repository.name_with_owner.clone(), issue.number))
                .collect();
            related_references.extend(closing_references.iter().cloned());
            let related_issues: Vec<_> = related_references
                .into_iter()
                .filter_map(|reference| {
                    issues.get(&reference).map(|issue| {
                        let relationship = if closing_references.contains(&reference) {
                            "closes"
                        } else {
                            "related"
                        };
                        json!({
                            "repository": reference.0,
                            "number": issue.number,
                            "title": issue.title,
                            "html_url": issue.html_url,
                            "state": issue.state,
                            "relationship": relationship,
                        })
                    })
                })
                .collect();
            if let Some(payload) = event.payload.as_object_mut() {
                payload.insert(
                    "_activity_report_pr".to_owned(),
                    json!({
                        "number": summary.number,
                        "title": summary.title,
                        "html_url": summary.html_url,
                        "state": summary.state,
                        "draft": summary.draft,
                        "merged_at": summary.merged_at,
                        "author": summary.user.login,
                        "head": summary.head.name,
                        "base": summary.base.name,
                        "related_issues": related_issues,
                    }),
                );
            }
        }
    }

    async fn open_pull_requests_for_branches(
        &self,
        branches: &BTreeSet<(String, String)>,
    ) -> BTreeMap<(String, u64), PullRequestSummary> {
        let repositories: BTreeSet<_> = branches
            .iter()
            .map(|(repository, _)| repository.clone())
            .collect();
        let mut tasks = JoinSet::new();
        let mut summaries = BTreeMap::new();

        for repository in repositories {
            let client = self.clone();
            tasks.spawn(async move {
                let result = match repository.split_once('/') {
                    Some((owner, repo)) => {
                        let url = format!(
                            "{}/repos/{}/{}/pulls?state=open&per_page=100",
                            client.api_url,
                            urlencoding::encode(owner),
                            urlencoding::encode(repo)
                        );
                        client.get_json::<Vec<PullRequestSummary>>(&url).await
                    }
                    None => Err(anyhow::anyhow!("invalid repository name {repository}")),
                };
                (repository, result)
            });
        }
        while let Some(result) = tasks.join_next().await {
            if let Ok((repository, Ok(pulls))) = result {
                for pull in pulls {
                    if branches.contains(&(repository.clone(), pull.head.name.clone())) {
                        summaries.insert((repository.clone(), pull.number), pull);
                    }
                }
            }
        }
        summaries
    }

    async fn closing_issues(
        &self,
        pull_requests: &BTreeMap<(String, u64), PullRequestSummary>,
    ) -> BTreeMap<(String, u64), Vec<ClosingIssue>> {
        let by_node_id: BTreeMap<_, _> = pull_requests
            .iter()
            .map(|(key, pull)| (pull.node_id.clone(), key.clone()))
            .collect();
        if by_node_id.is_empty() {
            return BTreeMap::new();
        }

        let query = r#"
            query ClosingIssues($ids: [ID!]!) {
              nodes(ids: $ids) {
                ... on PullRequest {
                  id
                  closingIssuesReferences(first: 50) {
                    nodes {
                      number
                      title
                      url
                      state
                      repository { nameWithOwner }
                    }
                  }
                }
              }
            }
        "#;
        let body = json!({
            "query": query,
            "variables": { "ids": by_node_id.keys().collect::<Vec<_>>() },
        });
        let url = graphql_url(&self.api_url);
        let Ok(response) = self
            .post_json::<GraphQlResponse<ClosingIssuesData>>(&url, &body)
            .await
        else {
            return BTreeMap::new();
        };
        let Some(data) = response.data else {
            return BTreeMap::new();
        };

        data.nodes
            .into_iter()
            .flatten()
            .filter_map(|pull| {
                by_node_id
                    .get(&pull.id)
                    .cloned()
                    .map(|key| (key, pull.closing_issues.nodes))
            })
            .collect()
    }

    async fn fetch_issues(
        &self,
        references: BTreeSet<(String, u64)>,
    ) -> BTreeMap<(String, u64), IssueSummary> {
        let mut tasks = JoinSet::new();
        let mut summaries = BTreeMap::new();
        for (repository, number) in references {
            let client = self.clone();
            tasks.spawn(async move {
                let result = match repository.split_once('/') {
                    Some((owner, repo)) => {
                        let url = format!(
                            "{}/repos/{}/{}/issues/{number}",
                            client.api_url,
                            urlencoding::encode(owner),
                            urlencoding::encode(repo)
                        );
                        client.get_json::<IssueSummary>(&url).await
                    }
                    None => Err(anyhow::anyhow!("invalid repository name {repository}")),
                };
                ((repository, number), result)
            });
            if tasks.len() >= 12
                && let Some(result) = tasks.join_next().await
            {
                collect_issue(result, &mut summaries);
            }
        }
        while let Some(result) = tasks.join_next().await {
            collect_issue(result, &mut summaries);
        }
        summaries.retain(|_, issue| issue.pull_request.is_none());
        summaries
    }

    async fn default_branch_commit_events(
        &self,
        user: &str,
        timeframe: &Timeframe,
        organization: Option<&str>,
    ) -> Vec<GitHubEvent> {
        let repositories = self
            .recently_contributed_repositories(timeframe, organization)
            .await;
        let mut tasks = JoinSet::new();
        let mut events = Vec::new();

        for repository in repositories {
            let client = self.clone();
            let user = user.to_owned();
            let timeframe = timeframe.clone();
            tasks.spawn(async move {
                client
                    .default_branch_commits(repository, user, timeframe)
                    .await
            });
            if tasks.len() >= 12
                && let Some(Ok(Ok(mut repository_events))) = tasks.join_next().await
            {
                events.append(&mut repository_events);
            }
        }
        while let Some(result) = tasks.join_next().await {
            if let Ok(Ok(mut repository_events)) = result {
                events.append(&mut repository_events);
            }
        }
        events
    }

    async fn recently_contributed_repositories(
        &self,
        timeframe: &Timeframe,
        organization: Option<&str>,
    ) -> Vec<ContributedRepository> {
        let query = r#"
            query ContributedRepositories($cursor: String) {
              viewer {
                repositoriesContributedTo(
                  first: 100
                  after: $cursor
                  includeUserRepositories: true
                  contributionTypes: [COMMIT]
                ) {
                  nodes {
                    nameWithOwner
                    isPrivate
                    pushedAt
                    defaultBranchRef { name }
                  }
                  pageInfo { hasNextPage endCursor }
                }
              }
            }
        "#;
        let url = graphql_url(&self.api_url);
        let mut cursor = None;
        let mut repositories = Vec::new();

        loop {
            let body = json!({
                "query": query,
                "variables": { "cursor": cursor.clone() },
            });
            let Ok(response) = self
                .post_json::<GraphQlResponse<ContributedRepositoriesData>>(&url, &body)
                .await
            else {
                break;
            };
            let Some(data) = response.data else {
                break;
            };
            let connection = data.viewer.repositories_contributed_to;
            repositories.extend(connection.nodes);
            if !connection.page_info.has_next_page {
                break;
            }
            let Some(next_cursor) = connection.page_info.end_cursor else {
                break;
            };
            cursor = Some(next_cursor);
        }

        repositories
            .into_iter()
            .filter(|repository| {
                repository
                    .pushed_at
                    .is_some_and(|pushed_at| pushed_at >= timeframe.start)
            })
            .filter(|repository| {
                organization.is_none_or(|organization| {
                    repository
                        .name_with_owner
                        .split_once('/')
                        .is_some_and(|(owner, _)| owner.eq_ignore_ascii_case(organization))
                })
            })
            .collect()
    }

    async fn default_branch_commits(
        &self,
        repository: ContributedRepository,
        user: String,
        timeframe: Timeframe,
    ) -> Result<Vec<GitHubEvent>> {
        let Some(branch) = repository.default_branch_ref.map(|branch| branch.name) else {
            return Ok(Vec::new());
        };
        let Some((owner, repo)) = repository.name_with_owner.split_once('/') else {
            return Ok(Vec::new());
        };
        let mut events = Vec::new();

        for page in 1..=MAX_COMMIT_PAGES {
            let url = format!(
                "{}/repos/{}/{}/commits?sha={}&author={}&since={}&until={}&per_page={PER_PAGE}&page={page}",
                self.api_url,
                urlencoding::encode(owner),
                urlencoding::encode(repo),
                urlencoding::encode(&branch),
                urlencoding::encode(&user),
                urlencoding::encode(&timeframe.start.to_rfc3339()),
                urlencoding::encode(&timeframe.end.to_rfc3339()),
            );
            let commits = self.get_json::<Vec<CommitSummary>>(&url).await?;
            let page_len = commits.len();
            for commit in commits
                .into_iter()
                .filter(|commit| timeframe.contains(commit.commit.author.date))
            {
                events.push(commit_event(
                    &repository.name_with_owner,
                    &branch,
                    &user,
                    repository.is_private,
                    commit,
                ));
            }
            if page_len < PER_PAGE {
                break;
            }
        }
        Ok(events)
    }

    /// Push event payloads may omit their commit list. Resolve the pushed head
    /// so a single-commit push can show its message instead of only its SHA.
    async fn enrich_commits(&self, events: &mut [GitHubEvent]) {
        let references: BTreeSet<_> = events
            .iter()
            .filter(|event| event.kind == "PushEvent")
            .filter_map(|event| {
                event
                    .payload
                    .get("head")
                    .and_then(Value::as_str)
                    .filter(|head| !head.is_empty())
                    .map(|head| (event.repo.name.clone(), head.to_owned()))
            })
            .collect();
        let mut tasks = JoinSet::new();
        let mut summaries = BTreeMap::new();

        for (repository, sha) in references {
            let client = self.clone();
            tasks.spawn(async move {
                let result = match repository.split_once('/') {
                    Some((owner, repo)) => {
                        let url = format!(
                            "{}/repos/{}/{}/commits/{}",
                            client.api_url,
                            urlencoding::encode(owner),
                            urlencoding::encode(repo),
                            urlencoding::encode(&sha)
                        );
                        client.get_json::<CommitSummary>(&url).await
                    }
                    None => Err(anyhow::anyhow!("invalid repository name {repository}")),
                };
                ((repository, sha), result)
            });

            if tasks.len() >= 12
                && let Some(result) = tasks.join_next().await
            {
                collect_commit(result, &mut summaries);
            }
        }
        while let Some(result) = tasks.join_next().await {
            collect_commit(result, &mut summaries);
        }

        for event in events {
            let Some(sha) = event.payload.get("head").and_then(Value::as_str) else {
                continue;
            };
            let key = (event.repo.name.clone(), sha.to_owned());
            let Some(summary) = summaries.get(&key) else {
                continue;
            };
            if let Some(payload) = event.payload.as_object_mut() {
                payload.insert(
                    "_activity_report_commit".to_owned(),
                    json!({
                        "sha": summary.sha,
                        "message": summary.commit.message,
                        "html_url": summary.html_url,
                    }),
                );
            }
        }
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .context("GitHub request failed")?;
        let status = response.status();
        if status.is_success() {
            return response
                .json()
                .await
                .context("invalid response from GitHub");
        }

        let body: Value = response.json().await.unwrap_or(Value::Null);
        let message = body
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("unknown GitHub API error");
        match status {
            StatusCode::UNAUTHORIZED => bail!("GitHub rejected GITHUB_TOKEN: {message}"),
            StatusCode::NOT_FOUND => bail!("GitHub resource not found: {message}"),
            StatusCode::FORBIDDEN => bail!("GitHub denied the request: {message}"),
            _ => bail!("GitHub API returned {status}: {message}"),
        }
    }

    async fn post_json<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        body: &Value,
    ) -> Result<T> {
        let response = self
            .client
            .post(url)
            .json(body)
            .send()
            .await
            .context("GitHub GraphQL request failed")?;
        let status = response.status();
        if status.is_success() {
            return response
                .json()
                .await
                .context("invalid GraphQL response from GitHub");
        }
        let body: Value = response.json().await.unwrap_or(Value::Null);
        let message = body
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("unknown GitHub GraphQL error");
        bail!("GitHub GraphQL returned {status}: {message}")
    }
}

fn matching_events(
    page_events: Vec<GitHubEvent>,
    user: &str,
    timeframe: &Timeframe,
    organization: Option<&str>,
) -> Vec<GitHubEvent> {
    // GitHub's events feed is not guaranteed to be strictly chronological.
    // Filter the complete page instead of stopping at the first old event.
    page_events
        .into_iter()
        .filter(|event| {
            event_is_in_scope(event, user, organization) && timeframe.contains(event.created_at)
        })
        .collect()
}

fn commit_event(
    repository: &str,
    branch: &str,
    user: &str,
    private: bool,
    commit: CommitSummary,
) -> GitHubEvent {
    let CommitSummary {
        sha,
        html_url,
        commit: CommitData { message, author },
    } = commit;
    let created_at = author.date;
    GitHubEvent {
        id: format!("contribution-commit-{sha}"),
        kind: "PushEvent".to_owned(),
        actor: Actor {
            login: user.to_owned(),
        },
        repo: Repository {
            name: repository.to_owned(),
        },
        payload: json!({
            "ref": format!("refs/heads/{branch}"),
            "head": sha,
            "size": 1,
            "distinct_size": 1,
            "commits": [{
                "sha": sha,
                "message": message,
            }],
            "_activity_report_commit": {
                "sha": sha,
                "message": message,
                "html_url": html_url,
            },
        }),
        public: !private,
        created_at,
    }
}

fn append_missing_commit_events(events: &mut Vec<GitHubEvent>, fallback_events: Vec<GitHubEvent>) {
    let mut seen = BTreeSet::new();
    for event in events.iter() {
        collect_commit_shas(event, &mut seen);
    }
    for event in fallback_events {
        let mut shas = BTreeSet::new();
        collect_commit_shas(&event, &mut shas);
        if shas.iter().any(|sha| seen.contains(sha)) {
            continue;
        }
        seen.extend(shas);
        events.push(event);
    }
}

fn collect_commit_shas(event: &GitHubEvent, shas: &mut BTreeSet<String>) {
    if event.kind != "PushEvent" {
        return;
    }
    if let Some(head) = event.payload.get("head").and_then(Value::as_str)
        && !head.is_empty()
    {
        shas.insert(head.to_owned());
    }
    if let Some(commits) = event.payload.get("commits").and_then(Value::as_array) {
        shas.extend(
            commits
                .iter()
                .filter_map(|commit| commit.get("sha").and_then(Value::as_str))
                .map(str::to_owned),
        );
    }
}

fn event_is_in_scope(event: &GitHubEvent, user: &str, organization: Option<&str>) -> bool {
    event.actor.login.eq_ignore_ascii_case(user)
        && organization.is_none_or(|organization| {
            event
                .repo
                .name
                .split_once('/')
                .is_some_and(|(owner, _)| owner.eq_ignore_ascii_case(organization))
        })
}

fn timeline_may_be_truncated(
    exhausted: bool,
    oldest_event_at: Option<DateTime<Utc>>,
    timeframe: &Timeframe,
) -> bool {
    !exhausted && oldest_event_at.is_none_or(|oldest| oldest >= timeframe.start)
}

fn graphql_url(api_url: &str) -> String {
    if let Some(root) = api_url.strip_suffix("/api/v3") {
        format!("{root}/api/graphql")
    } else {
        format!("{}/graphql", api_url.trim_end_matches('/'))
    }
}

fn collect_summary(
    result: Result<((String, u64), Result<PullRequestSummary>), tokio::task::JoinError>,
    summaries: &mut BTreeMap<(String, u64), PullRequestSummary>,
) {
    if let Ok((key, Ok(summary))) = result {
        summaries.insert(key, summary);
    }
}

fn collect_commit(
    result: Result<((String, String), Result<CommitSummary>), tokio::task::JoinError>,
    summaries: &mut BTreeMap<(String, String), CommitSummary>,
) {
    if let Ok((key, Ok(summary))) = result {
        summaries.insert(key, summary);
    }
}

fn collect_issue(
    result: Result<((String, u64), Result<IssueSummary>), tokio::task::JoinError>,
    summaries: &mut BTreeMap<(String, u64), IssueSummary>,
) {
    if let Ok((key, Ok(summary))) = result {
        summaries.insert(key, summary);
    }
}

fn event_branch(event: &GitHubEvent) -> Option<String> {
    if !matches!(
        event.kind.as_str(),
        "PushEvent" | "CreateEvent" | "DeleteEvent"
    ) {
        return None;
    }
    event
        .payload
        .get("ref")
        .and_then(Value::as_str)
        .and_then(|reference| reference.strip_prefix("refs/heads/").or(Some(reference)))
        .map(str::to_owned)
}

fn commit_pull_request_reference(event: &GitHubEvent) -> Option<(String, u64)> {
    if event.kind != "PushEvent" {
        return None;
    }
    let message = event
        .payload
        .pointer("/_activity_report_commit/message")
        .and_then(Value::as_str)?;
    let number = pull_number_from_commit_message(message)?;
    Some((event.repo.name.clone(), number))
}

fn pull_number_from_commit_message(message: &str) -> Option<u64> {
    let pattern = Regex::new(r"\(#([0-9]+)\)\s*$").expect("valid merge commit regex");
    let subject = message.lines().next()?;
    pattern.captures(subject)?[1].parse().ok()
}

fn related_issue_references(repository: &str, body: &str) -> BTreeSet<(String, u64)> {
    let mut references = BTreeSet::new();
    let issue_url =
        Regex::new(r"https?://github\.com/([A-Za-z0-9_.-]+)/([A-Za-z0-9_.-]+)/issues/([0-9]+)")
            .expect("valid issue URL regex");
    for captures in issue_url.captures_iter(body) {
        if let Ok(number) = captures[3].parse() {
            references.insert((format!("{}/{}", &captures[1], &captures[2]), number));
        }
    }

    let shorthand = Regex::new(r"([A-Za-z0-9_.-]+)/([A-Za-z0-9_.-]+)#([0-9]+)")
        .expect("valid issue shorthand regex");
    for captures in shorthand.captures_iter(body) {
        if let Ok(number) = captures[3].parse() {
            references.insert((format!("{}/{}", &captures[1], &captures[2]), number));
        }
    }

    let local = Regex::new(
        r"(?i)(?:close[sd]?|fix(?:e[sd])?|resolve[sd]?|relate(?:d)?\s+to|refs?)\s*:?[ \t]*#([0-9]+)",
    )
    .expect("valid local issue regex");
    for captures in local.captures_iter(body) {
        if let Ok(number) = captures[1].parse() {
            references.insert((repository.to_owned(), number));
        }
    }
    references
}

fn pull_request_reference(event: &GitHubEvent) -> Option<(String, u64)> {
    if !matches!(
        event.kind.as_str(),
        "PullRequestEvent"
            | "PullRequestReviewEvent"
            | "PullRequestReviewCommentEvent"
            | "IssueCommentEvent"
    ) {
        return None;
    }

    let number = event
        .payload
        .pointer("/pull_request/number")
        .and_then(Value::as_u64)
        .or_else(|| event.payload.get("number").and_then(Value::as_u64))
        .or_else(|| {
            (event.payload.pointer("/issue/pull_request").is_some())
                .then(|| {
                    event
                        .payload
                        .pointer("/issue/number")
                        .and_then(Value::as_u64)
                })
                .flatten()
        })
        .or_else(|| {
            [
                "/comment/html_url",
                "/review/html_url",
                "/pull_request/html_url",
            ]
            .iter()
            .find_map(|path| {
                event
                    .payload
                    .pointer(path)
                    .and_then(Value::as_str)
                    .and_then(pull_number_from_url)
            })
        })?;
    Some((event.repo.name.clone(), number))
}

fn pull_number_from_url(url: &str) -> Option<u64> {
    let (_, tail) = url.split_once("/pull/")?;
    tail.split(['/', '#'])
        .next()
        .and_then(|number| number.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::{
        Actor, GitHubEvent, Repository, append_missing_commit_events, matching_events,
        pull_number_from_commit_message, pull_number_from_url, related_issue_references,
        timeline_may_be_truncated,
    };
    use crate::timeframe::Timeframe;
    use chrono::{NaiveDate, TimeZone, Utc};
    use serde_json::{Value, json};

    fn event(id: &str, created_at: chrono::DateTime<Utc>) -> GitHubEvent {
        GitHubEvent {
            id: id.to_owned(),
            kind: "PushEvent".to_owned(),
            actor: Actor {
                login: "octocat".to_owned(),
            },
            repo: Repository {
                name: "acme/api".to_owned(),
            },
            payload: Value::Null,
            public: false,
            created_at,
        }
    }

    fn commit_event(id: &str, sha: &str) -> GitHubEvent {
        let mut event = event(id, Utc.with_ymd_and_hms(2026, 10, 28, 12, 0, 0).unwrap());
        event.payload = json!({
            "ref": "refs/heads/main",
            "head": sha,
            "commits": [{"sha": sha, "message": "Test commit"}],
        });
        event
    }

    fn yesterday() -> Timeframe {
        Timeframe {
            label: "yesterday".to_owned(),
            start: Utc.with_ymd_and_hms(2026, 10, 27, 23, 0, 0).unwrap(),
            end: Utc.with_ymd_and_hms(2026, 10, 28, 23, 0, 0).unwrap(),
            start_date: NaiveDate::from_ymd_opt(2026, 10, 28).unwrap(),
            end_date: NaiveDate::from_ymd_opt(2026, 10, 28).unwrap(),
        }
    }

    #[test]
    fn keeps_in_range_events_after_an_older_event() {
        let timeframe = yesterday();
        let events = vec![
            event(
                "older",
                Utc.with_ymd_and_hms(2026, 10, 27, 17, 52, 1).unwrap(),
            ),
            event(
                "in-range",
                Utc.with_ymd_and_hms(2026, 10, 28, 22, 20, 53).unwrap(),
            ),
        ];

        let matching = matching_events(events, "octocat", &timeframe, None);

        assert_eq!(matching.len(), 1);
        assert_eq!(matching[0].id, "in-range");
    }

    #[test]
    fn filters_events_by_repository_organization() {
        let timeframe = yesterday();
        let created_at = Utc.with_ymd_and_hms(2026, 10, 28, 12, 0, 0).unwrap();
        let mut other = event("other", created_at);
        other.repo.name = "other/api".to_owned();
        let events = vec![event("acme", created_at), other];

        let matching = matching_events(events, "octocat", &timeframe, Some("ACME"));

        assert_eq!(matching.len(), 1);
        assert_eq!(matching[0].id, "acme");
    }

    #[test]
    fn contribution_fallback_only_adds_missing_commit_shas() {
        let mut events = vec![commit_event("events-api", "existing")];
        let fallback = vec![
            commit_event("duplicate", "existing"),
            commit_event("missing", "missing"),
        ];

        append_missing_commit_events(&mut events, fallback);

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].id, "events-api");
        assert_eq!(events[1].id, "missing");
    }

    #[test]
    fn full_timeline_is_complete_when_it_reaches_before_the_timeframe() {
        let timeframe = yesterday();
        let oldest = Utc.with_ymd_and_hms(2026, 10, 20, 11, 35, 12).unwrap();

        assert!(!timeline_may_be_truncated(false, Some(oldest), &timeframe));
    }

    #[test]
    fn full_timeline_may_be_truncated_when_it_starts_inside_the_timeframe() {
        let timeframe = yesterday();
        let oldest = Utc.with_ymd_and_hms(2026, 10, 28, 12, 0, 0).unwrap();

        assert!(timeline_may_be_truncated(false, Some(oldest), &timeframe));
    }

    #[test]
    fn reads_pull_number_from_review_comment_url() {
        assert_eq!(
            pull_number_from_url("https://github.com/acme/api/pull/42#discussion_r123"),
            Some(42)
        );
    }

    #[test]
    fn extracts_local_and_cross_repository_related_issues() {
        let references = related_issue_references(
            "acme/api",
            "Fixes #12, related to acme/web#34 and https://github.com/acme/ops/issues/56",
        );
        assert!(references.contains(&("acme/api".to_owned(), 12)));
        assert!(references.contains(&("acme/web".to_owned(), 34)));
        assert!(references.contains(&("acme/ops".to_owned(), 56)));
    }

    #[test]
    fn reads_pull_number_from_squash_commit_subject() {
        assert_eq!(
            pull_number_from_commit_message(
                "ci: Add workflow fixes (#37)\n\nCo-authored-by: Example"
            ),
            Some(37)
        );
    }
}
