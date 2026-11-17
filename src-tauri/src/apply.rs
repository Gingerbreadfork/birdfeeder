//! Turns freshly fetched GitHub data into stored state and events.
//!
//! Nothing here touches the network, so every rule can be tested against an
//! in-memory database.

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use serde_json::json;

use crate::db::{self, NewEvent, StoredRepo};
use crate::github::{Activity, Actor, Asset, Release, Repo, Thread, ThreadTarget, Ticket, Traffic};

pub const STAR_MILESTONES: &[i64] = &[
    1, 10, 25, 50, 100, 250, 500, 1_000, 2_500, 5_000, 10_000, 25_000, 50_000, 100_000,
];
pub const DOWNLOAD_MILESTONES: &[i64] = &[
    100, 500, 1_000, 5_000, 10_000, 50_000, 100_000, 500_000, 1_000_000,
];
pub const FOLLOWER_MILESTONES: &[i64] = &[10, 25, 50, 100, 250, 500, 1_000, 5_000, 10_000];

/// A burst of views counts as a spike at this many views and this multiple of the usual day.
const SPIKE_MIN_VIEWS: i64 = 25;
const SPIKE_FACTOR: f64 = 3.0;

pub struct Ctx<'a> {
    /// The sync's timestamp, RFC 3339 in UTC.
    pub now: &'a str,
    /// The local calendar day.
    pub today: &'a str,
    pub viewer: &'a str,
    pub ignore_assets: &'a [String],
}

impl Ctx<'_> {
    fn is_viewer(&self, login: &str) -> bool {
        login.eq_ignore_ascii_case(self.viewer)
    }

    /// The repo name without the owner when the viewer owns it.
    fn short<'r>(&self, repo: &'r str) -> &'r str {
        match repo.split_once('/') {
            Some((owner, name)) if self.is_viewer(owner) => name,
            _ => repo,
        }
    }
}

/// Text for one desktop notification.
#[derive(Debug, Clone, Serialize)]
pub struct Notice {
    pub kind: &'static str,
    pub title: String,
    pub body: String,
    /// The person the notice is about, so their profile can be added to it.
    #[serde(skip)]
    pub actor: Option<String>,
}

pub fn plural(n: i64, one: &str, many: &str) -> String {
    format!("{} {}", group_digits(n), if n == 1 { one } else { many })
}

pub fn group_digits(n: i64) -> String {
    let digits = n.abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

/// Matches a file name against a pattern where `*` stands for any run of characters.
pub fn glob_match(pattern: &str, name: &str) -> bool {
    let pattern = pattern.trim().to_lowercase();
    let name = name.to_lowercase();
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == name;
    }
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    let Some(mut rest) = name
        .strip_prefix(first)
        .and_then(|rest| rest.strip_suffix(last))
    else {
        return false;
    };
    for part in &parts[1..parts.len() - 1] {
        match rest.find(part) {
            Some(at) => rest = &rest[at + part.len()..],
            None => return false,
        }
    }
    true
}

/// Downloads of the assets that are not on the ignore list.
pub fn counted_downloads(assets: &[Asset], ignore: &[String]) -> i64 {
    assets
        .iter()
        .filter(|a| !ignore.iter().any(|p| glob_match(p, &a.name)))
        .map(|a| a.downloads)
        .sum()
}

pub fn crossed(milestones: &[i64], before: i64, after: i64) -> Option<i64> {
    milestones
        .iter()
        .copied()
        .rfind(|&m| before < m && m <= after)
}

/// Marks every repo as gone. Each repo still listed is revived by `repo`.
pub fn begin(conn: &Connection) -> Result<()> {
    conn.execute("UPDATE repos SET gone = 1", [])?;
    Ok(())
}

/// Whether the repo's stars, issues, pull requests and forks must be re-read.
pub fn needs_activity(stored: Option<&StoredRepo>, repo: &Repo) -> bool {
    let Some(s) = stored else { return true };
    s.seeded_at.is_none()
        || s.gone
        || s.stars != repo.stargazers_count
        || s.forks != repo.forks_count
        || s.open_total != repo.open_issues_count
        || s.pushed_at != repo.pushed_at
        || s.updated_at != repo.updated_at
}

/// Releases read for one repo. `complete` means no release is missing from `nodes`.
pub struct ReleaseSet {
    pub nodes: Vec<Release>,
    pub complete: bool,
    /// The list was paged past the first page on this sync.
    pub paged: bool,
}

/// What one sync learned about a repo.
pub struct Fetched<'a> {
    pub repo: &'a Repo,
    pub stored: Option<&'a StoredRepo>,
    /// The repo's counts changed, so its activity was asked for.
    pub flagged: bool,
    pub activity: Option<&'a Activity>,
    pub releases: Option<&'a ReleaseSet>,
}

pub fn repo(
    conn: &Connection,
    ctx: &Ctx,
    fetched: &Fetched,
    notices: &mut Vec<Notice>,
) -> Result<()> {
    let Fetched {
        repo,
        stored,
        flagged,
        activity,
        releases,
    } = *fetched;
    let id = &repo.node_id;
    // A repo that was away comes back as if seen for the first time, so nothing
    // that happened in between is reported as new.
    let returning = stored.is_some_and(|s| s.gone);
    if flagged && activity.is_none() {
        // Leave the stored counts alone so the change is picked up again next time.
        if !returning {
            conn.execute("UPDATE repos SET gone = 0 WHERE id = ?1", [id])?;
        }
        return Ok(());
    }
    // The count that came with the stargazers' names, so the two cannot disagree.
    let star_count = activity
        .and_then(|a| a.stargazer_count)
        .unwrap_or(repo.stargazers_count);

    conn.execute(
        "INSERT INTO repos (id, name, url, description, is_private, is_fork, is_archived, can_push,
                            language, stars, forks, open_total, pushed_at, updated_at, created_at, gone)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, 0)
         ON CONFLICT (id) DO UPDATE SET
           name = excluded.name, url = excluded.url, description = excluded.description,
           is_private = excluded.is_private, is_fork = excluded.is_fork,
           is_archived = excluded.is_archived, can_push = excluded.can_push,
           language = excluded.language, stars = excluded.stars, forks = excluded.forks,
           open_total = excluded.open_total, pushed_at = excluded.pushed_at,
           updated_at = excluded.updated_at, created_at = excluded.created_at, gone = 0",
        params![
            id,
            repo.full_name,
            repo.html_url,
            repo.description,
            repo.private,
            repo.fork,
            repo.archived,
            repo.permissions.push,
            repo.language,
            star_count,
            repo.forks_count,
            repo.open_issues_count,
            repo.pushed_at,
            repo.updated_at,
            repo.created_at,
        ],
    )?;

    let seeded = stored
        .filter(|_| !returning)
        .and_then(|s| s.seeded_at.as_deref());
    let name = ctx.short(&repo.full_name);

    if let Some(act) = activity {
        stars(conn, ctx, repo, stored, seeded, star_count, act, notices)?;
        for ticket in &act.issues.nodes {
            self::ticket(conn, ctx, repo, seeded, "issue", ticket, notices)?;
        }
        for ticket in &act.pull_requests.nodes {
            self::ticket(conn, ctx, repo, seeded, "pr", ticket, notices)?;
        }
        for fork in &act.forks.nodes {
            let fresh = seeded.is_some_and(|s| fork.created_at.as_str() > s);
            let quiet = !fresh || ctx.is_viewer(&fork.owner.login);
            let inserted = db::insert_event(
                conn,
                &NewEvent {
                    key: format!("fork:{id}:{}", fork.name_with_owner),
                    kind: "fork",
                    repo_id: Some(id.clone()),
                    repo: Some(repo.full_name.clone()),
                    actor: Some(fork.owner.login.clone()),
                    actor_avatar: fork.owner.avatar_url.clone(),
                    subject: Some(fork.name_with_owner.clone()),
                    url: Some(fork.url.clone()),
                    occurred_at: fork.created_at.clone(),
                    read: quiet,
                    count: 1,
                    ..Default::default()
                },
            )?;
            if inserted && !quiet {
                notices.push(Notice {
                    kind: "fork",
                    title: format!("{} forked {name}", fork.owner.login),
                    body: plural(repo.forks_count, "fork", "forks"),
                    actor: Some(fork.owner.login.clone()),
                });
            }
        }

        // The cursor is kept only on first sight, to page in the older stargazers.
        let star_cursor = (seeded.is_none() && act.stargazers.page_info.has_next_page)
            .then(|| act.stargazers.page_info.end_cursor.clone())
            .flatten();
        conn.execute(
            "UPDATE repos SET watchers = ?1, open_issues = ?2, open_prs = ?3, release_count = ?4,
                              seeded_at = CASE WHEN ?8 THEN ?5 ELSE COALESCE(seeded_at, ?5) END,
                              star_cursor = COALESCE(?6, star_cursor)
             WHERE id = ?7",
            params![
                act.watchers.total,
                act.open_issues.total,
                act.open_pull_requests.total,
                act.releases.total,
                ctx.now,
                star_cursor,
                id,
                returning
            ],
        )?;
    }

    if let (Some(old), Some(_)) = (stored, seeded)
        && let Some(mark) = crossed(STAR_MILESTONES, old.stars, star_count)
    {
        milestone(conn, ctx, Some(repo), "stars", mark, notices)?;
    }

    if let Some(set) = releases {
        let before = stored.map_or(0, |s| s.downloads);
        self::releases(conn, ctx, repo, seeded, before, set, notices)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn stars(
    conn: &Connection,
    ctx: &Ctx,
    repo: &Repo,
    stored: Option<&StoredRepo>,
    seeded: Option<&str>,
    star_count: i64,
    act: &Activity,
    notices: &mut Vec<Notice>,
) -> Result<()> {
    let id = &repo.node_id;
    let name = ctx.short(&repo.full_name);
    let mut gained = 0;
    for edge in &act.stargazers.edges {
        let login = &edge.node.login;
        let fresh = seeded.is_some_and(|s| edge.starred_at.as_str() > s);
        let quiet = !fresh || ctx.is_viewer(login);
        let inserted =
            db::insert_event(conn, &star_event(repo, &edge.node, &edge.starred_at, quiet))?;
        if inserted && fresh {
            gained += 1;
            if !quiet {
                notices.push(Notice {
                    kind: "star",
                    title: format!("{login} starred {name}"),
                    body: plural(star_count, "star", "stars"),
                    actor: Some(login.clone()),
                });
            }
        }
    }

    let (Some(old), Some(_)) = (stored, seeded) else {
        return Ok(());
    };
    let lost = gained - (star_count - old.stars);
    if lost > 0 {
        db::bump_event(
            conn,
            &NewEvent {
                key: format!("unstar:{id}:{}", ctx.today),
                kind: "unstar",
                repo_id: Some(id.clone()),
                repo: Some(repo.full_name.clone()),
                count: lost,
                url: Some(format!("{}/stargazers", repo.html_url)),
                occurred_at: ctx.now.to_string(),
                read: true,
                ..Default::default()
            },
        )?;
    } else if lost < 0 {
        // More stars arrived than the fetched page could name.
        db::insert_event(
            conn,
            &NewEvent {
                key: format!("stars:{id}:{}", ctx.now),
                kind: "star",
                repo_id: Some(id.clone()),
                repo: Some(repo.full_name.clone()),
                count: -lost,
                url: Some(format!("{}/stargazers", repo.html_url)),
                occurred_at: ctx.now.to_string(),
                ..Default::default()
            },
        )?;
        notices.push(Notice {
            kind: "star",
            title: format!("{name} gained {}", plural(-lost, "more star", "more stars")),
            body: plural(star_count, "star", "stars"),
            actor: None,
        });
    }
    Ok(())
}

pub fn star_event(repo: &Repo, who: &Actor, starred_at: &str, read: bool) -> NewEvent {
    star_event_for(&repo.node_id, &repo.full_name, who, starred_at, read)
}

pub fn star_event_for(
    repo_id: &str,
    repo_name: &str,
    who: &Actor,
    starred_at: &str,
    read: bool,
) -> NewEvent {
    NewEvent {
        key: format!("star:{repo_id}:{}:{starred_at}", who.login),
        kind: "star",
        repo_id: Some(repo_id.to_string()),
        repo: Some(repo_name.to_string()),
        actor: Some(who.login.clone()),
        actor_avatar: who.avatar_url.clone(),
        url: Some(format!("https://github.com/{}", who.login)),
        occurred_at: starred_at.to_string(),
        read,
        count: 1,
        ..Default::default()
    }
}

fn ticket(
    conn: &Connection,
    ctx: &Ctx,
    repo: &Repo,
    seeded: Option<&str>,
    kind: &'static str,
    ticket: &Ticket,
    notices: &mut Vec<Notice>,
) -> Result<()> {
    let author = ticket.author.as_ref();
    let fresh = seeded.is_some_and(|s| ticket.created_at.as_str() > s);
    let quiet = !fresh || author.is_some_and(|a| ctx.is_viewer(&a.login) || a.is_bot());
    let inserted = db::insert_event(
        conn,
        &NewEvent {
            key: format!("{kind}:{}:{}", repo.node_id, ticket.number),
            kind,
            repo_id: Some(repo.node_id.clone()),
            repo: Some(repo.full_name.clone()),
            actor: author.map(|a| a.login.clone()),
            actor_avatar: author.and_then(|a| a.avatar_url.clone()),
            subject: Some(ticket.title.clone()),
            number: Some(ticket.number),
            url: Some(ticket.url.clone()),
            meta: author
                .filter(|a| a.is_bot())
                .map(|_| json!({ "bot": true })),
            occurred_at: ticket.created_at.clone(),
            read: quiet,
            count: 1,
        },
    )?;
    if inserted && !quiet {
        let what = if kind == "pr" {
            "pull request"
        } else {
            "issue"
        };
        let who = author.map_or("Someone", |a| a.login.as_str());
        notices.push(Notice {
            kind,
            title: format!("New {what} in {}", ctx.short(&repo.full_name)),
            body: format!("{who} opened #{}: {}", ticket.number, ticket.title),
            actor: None,
        });
    }
    Ok(())
}

fn milestone(
    conn: &Connection,
    ctx: &Ctx,
    repo: Option<&Repo>,
    what: &'static str,
    mark: i64,
    notices: &mut Vec<Notice>,
) -> Result<()> {
    let scope = repo.map_or("account", |r| r.node_id.as_str());
    let inserted = db::insert_event(
        conn,
        &NewEvent {
            key: format!("milestone:{what}:{scope}:{mark}"),
            kind: "milestone",
            repo_id: repo.map(|r| r.node_id.clone()),
            repo: repo.map(|r| r.full_name.clone()),
            subject: Some(what.to_string()),
            count: mark,
            url: repo.map(|r| r.html_url.clone()),
            occurred_at: ctx.now.to_string(),
            ..Default::default()
        },
    )?;
    if inserted {
        let reached = match what {
            "stars" => plural(mark, "star", "stars"),
            "all-stars" => format!("{} across your repositories", plural(mark, "star", "stars")),
            "downloads" => plural(mark, "download", "downloads"),
            _ => plural(mark, "follower", "followers"),
        };
        notices.push(Notice {
            kind: "milestone",
            title: match repo {
                Some(r) => format!("{} reached {reached}", ctx.short(&r.full_name)),
                None => format!("You reached {reached}"),
            },
            body: String::new(),
            actor: None,
        });
    }
    Ok(())
}

fn releases(
    conn: &Connection,
    ctx: &Ctx,
    repo: &Repo,
    seeded: Option<&str>,
    downloads_before: i64,
    set: &ReleaseSet,
    notices: &mut Vec<Notice>,
) -> Result<()> {
    let id = &repo.node_id;
    let name = ctx.short(&repo.full_name);
    let mut gains: Vec<(String, i64)> = Vec::new();
    let mut tags: HashSet<&str> = HashSet::new();

    for release in &set.nodes {
        let Some(published_at) = release
            .published_at
            .as_deref()
            .filter(|_| !release.is_draft)
        else {
            continue;
        };
        tags.insert(&release.tag_name);
        let assets = &release.release_assets.nodes;
        let counted = counted_downloads(assets, ctx.ignore_assets);
        let known: Option<String> = conn
            .query_row(
                "SELECT assets FROM releases WHERE repo_id = ?1 AND tag = ?2",
                params![id, release.tag_name],
                |r| r.get(0),
            )
            .optional()?;

        let gain = match known {
            Some(raw) => {
                let old: Vec<Asset> = serde_json::from_str(&raw).unwrap_or_default();
                counted - counted_downloads(&old, ctx.ignore_assets)
            }
            None => {
                let author = release.author.as_ref();
                let fresh = seeded.is_some_and(|s| published_at > s);
                let quiet = !fresh || author.is_some_and(|a| ctx.is_viewer(&a.login) || a.is_bot());
                let inserted = db::insert_event(
                    conn,
                    &NewEvent {
                        key: format!("release:{id}:{}", release.tag_name),
                        kind: "release",
                        repo_id: Some(id.clone()),
                        repo: Some(repo.full_name.clone()),
                        actor: author.map(|a| a.login.clone()),
                        actor_avatar: author.and_then(|a| a.avatar_url.clone()),
                        subject: Some(release.tag_name.clone()),
                        url: Some(release.url.clone()),
                        meta: Some(json!({
                            "name": release.name,
                            "prerelease": release.is_prerelease,
                        })),
                        occurred_at: published_at.to_string(),
                        read: quiet,
                        count: 1,
                        ..Default::default()
                    },
                )?;
                if inserted && !quiet {
                    notices.push(Notice {
                        kind: "release",
                        title: format!("{name} {} published", release.tag_name),
                        body: release.name.clone().unwrap_or_default(),
                        actor: None,
                    });
                }
                if fresh { counted } else { 0 }
            }
        };
        if gain > 0 {
            gains.push((release.tag_name.clone(), gain));
        }

        conn.execute(
            "INSERT INTO releases (repo_id, tag, name, url, published_at, is_prerelease, downloads, assets)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT (repo_id, tag) DO UPDATE SET
               name = excluded.name, url = excluded.url, published_at = excluded.published_at,
               is_prerelease = excluded.is_prerelease, downloads = excluded.downloads,
               assets = excluded.assets",
            params![
                id,
                release.tag_name,
                release.name,
                release.url,
                published_at,
                release.is_prerelease,
                counted,
                serde_json::to_string(assets)?,
            ],
        )?;
    }

    if set.complete {
        let mut stmt = conn.prepare("SELECT tag FROM releases WHERE repo_id = ?1")?;
        let stored: Vec<String> = stmt
            .query_map([id], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        for tag in stored.iter().filter(|t| !tags.contains(t.as_str())) {
            conn.execute(
                "DELETE FROM releases WHERE repo_id = ?1 AND tag = ?2",
                params![id, tag],
            )?;
        }
    }

    let total: i64 = conn.query_row(
        "SELECT COALESCE(SUM(downloads), 0) FROM releases WHERE repo_id = ?1",
        [id],
        |r| r.get(0),
    )?;
    conn.execute(
        "UPDATE repos SET downloads = ?1,
                          releases_full_at = CASE WHEN ?2 THEN ?3 ELSE releases_full_at END
         WHERE id = ?4",
        params![total, set.paged || set.complete, ctx.now, id],
    )?;

    let gained: i64 = gains.iter().map(|(_, n)| n).sum();
    if gained > 0 {
        let per_release: serde_json::Map<String, serde_json::Value> = gains
            .iter()
            .map(|(tag, n)| (tag.clone(), json!(n)))
            .collect();
        db::bump_event(
            conn,
            &NewEvent {
                key: format!("downloads:{id}:{}", ctx.today),
                kind: "downloads",
                repo_id: Some(id.clone()),
                repo: Some(repo.full_name.clone()),
                count: gained,
                url: Some(format!("{}/releases", repo.html_url)),
                meta: Some(json!({ "releases": per_release })),
                occurred_at: ctx.now.to_string(),
                ..Default::default()
            },
        )?;
        gains.sort_by_key(|(_, n)| -n);
        notices.push(Notice {
            kind: "downloads",
            title: format!(
                "{name}: {}",
                plural(gained, "new download", "new downloads")
            ),
            body: gains
                .iter()
                .take(4)
                .map(|(tag, n)| format!("{tag} +{n}"))
                .collect::<Vec<_>>()
                .join(", "),
            actor: None,
        });
    }

    if seeded.is_some()
        && let Some(mark) = crossed(DOWNLOAD_MILESTONES, downloads_before, total)
    {
        milestone(conn, ctx, Some(repo), "downloads", mark, notices)?;
    }
    Ok(())
}

/// Recounts stored releases after the ignore list changed.
pub fn recount_downloads(conn: &Connection, ignore: &[String]) -> Result<()> {
    let mut stmt = conn.prepare("SELECT repo_id, tag, assets FROM releases")?;
    let rows: Vec<(String, String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    for (repo_id, tag, raw) in rows {
        let assets: Vec<Asset> = serde_json::from_str(&raw).unwrap_or_default();
        conn.execute(
            "UPDATE releases SET downloads = ?1 WHERE repo_id = ?2 AND tag = ?3",
            params![counted_downloads(&assets, ignore), repo_id, tag],
        )?;
    }
    conn.execute(
        "UPDATE repos SET downloads =
           (SELECT COALESCE(SUM(downloads), 0) FROM releases WHERE repo_id = repos.id)",
        [],
    )?;
    Ok(())
}

/// Records today's totals so growth can be charted later.
///
/// `stored` is how the repos stood before this sync. The account's star total is
/// compared over the repos that were already watched then, so one that joins
/// with stars of its own does not look like a milestone.
pub fn finish(
    conn: &Connection,
    ctx: &Ctx,
    stored: &HashMap<String, StoredRepo>,
    followers: i64,
    notices: &mut Vec<Notice>,
) -> Result<()> {
    let (mut before, mut after) = (0, 0);
    {
        let mut stmt = conn.prepare("SELECT id, stars FROM repos WHERE gone = 0")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        for row in rows {
            let (id, stars) = row?;
            if let Some(old) = stored.get(&id).filter(|s| !s.gone && s.seeded_at.is_some()) {
                before += old.stars;
                after += stars;
            }
        }
    }
    if let Some(mark) = crossed(STAR_MILESTONES, before, after) {
        // The total passes a mark no later than any one repo does, so a repo that has
        // already been congratulated for it makes this old news.
        let said: bool = conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM events
               WHERE kind = 'milestone' AND subject = 'stars' AND count = ?1)",
            [mark],
            |r| r.get(0),
        )?;
        if !said {
            milestone(conn, ctx, None, "all-stars", mark, notices)?;
        }
    }

    conn.execute(
        "INSERT INTO repo_daily (repo_id, date, stars, forks, open_issues, open_prs, downloads)
         SELECT id, ?1, stars, forks, open_issues, open_prs, downloads FROM repos WHERE gone = 0
         ON CONFLICT (repo_id, date) DO UPDATE SET
           stars = excluded.stars, forks = excluded.forks, open_issues = excluded.open_issues,
           open_prs = excluded.open_prs, downloads = excluded.downloads",
        [ctx.today],
    )?;
    conn.execute(
        "INSERT INTO account_daily (date, followers, stars, forks, downloads)
         SELECT ?1, ?2, COALESCE(SUM(stars), 0), COALESCE(SUM(forks), 0), COALESCE(SUM(downloads), 0)
         FROM repos WHERE gone = 0
         ON CONFLICT (date) DO UPDATE SET
           followers = excluded.followers, stars = excluded.stars,
           forks = excluded.forks, downloads = excluded.downloads",
        params![ctx.today, followers],
    )?;
    if db::meta_get(conn, "tracking_since")?.is_none() {
        db::meta_set(conn, "tracking_since", ctx.now)?;
    }
    Ok(())
}

/// Compares the follower list with the stored one. `complete` allows spotting unfollows.
pub fn followers(
    conn: &Connection,
    ctx: &Ctx,
    list: &[Actor],
    complete: bool,
    total: i64,
    notices: &mut Vec<Notice>,
) -> Result<()> {
    let seeded = db::meta_get(conn, "followers_seeded_at")?.is_some();
    let before: Option<i64> = db::meta_get(conn, "followers_count")?.and_then(|v| v.parse().ok());
    let mut stmt = conn.prepare("SELECT login FROM followers")?;
    let stored: HashSet<String> = stmt
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let current: HashSet<&str> = list.iter().map(|f| f.login.as_str()).collect();

    for f in list.iter().filter(|f| !stored.contains(&f.login)) {
        conn.execute(
            "INSERT OR REPLACE INTO followers (login, avatar_url) VALUES (?1, ?2)",
            params![f.login, f.avatar_url],
        )?;
        if !seeded {
            continue;
        }
        db::insert_event(
            conn,
            &NewEvent {
                key: format!("follower:{}:{}", f.login, ctx.now),
                kind: "follower",
                actor: Some(f.login.clone()),
                actor_avatar: f.avatar_url.clone(),
                url: Some(format!("https://github.com/{}", f.login)),
                occurred_at: ctx.now.to_string(),
                count: 1,
                ..Default::default()
            },
        )?;
        notices.push(Notice {
            kind: "follower",
            title: format!("{} followed you", f.login),
            body: plural(total, "follower", "followers"),
            actor: Some(f.login.clone()),
        });
    }

    if complete {
        for login in stored.iter().filter(|l| !current.contains(l.as_str())) {
            conn.execute("DELETE FROM followers WHERE login = ?1", [login])?;
            if seeded {
                db::insert_event(
                    conn,
                    &NewEvent {
                        key: format!("unfollow:{login}:{}", ctx.now),
                        kind: "unfollow",
                        actor: Some(login.clone()),
                        url: Some(format!("https://github.com/{login}")),
                        occurred_at: ctx.now.to_string(),
                        read: true,
                        count: 1,
                        ..Default::default()
                    },
                )?;
            }
        }
    }

    if let (true, Some(before)) = (seeded, before)
        && let Some(mark) = crossed(FOLLOWER_MILESTONES, before, total)
    {
        milestone(conn, ctx, None, "followers", mark, notices)?;
    }
    if !seeded {
        db::meta_set(conn, "followers_seeded_at", ctx.now)?;
    }
    db::meta_set(conn, "followers_count", &total.to_string())?;
    db::meta_set(conn, "followers_checked_at", ctx.now)?;
    Ok(())
}

pub struct InboxItem {
    pub thread: Thread,
    pub target: Option<ThreadTarget>,
}

pub fn inbox_key(thread: &Thread) -> String {
    format!("inbox:{}:{}", thread.id, thread.updated_at)
}

pub fn event_exists(conn: &Connection, key: &str) -> Result<bool> {
    Ok(conn
        .query_row("SELECT 1 FROM events WHERE key = ?1", [key], |_| Ok(()))
        .optional()?
        .is_some())
}

fn last_segment(url: &str) -> Option<&str> {
    url.rsplit('/').next().filter(|s| !s.is_empty())
}

fn thread_url(thread: &Thread) -> String {
    let repo = &thread.repository.html_url;
    let tail = thread.subject.url.as_deref().and_then(last_segment);
    match (thread.subject.kind.as_str(), tail) {
        ("Issue", Some(n)) => format!("{repo}/issues/{n}"),
        ("PullRequest", Some(n)) => format!("{repo}/pull/{n}"),
        ("Commit", Some(sha)) => format!("{repo}/commit/{sha}"),
        ("Discussion", _) => format!("{repo}/discussions"),
        ("CheckSuite" | "WorkflowRun", _) => format!("{repo}/actions"),
        ("Release", _) => format!("{repo}/releases"),
        ("RepositoryVulnerabilityAlert" | "RepositoryDependabotAlertsThread", _) => {
            format!("{repo}/security/dependabot")
        }
        _ => repo.clone(),
    }
}

fn snippet(body: &str) -> String {
    let flat = body.split_whitespace().collect::<Vec<_>>().join(" ");
    match flat.char_indices().nth(160) {
        Some((cut, _)) => format!("{}…", &flat[..cut]),
        None => flat,
    }
}

fn seconds_apart(a: &str, b: &str) -> Option<i64> {
    let a = DateTime::parse_from_rfc3339(a).ok()?;
    let b = DateTime::parse_from_rfc3339(b).ok()?;
    Some((a - b).num_seconds().abs())
}

/// A notification that only repeats an issue or pull request already reported from its repo.
fn is_echo(conn: &Connection, thread: &Thread, number: Option<i64>) -> Result<bool> {
    let kind = match thread.subject.kind.as_str() {
        "Issue" => "issue",
        "PullRequest" => "pr",
        _ => return Ok(false),
    };
    let about_a_comment = thread
        .subject
        .latest_comment_url
        .as_deref()
        .is_some_and(|c| Some(c) != thread.subject.url.as_deref());
    let Some(number) = number.filter(|_| !about_a_comment) else {
        return Ok(false);
    };
    let opened_at: Option<String> = conn
        .query_row(
            "SELECT occurred_at FROM events WHERE key = ?1",
            [format!("{kind}:{}:{number}", thread.repository.node_id)],
            |r| r.get(0),
        )
        .optional()?;
    Ok(opened_at
        .and_then(|at| seconds_apart(&at, &thread.updated_at))
        .is_some_and(|gap| gap <= 120))
}

fn reason_phrase(reason: &str) -> &'static str {
    match reason {
        "mention" => "You were mentioned",
        "team_mention" => "Your team was mentioned",
        "review_requested" => "Review requested",
        "assign" => "You were assigned",
        "author" => "New activity on your thread",
        "comment" => "New reply",
        "ci_activity" => "Workflow run finished",
        "security_alert" => "Security alert",
        "state_change" => "Thread status changed",
        "invitation" => "Repository invitation",
        "approval_requested" => "Deployment approval requested",
        _ => "New activity",
    }
}

pub fn inbox(
    conn: &Connection,
    ctx: &Ctx,
    items: &[InboxItem],
    notices: &mut Vec<Notice>,
) -> Result<()> {
    let seeded = db::meta_get(conn, "inbox_seeded_at")?;
    for item in items {
        let thread = &item.thread;
        let number = match thread.subject.kind.as_str() {
            "Issue" | "PullRequest" => thread
                .subject
                .url
                .as_deref()
                .and_then(last_segment)
                .and_then(|n| n.parse::<i64>().ok()),
            _ => None,
        };
        if is_echo(conn, thread, number)? {
            continue;
        }
        let target = item.target.as_ref();
        let who = target.and_then(|t| t.user.as_ref().or(t.author.as_ref()));
        let text = target
            .and_then(|t| t.body.as_deref())
            .map(snippet)
            .filter(|s| !s.is_empty());
        let fresh = seeded
            .as_deref()
            .is_some_and(|s| thread.updated_at.as_str() > s);
        let quiet = !fresh || who.is_some_and(|w| ctx.is_viewer(&w.login));
        let inserted = db::insert_event(
            conn,
            &NewEvent {
                key: inbox_key(thread),
                kind: "inbox",
                repo_id: Some(thread.repository.node_id.clone()),
                repo: Some(thread.repository.full_name.clone()),
                actor: who.map(|w| w.login.clone()),
                actor_avatar: who.and_then(|w| w.avatar_url.clone()),
                subject: Some(thread.subject.title.clone()),
                number,
                url: Some(
                    target
                        .and_then(|t| t.html_url.clone())
                        .unwrap_or_else(|| thread_url(thread)),
                ),
                meta: Some(json!({
                    "reason": thread.reason,
                    "type": thread.subject.kind,
                    "snippet": text,
                })),
                occurred_at: thread.updated_at.clone(),
                read: quiet,
                count: 1,
            },
        )?;
        if inserted && !quiet {
            notices.push(Notice {
                kind: "inbox",
                title: format!(
                    "{} in {}",
                    reason_phrase(&thread.reason),
                    ctx.short(&thread.repository.full_name)
                ),
                body: match (who, &text) {
                    (Some(w), Some(text)) => format!("{}: {text}", w.login),
                    _ => thread.subject.title.clone(),
                },
                actor: None,
            });
        }
    }
    if seeded.is_none() {
        db::meta_set(conn, "inbox_seeded_at", ctx.now)?;
    }
    Ok(())
}

/// Stores a repo's daily views and clones and reports an unusual burst of views.
pub fn traffic(
    conn: &Connection,
    ctx: &Ctx,
    repo_id: &str,
    repo_name: &str,
    repo_url: &str,
    traffic: &Traffic,
    notices: &mut Vec<Notice>,
) -> Result<()> {
    let first: bool = conn
        .query_row(
            "SELECT traffic_at IS NULL FROM repos WHERE id = ?1",
            [repo_id],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(true);

    for day in &traffic.views {
        conn.execute(
            "INSERT INTO traffic_daily (repo_id, date, views, view_uniques) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (repo_id, date) DO UPDATE SET
               views = excluded.views, view_uniques = excluded.view_uniques",
            params![
                repo_id,
                &day.timestamp[..10.min(day.timestamp.len())],
                day.count,
                day.uniques
            ],
        )?;
    }
    for day in &traffic.clones {
        conn.execute(
            "INSERT INTO traffic_daily (repo_id, date, clones, clone_uniques) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (repo_id, date) DO UPDATE SET
               clones = excluded.clones, clone_uniques = excluded.clone_uniques",
            params![repo_id, &day.timestamp[..10.min(day.timestamp.len())], day.count, day.uniques],
        )?;
    }
    conn.execute("DELETE FROM referrers WHERE repo_id = ?1", [repo_id])?;
    for r in &traffic.referrers {
        conn.execute(
            "INSERT OR REPLACE INTO referrers (repo_id, referrer, count, uniques) VALUES (?1, ?2, ?3, ?4)",
            params![repo_id, r.referrer, r.count, r.uniques],
        )?;
    }
    conn.execute(
        "UPDATE repos SET traffic_at = ?1 WHERE id = ?2",
        params![ctx.now, repo_id],
    )?;

    // GitHub buckets traffic by UTC day.
    let utc_today = Utc::now().format("%Y-%m-%d").to_string();
    let Some(today) = traffic
        .views
        .iter()
        .find(|d| d.timestamp.starts_with(&utc_today))
    else {
        return Ok(());
    };
    let earlier: i64 = traffic
        .views
        .iter()
        .filter(|d| !d.timestamp.starts_with(&utc_today))
        .map(|d| d.count)
        .sum();
    let usual = (earlier as f64 / 13.0).max(1.0);
    if first || today.count < SPIKE_MIN_VIEWS || (today.count as f64) < SPIKE_FACTOR * usual {
        return Ok(());
    }

    let top = traffic.referrers.iter().max_by_key(|r| r.count);
    let key = format!("traffic:{repo_id}:{utc_today}");
    let event = NewEvent {
        key: key.clone(),
        kind: "traffic",
        repo_id: Some(repo_id.to_string()),
        repo: Some(repo_name.to_string()),
        subject: top.map(|r| r.referrer.clone()),
        count: today.count,
        url: Some(format!("{repo_url}/graphs/traffic")),
        meta: Some(json!({ "uniques": today.uniques })),
        occurred_at: ctx.now.to_string(),
        ..Default::default()
    };
    if db::insert_event(conn, &event)? {
        notices.push(Notice {
            kind: "traffic",
            title: format!("{} is getting more visitors", ctx.short(repo_name)),
            body: match top {
                Some(r) => format!(
                    "{} today, mostly from {}",
                    plural(today.count, "view", "views"),
                    r.referrer
                ),
                None => format!("{} today", plural(today.count, "view", "views")),
            },
            actor: None,
        });
    } else {
        conn.execute(
            "UPDATE events SET count = ?1, subject = COALESCE(?2, subject) WHERE key = ?3",
            params![today.count, event.subject, key],
        )?;
    }
    Ok(())
}
