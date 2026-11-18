use std::collections::BTreeMap;
use std::fmt;
use std::sync::Mutex;

use reqwest::{Client, RequestBuilder, Response, StatusCode, header};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const API: &str = "https://api.github.com";
const USER_AGENT: &str = concat!("birdfeeder/", env!("CARGO_PKG_VERSION"));

const ACTIVITY_QUERY: &str = include_str!("queries/activity.graphql");
const RELEASES_QUERY: &str = include_str!("queries/releases.graphql");
const RELEASE_PAGE_QUERY: &str = include_str!("queries/release_page.graphql");
const STARGAZERS_QUERY: &str = include_str!("queries/stargazers.graphql");
const FOLLOWERS_QUERY: &str = include_str!("queries/followers.graphql");
const CONTRIBUTIONS_QUERY: &str = "{ viewer { contributionsCollection { contributionCalendar \
    { weeks { contributionDays { date contributionCount } } } } } }";

#[derive(Debug)]
pub enum ApiError {
    /// The token is missing, expired or revoked.
    Unauthorized,
    RateLimited {
        reset_at: Option<i64>,
    },
    /// The request never reached GitHub.
    Offline(String),
    Other(String),
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unauthorized => write!(f, "GitHub rejected the access token"),
            Self::RateLimited { .. } => write!(f, "GitHub API rate limit reached"),
            Self::Offline(detail) => write!(f, "Could not reach GitHub: {detail}"),
            Self::Other(detail) => write!(f, "{detail}"),
        }
    }
}

impl std::error::Error for ApiError {}

type ApiResult<T> = Result<T, ApiError>;

#[derive(Debug, Clone, Serialize)]
pub struct RateLimit {
    pub remaining: i64,
    pub limit: i64,
    pub reset_at: i64,
}

pub struct GitHub {
    http: Client,
    token: String,
    rates: Mutex<BTreeMap<String, RateLimit>>,
}

pub fn http_client() -> Client {
    let builder = Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(30))
        .connect_timeout(std::time::Duration::from_secs(10));
    // Android's own certificate store is only reachable through Java, so the
    // app carries Mozilla's list of root certificates instead.
    #[cfg(target_os = "android")]
    let builder = builder.tls_certs_only(
        webpki_root_certs::TLS_SERVER_ROOT_CERTS
            .iter()
            .filter_map(|der| reqwest::Certificate::from_der(der).ok()),
    );
    builder.build().expect("HTTP client")
}

/// An error and what lies beneath it, since "error sending request" alone says little.
fn with_cause(e: &dyn std::error::Error) -> String {
    let mut root = e;
    while let Some(next) = root.source() {
        root = next;
    }
    if std::ptr::addr_eq(root, e) {
        e.to_string()
    } else {
        format!("{e}: {root}")
    }
}

fn header_i64(resp: &Response, name: &str) -> Option<i64> {
    resp.headers().get(name)?.to_str().ok()?.parse().ok()
}

impl GitHub {
    pub fn new(http: Client, token: String) -> Self {
        Self {
            http,
            token,
            rates: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn http(&self) -> &Client {
        &self.http
    }

    /// Remaining request budget per API resource, as last reported by GitHub.
    pub fn rates(&self) -> BTreeMap<String, RateLimit> {
        self.rates.lock().unwrap().clone()
    }

    async fn send(&self, req: RequestBuilder) -> ApiResult<Response> {
        let resp = req
            .bearer_auth(&self.token)
            .header(header::ACCEPT, "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .await
            .map_err(|e| {
                let offline = e.is_connect() || e.is_timeout() || e.is_request();
                let detail = with_cause(&e.without_url());
                if offline {
                    ApiError::Offline(detail)
                } else {
                    ApiError::Other(detail)
                }
            })?;

        let remaining = header_i64(&resp, "x-ratelimit-remaining");
        let reset_at = header_i64(&resp, "x-ratelimit-reset");
        if let (Some(remaining), Some(limit), Some(reset_at)) =
            (remaining, header_i64(&resp, "x-ratelimit-limit"), reset_at)
        {
            let resource = resp
                .headers()
                .get("x-ratelimit-resource")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("core")
                .to_string();
            self.rates.lock().unwrap().insert(
                resource,
                RateLimit {
                    remaining,
                    limit,
                    reset_at,
                },
            );
        }

        let status = resp.status();
        if status.is_success() || status == StatusCode::NOT_MODIFIED {
            return Ok(resp);
        }
        let limited = remaining == Some(0) || resp.headers().contains_key(header::RETRY_AFTER);
        match status {
            StatusCode::UNAUTHORIZED => Err(ApiError::Unauthorized),
            StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS if limited => {
                Err(ApiError::RateLimited { reset_at })
            }
            _ => {
                let body = resp.text().await.unwrap_or_default();
                let message = serde_json::from_str::<Value>(&body)
                    .ok()
                    .and_then(|v| v.get("message")?.as_str().map(str::to_owned))
                    .unwrap_or_else(|| status.to_string());
                Err(ApiError::Other(format!(
                    "GitHub API error {}: {message}",
                    status.as_u16()
                )))
            }
        }
    }

    /// The address for a path, or for a full link GitHub's API handed back.
    /// Anything that is not on the API is refused, so the token is never sent elsewhere.
    fn url(path: &str) -> ApiResult<String> {
        if path.starts_with('/') {
            Ok(format!("{API}{path}"))
        } else if path.starts_with(&format!("{API}/")) {
            Ok(path.to_string())
        } else {
            Err(ApiError::Other(format!("Not a GitHub API address: {path}")))
        }
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> ApiResult<T> {
        let resp = self.send(self.http.get(Self::url(path)?)).await?;
        resp.json()
            .await
            .map_err(|e| ApiError::Other(e.to_string()))
    }

    async fn graphql<T: DeserializeOwned>(&self, query: &str, variables: Value) -> ApiResult<T> {
        #[derive(Deserialize)]
        struct Envelope<T> {
            data: Option<T>,
            errors: Option<Vec<GqlError>>,
        }
        #[derive(Deserialize)]
        struct GqlError {
            message: String,
            #[serde(rename = "type")]
            kind: Option<String>,
        }

        let req = self
            .http
            .post(format!("{API}/graphql"))
            .json(&json!({ "query": query, "variables": variables }));
        let resp = self.send(req).await?;
        let body: Envelope<T> = resp
            .json()
            .await
            .map_err(|e| ApiError::Other(e.to_string()))?;
        if let Some(errors) = &body.errors
            && errors
                .iter()
                .any(|e| e.kind.as_deref() == Some("RATE_LIMITED"))
        {
            let reset_at = self.rates().get("graphql").map(|r| r.reset_at);
            return Err(ApiError::RateLimited { reset_at });
        }
        // A partial result is still useful: a deleted repo only nulls its own node.
        match (body.data, body.errors) {
            (Some(data), _) => Ok(data),
            (None, Some(errors)) => Err(ApiError::Other(
                errors
                    .into_iter()
                    .map(|e| e.message)
                    .collect::<Vec<_>>()
                    .join("; "),
            )),
            (None, None) => Err(ApiError::Other("Empty GraphQL response".into())),
        }
    }

    pub async fn viewer(&self) -> ApiResult<User> {
        self.get("/user").await
    }

    /// Every repository the token's user owns, plus organisation repos when asked.
    pub async fn repos(&self, include_orgs: bool) -> ApiResult<Vec<Repo>> {
        let affiliation = if include_orgs {
            "owner,organization_member"
        } else {
            "owner"
        };
        let mut all = Vec::new();
        for page in 1..=50 {
            let batch: Vec<Repo> = self
                .get(&format!(
                    "/user/repos?affiliation={affiliation}&per_page=100&sort=full_name&page={page}"
                ))
                .await?;
            let done = batch.len() < 100;
            all.extend(batch);
            if done {
                break;
            }
        }
        Ok(all)
    }

    pub async fn activity(&self, ids: &[String], star_page: u32) -> ApiResult<Vec<Activity>> {
        #[derive(Deserialize)]
        struct Data {
            nodes: Vec<Option<Activity>>,
        }
        let data: Data = self
            .graphql(ACTIVITY_QUERY, json!({ "ids": ids, "starPage": star_page }))
            .await?;
        Ok(data.nodes.into_iter().flatten().collect())
    }

    pub async fn releases(&self, ids: &[String]) -> ApiResult<Vec<RepoReleases>> {
        #[derive(Deserialize)]
        struct Data {
            nodes: Vec<Option<RepoReleases>>,
        }
        let data: Data = self.graphql(RELEASES_QUERY, json!({ "ids": ids })).await?;
        Ok(data.nodes.into_iter().flatten().collect())
    }

    pub async fn release_page(&self, id: &str, cursor: &str) -> ApiResult<Option<ReleaseList>> {
        #[derive(Deserialize)]
        struct Data {
            node: Option<RepoReleases>,
        }
        let data: Data = self
            .graphql(RELEASE_PAGE_QUERY, json!({ "id": id, "cursor": cursor }))
            .await?;
        Ok(data.node.map(|n| n.releases))
    }

    pub async fn stargazer_page(&self, id: &str, cursor: &str) -> ApiResult<Option<Stargazers>> {
        #[derive(Deserialize)]
        struct Node {
            stargazers: Stargazers,
        }
        #[derive(Deserialize)]
        struct Data {
            node: Option<Node>,
        }
        let data: Data = self
            .graphql(STARGAZERS_QUERY, json!({ "id": id, "cursor": cursor }))
            .await?;
        Ok(data.node.map(|n| n.stargazers))
    }

    /// The follower list, and whether it is complete or was cut off at `max_pages`.
    pub async fn followers(&self, max_pages: usize) -> ApiResult<(Vec<Actor>, bool)> {
        #[derive(Deserialize)]
        struct Data {
            viewer: Viewer,
        }
        #[derive(Deserialize)]
        struct Viewer {
            followers: Page,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Page {
            page_info: PageInfo,
            nodes: Vec<Actor>,
        }

        let mut all = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..max_pages {
            let data: Data = self
                .graphql(FOLLOWERS_QUERY, json!({ "cursor": cursor }))
                .await?;
            let page = data.viewer.followers;
            all.extend(page.nodes);
            if !page.page_info.has_next_page {
                return Ok((all, true));
            }
            cursor = page.page_info.end_cursor;
        }
        Ok((all, false))
    }

    /// Public profiles for up to a few dozen logins. Logins that are not people are left out.
    pub async fn people(&self, logins: &[String]) -> ApiResult<Vec<Person>> {
        let declared: Vec<String> = (0..logins.len())
            .map(|i| format!("$l{i}: String!"))
            .collect();
        let asked: Vec<String> = (0..logins.len())
            .map(|i| {
                format!(
                    "u{i}: user(login: $l{i}) {{ login name company followers {{ totalCount }} }}"
                )
            })
            .collect();
        let query = format!("query({}) {{ {} }}", declared.join(", "), asked.join(" "));
        let variables: serde_json::Map<String, Value> = logins
            .iter()
            .enumerate()
            .map(|(i, login)| (format!("l{i}"), json!(login)))
            .collect();
        let found: BTreeMap<String, Option<Person>> =
            self.graphql(&query, Value::Object(variables)).await?;
        Ok(found.into_values().flatten().collect())
    }

    /// The account's own contributions for each day of the last year, oldest first.
    pub async fn contributions(&self) -> ApiResult<Vec<(String, i64)>> {
        #[derive(Deserialize)]
        struct Data {
            viewer: Viewer,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Viewer {
            contributions_collection: Collection,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Collection {
            contribution_calendar: Calendar,
        }
        #[derive(Deserialize)]
        struct Calendar {
            weeks: Vec<Week>,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Week {
            contribution_days: Vec<Day>,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Day {
            date: String,
            contribution_count: i64,
        }

        let data: Data = self.graphql(CONTRIBUTIONS_QUERY, json!({})).await?;
        Ok(data
            .viewer
            .contributions_collection
            .contribution_calendar
            .weeks
            .into_iter()
            .flat_map(|week| week.contribution_days)
            .map(|day| (day.date, day.contribution_count))
            .collect())
    }

    /// Unread notification threads. `None` when nothing changed since `last_modified`.
    pub async fn inbox(
        &self,
        last_modified: Option<&str>,
    ) -> ApiResult<Option<(Vec<Thread>, Option<String>)>> {
        let mut req = self.http.get(format!("{API}/notifications?per_page=50"));
        if let Some(stamp) = last_modified {
            req = req.header(header::IF_MODIFIED_SINCE, stamp);
        }
        let resp = self.send(req).await?;
        if resp.status() == StatusCode::NOT_MODIFIED {
            return Ok(None);
        }
        let stamp = resp
            .headers()
            .get(header::LAST_MODIFIED)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let threads = resp
            .json()
            .await
            .map_err(|e| ApiError::Other(e.to_string()))?;
        Ok(Some((threads, stamp)))
    }

    pub async fn traffic(&self, repo: &str) -> ApiResult<Traffic> {
        let views_url = format!("/repos/{repo}/traffic/views");
        let clones_url = format!("/repos/{repo}/traffic/clones");
        let referrers_url = format!("/repos/{repo}/traffic/popular/referrers");
        let (views, clones, referrers) = tokio::join!(
            self.get::<TrafficSeries>(&views_url),
            self.get::<TrafficSeries>(&clones_url),
            self.get::<Vec<Referrer>>(&referrers_url),
        );
        Ok(Traffic {
            views: views?.days(),
            clones: clones?.days(),
            referrers: referrers?,
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct User {
    /// GitHub's number for the account, which survives a change of login.
    #[serde(default)]
    pub id: i64,
    pub login: String,
    pub name: Option<String>,
    pub avatar_url: String,
    pub html_url: String,
    pub followers: i64,
    pub following: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Repo {
    pub node_id: String,
    pub full_name: String,
    pub html_url: String,
    pub description: Option<String>,
    pub private: bool,
    pub fork: bool,
    pub archived: bool,
    pub language: Option<String>,
    pub stargazers_count: i64,
    pub forks_count: i64,
    pub open_issues_count: i64,
    pub pushed_at: Option<String>,
    pub updated_at: Option<String>,
    pub created_at: Option<String>,
    #[serde(default)]
    pub permissions: Permissions,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Permissions {
    #[serde(default)]
    pub push: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageInfo {
    pub has_next_page: bool,
    pub end_cursor: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Count {
    #[serde(rename = "totalCount")]
    pub total: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Actor {
    pub login: String,
    pub avatar_url: Option<String>,
    #[serde(rename = "__typename")]
    pub typename: Option<String>,
}

impl Actor {
    pub fn is_bot(&self) -> bool {
        self.typename.as_deref() == Some("Bot") || self.login.ends_with("[bot]")
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Activity {
    pub id: String,
    /// The star total from the same answer as the stargazers listed.
    #[serde(default)]
    pub stargazer_count: Option<i64>,
    pub watchers: Count,
    pub open_issues: Count,
    pub open_pull_requests: Count,
    pub releases: Count,
    pub stargazers: Stargazers,
    pub issues: Nodes<Ticket>,
    pub pull_requests: Nodes<Ticket>,
    pub forks: Nodes<Fork>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Nodes<T> {
    pub nodes: Vec<T>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stargazers {
    pub page_info: PageInfo,
    pub edges: Vec<StarEdge>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StarEdge {
    pub starred_at: String,
    pub node: Actor,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Person {
    pub login: String,
    pub name: Option<String>,
    pub company: Option<String>,
    pub followers: Count,
}

/// An issue or a pull request.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ticket {
    pub number: i64,
    pub title: String,
    pub url: String,
    pub created_at: String,
    pub author: Option<Actor>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fork {
    pub name_with_owner: String,
    pub url: String,
    pub created_at: String,
    pub owner: Actor,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RepoReleases {
    pub id: String,
    pub releases: ReleaseList,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseList {
    pub page_info: PageInfo,
    pub nodes: Vec<Release>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    pub tag_name: String,
    pub name: Option<String>,
    pub url: String,
    pub is_draft: bool,
    pub is_prerelease: bool,
    pub published_at: Option<String>,
    pub author: Option<Actor>,
    pub release_assets: Nodes<Asset>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Asset {
    pub name: String,
    #[serde(alias = "downloadCount")]
    pub downloads: i64,
    pub size: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Thread {
    pub id: String,
    pub reason: String,
    pub updated_at: String,
    pub subject: ThreadSubject,
    pub repository: ThreadRepo,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ThreadSubject {
    pub title: String,
    pub url: Option<String>,
    pub latest_comment_url: Option<String>,
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ThreadRepo {
    pub node_id: String,
    pub full_name: String,
    pub html_url: String,
}

/// The comment, issue or release a notification thread points at.
#[derive(Debug, Clone, Deserialize)]
pub struct ThreadTarget {
    pub html_url: Option<String>,
    pub body: Option<String>,
    pub user: Option<RestActor>,
    pub author: Option<RestActor>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RestActor {
    pub login: String,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TrafficSeries {
    #[serde(default)]
    views: Vec<TrafficPoint>,
    #[serde(default)]
    clones: Vec<TrafficPoint>,
}

impl TrafficSeries {
    fn days(self) -> Vec<TrafficPoint> {
        if self.views.is_empty() {
            self.clones
        } else {
            self.views
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct TrafficPoint {
    pub timestamp: String,
    pub count: i64,
    pub uniques: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Referrer {
    pub referrer: String,
    pub count: i64,
    pub uniques: i64,
}

#[derive(Debug, Clone)]
pub struct Traffic {
    pub views: Vec<TrafficPoint>,
    pub clones: Vec<TrafficPoint>,
    pub referrers: Vec<Referrer>,
}
