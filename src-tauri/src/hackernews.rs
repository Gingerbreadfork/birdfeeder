//! Finds Hacker News stories that link to the account's repositories.
//!
//! This is the one place the app talks to something other than GitHub, and only when asked to.

use anyhow::Result;
use chrono::{DateTime, SecondsFormat, Utc};
use reqwest::Client;
use serde::Deserialize;

const SEARCH: &str = "https://hn.algolia.com/api/v1/search_by_date";

#[derive(Debug, Clone)]
pub struct Story {
    pub id: String,
    pub title: String,
    /// The repo the story links to, as `owner/name` in the case the link used.
    pub repo: String,
    pub points: i64,
    pub comments: i64,
    pub posted_at: String,
}

#[derive(Deserialize)]
struct Found {
    hits: Vec<Hit>,
}

#[derive(Deserialize)]
struct Hit {
    #[serde(rename = "objectID")]
    id: String,
    title: Option<String>,
    url: Option<String>,
    points: Option<i64>,
    num_comments: Option<i64>,
    created_at: String,
}

/// The `owner/name` a GitHub link points at, if it points at one of `owner`'s repos.
fn linked_repo(url: &str, owner: &str) -> Option<String> {
    let path = url.split_once("github.com/")?.1;
    let mut parts = path.split(['/', '?', '#']);
    let (found_owner, name) = (parts.next()?, parts.next()?);
    (found_owner.eq_ignore_ascii_case(owner) && !name.is_empty())
        .then(|| format!("{found_owner}/{name}"))
}

/// Stories whose link goes to a repository owned by `owner`.
pub async fn stories(http: &Client, owner: &str) -> Result<Vec<Story>> {
    let url = format!(
        "{SEARCH}?query=github.com%2F{owner}%2F&restrictSearchableAttributes=url&tags=story&hitsPerPage=100"
    );
    let found: Found = http
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(found
        .hits
        .into_iter()
        .filter_map(|hit| {
            let repo = linked_repo(hit.url.as_deref()?, owner)?;
            let posted = DateTime::parse_from_rfc3339(&hit.created_at).ok()?;
            Some(Story {
                id: hit.id,
                title: hit.title.unwrap_or_default(),
                repo,
                points: hit.points.unwrap_or(0),
                comments: hit.num_comments.unwrap_or(0),
                posted_at: posted
                    .with_timezone(&Utc)
                    .to_rfc3339_opts(SecondsFormat::Secs, true),
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::linked_repo;

    #[test]
    fn links_are_matched_to_the_owner() {
        assert_eq!(
            linked_repo("https://github.com/Robin/seedcake", "robin").as_deref(),
            Some("Robin/seedcake")
        );
        assert_eq!(
            linked_repo(
                "https://github.com/robin/seedcake/releases/tag/v2#notes",
                "robin"
            )
            .as_deref(),
            Some("robin/seedcake")
        );
        assert_eq!(
            linked_repo("https://github.com/robinson/seedcake", "robin"),
            None
        );
        assert_eq!(linked_repo("https://github.com/robin", "robin"), None);
        assert_eq!(
            linked_repo("https://example.com/robin/seedcake", "robin"),
            None
        );
    }
}
