//! One pass over the account: fetch what changed, record it, report what is new.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Mutex;

use anyhow::Result;
use chrono::{DateTime, Duration, Local, SecondsFormat, Utc};
use futures_util::stream::{self, StreamExt};
use rusqlite::Connection;

use crate::apply::{self, Ctx, Fetched, InboxItem, Notice, ReleaseSet};
use crate::db;
use crate::github::{
    Activity, ApiError, GitHub, RateLimit, Repo, RepoReleases, Thread, ThreadTarget, Traffic, User,
};
use crate::settings::Settings;
use crate::{hackernews, highlights};

const ACTIVITY_CHUNK: usize = 8;
const RELEASE_CHUNK: usize = 10;
/// Stargazers fetched per repo: the full page on first sight, a short one afterwards.
const STAR_PAGE_FIRST: u32 = 100;
const STAR_PAGE: u32 = 30;
const STAR_BACKFILL_PAGES: usize = 30;
const RELEASE_PAGES: usize = 40;
const FOLLOWER_PAGES: usize = 50;
const INBOX_LOOKUPS: usize = 20;
const PEOPLE_CHUNK: usize = 40;
const PEOPLE_PER_SYNC: i64 = 200;

pub struct Report {
    pub viewer: User,
    pub notices: Vec<Notice>,
    pub rates: BTreeMap<String, RateLimit>,
    /// Parts of the sync that failed without stopping it.
    pub warnings: Vec<String>,
    /// What stopped the sync after its main findings were already stored.
    pub cut_short: Option<anyhow::Error>,
}

/// The token belongs to a different account than the one the stored data is about.
#[derive(Debug)]
pub struct OtherAccount {
    pub watched: String,
    pub signed_in: String,
}

impl std::fmt::Display for OtherAccount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Birdfeeder has been watching {}, and this token belongs to {}.",
            self.watched, self.signed_in
        )
    }
}

impl std::error::Error for OtherAccount {}

/// Refuses to mix a second account into what is stored about the first.
pub(crate) fn same_account(conn: &Connection, viewer: &User) -> Result<()> {
    let Some(watched) =
        db::meta_get(conn, "account")?.and_then(|raw| serde_json::from_str::<User>(&raw).ok())
    else {
        return Ok(());
    };
    let same = if watched.id != 0 && viewer.id != 0 {
        watched.id == viewer.id
    } else {
        watched.login.eq_ignore_ascii_case(&viewer.login)
    };
    if same {
        return Ok(());
    }
    Err(OtherAccount {
        watched: watched.login,
        signed_in: viewer.login.clone(),
    }
    .into())
}

pub fn now_utc() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn older_than(stamp: Option<&str>, age: Duration) -> bool {
    stamp
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .is_none_or(|at| Utc::now() - at.with_timezone(&Utc) > age)
}

/// Errors that make the rest of the sync pointless.
fn is_fatal(e: &ApiError) -> bool {
    matches!(
        e,
        ApiError::Unauthorized | ApiError::RateLimited { .. } | ApiError::Offline(_)
    )
}

pub async fn run(
    db: &Mutex<Connection>,
    gh: &GitHub,
    settings: &Settings,
    progress: &(dyn Fn(&str) + Sync),
) -> Result<Report> {
    let now = now_utc();
    let today = Local::now().format("%Y-%m-%d").to_string();
    let mut warnings = Vec::new();
    let mut notices = Vec::new();

    progress("Checking repositories");
    let (viewer, repos) = tokio::try_join!(gh.viewer(), gh.repos(settings.include_org_repos))?;
    same_account(&db.lock().unwrap(), &viewer)?;
    // A repo can be listed twice when the list changes while it is being paged through.
    let mut listed = HashSet::new();
    let repos: Vec<Repo> = repos
        .into_iter()
        .filter(|r| {
            (settings.include_forks || !r.fork) && (settings.include_archived || !r.archived)
        })
        .filter(|r| listed.insert(r.node_id.clone()))
        .collect();
    let ctx = Ctx {
        now: &now,
        today: &today,
        viewer: &viewer.login,
        ignore_assets: &settings.ignore_assets,
    };

    let stored = db::stored_repos(&db.lock().unwrap())?;
    let flagged: Vec<&Repo> = repos
        .iter()
        .filter(|r| apply::needs_activity(stored.get(&r.node_id), r))
        .collect();
    let flagged_ids: HashSet<&str> = flagged.iter().map(|r| r.node_id.as_str()).collect();

    let mut activity: HashMap<String, Activity> = HashMap::new();
    if !flagged.is_empty() {
        progress(&match flagged.len() {
            1 => "Reading activity for 1 repository".to_string(),
            n => format!("Reading activity for {n} repositories"),
        });
        let mut jobs = Vec::new();
        for chunk in flagged.chunks(ACTIVITY_CHUNK) {
            let unseen = chunk.iter().any(|r| {
                stored
                    .get(&r.node_id)
                    .is_none_or(|s| s.seeded_at.is_none() || s.gone)
            });
            let ids: Vec<String> = chunk.iter().map(|r| r.node_id.clone()).collect();
            let page = if unseen { STAR_PAGE_FIRST } else { STAR_PAGE };
            jobs.push(fetch_activity(gh, ids, page));
        }
        let results: Vec<_> = stream::iter(jobs).buffer_unordered(4).collect().await;
        for result in results {
            match result {
                Ok(nodes) => activity.extend(nodes.into_iter().map(|a| (a.id.clone(), a))),
                Err(e) if is_fatal(&e) => return Err(e.into()),
                Err(e) => warnings.push(format!("Activity: {e}")),
            }
        }
    }

    let with_releases: Vec<String> = repos
        .iter()
        .filter(|r| {
            let count = match activity.get(&r.node_id) {
                Some(a) => a.releases.total,
                None => stored.get(&r.node_id).map_or(0, |s| s.release_count),
            };
            count > 0
        })
        .map(|r| r.node_id.clone())
        .collect();
    let mut releases: HashMap<String, ReleaseSet> = HashMap::new();
    if !with_releases.is_empty() {
        progress("Counting release downloads");
        let jobs: Vec<_> = with_releases
            .chunks(RELEASE_CHUNK)
            .map(|ids| fetch_releases(gh, ids.to_vec()))
            .collect();
        let results: Vec<_> = stream::iter(jobs).buffer_unordered(2).collect().await;
        for result in results {
            let nodes = match result {
                Ok(nodes) => nodes,
                Err(e) if is_fatal(&e) => return Err(e.into()),
                Err(e) => {
                    warnings.push(format!("Releases: {e}"));
                    continue;
                }
            };
            for node in nodes {
                let mut set = ReleaseSet {
                    complete: !node.releases.page_info.has_next_page,
                    nodes: node.releases.nodes,
                    paged: false,
                };
                // Older releases change slowly, so the long tail is re-read only now and then.
                let due = older_than(
                    stored
                        .get(&node.id)
                        .and_then(|s| s.releases_full_at.as_deref()),
                    Duration::hours(6),
                );
                let mut cursor = node.releases.page_info.end_cursor;
                while let (false, true, Some(at)) = (set.complete, due, cursor.take()) {
                    if set.nodes.len() >= RELEASE_PAGES * 50 {
                        break;
                    }
                    match gh.release_page(&node.id, &at).await {
                        Ok(Some(page)) => {
                            set.paged = true;
                            set.complete = !page.page_info.has_next_page;
                            set.nodes.extend(page.nodes);
                            cursor = page.page_info.end_cursor;
                        }
                        Ok(None) => break,
                        Err(e) if is_fatal(&e) => return Err(e.into()),
                        Err(e) => {
                            warnings.push(format!("Releases: {e}"));
                            break;
                        }
                    }
                }
                releases.insert(node.id, set);
            }
        }
    }

    {
        let mut conn = db.lock().unwrap();
        let tx = conn.transaction()?;
        apply::begin(&tx)?;
        for repo in &repos {
            let fetched = Fetched {
                repo,
                stored: stored.get(&repo.node_id),
                flagged: flagged_ids.contains(repo.node_id.as_str()),
                activity: activity.get(&repo.node_id),
                releases: releases.get(&repo.node_id),
            };
            apply::repo(&tx, &ctx, &fetched, &mut notices)?;
        }
        apply::finish(&tx, &ctx, &stored, viewer.followers, &mut notices)?;
        db::meta_set(&tx, "account", &serde_json::to_string(&viewer)?)?;
        tx.commit()?;
    }

    // What was found so far is stored. A failure from here on stops the remaining
    // reads, and still lets the notices for it go out.
    let cut_short = rest(
        db,
        gh,
        settings,
        &ctx,
        &viewer,
        &repos,
        &stored,
        progress,
        &mut notices,
        &mut warnings,
    )
    .await
    .err();
    {
        let conn = db.lock().unwrap();
        highlights::note_streak(&conn, &ctx, &mut notices)?;
        highlights::note_record(&conn, &ctx, &mut notices)?;
        highlights::note_trends(&conn, &ctx, &mut notices)?;
        highlights::note_sightings(&conn, &ctx, &mut notices)?;
        for notice in &mut notices {
            highlights::introduce(&conn, notice)?;
        }
    }

    Ok(Report {
        viewer,
        notices,
        rates: gh.rates(),
        warnings,
        cut_short,
    })
}

/// The reads that follow the repos: followers, inbox, traffic, history and profiles.
#[allow(clippy::too_many_arguments)]
async fn rest(
    db: &Mutex<Connection>,
    gh: &GitHub,
    settings: &Settings,
    ctx: &Ctx<'_>,
    viewer: &User,
    repos: &[Repo],
    stored: &HashMap<String, db::StoredRepo>,
    progress: &(dyn Fn(&str) + Sync),
    notices: &mut Vec<Notice>,
    warnings: &mut Vec<String>,
) -> Result<()> {
    if let Err(e) = sync_followers(db, gh, ctx, viewer, progress, notices).await {
        soften(e, "Followers", warnings)?;
    }
    if settings.watch_inbox
        && let Err(e) = sync_inbox(db, gh, ctx, progress, notices).await
    {
        soften(e, "Notifications", warnings)?;
    }
    if settings.track_traffic
        && let Err(e) = sync_traffic(db, gh, ctx, repos, stored, progress, notices).await
    {
        soften(e, "Traffic", warnings)?;
    }
    if let Err(e) = backfill_stars(db, gh, progress).await {
        soften(e, "Star history", warnings)?;
    }
    if let Err(e) = sync_contributions(db, gh).await {
        soften(e, "Contributions", warnings)?;
    }
    if let Err(e) = sync_people(db, gh, progress).await {
        soften(e, "Profiles", warnings)?;
    }
    if settings.watch_hacker_news
        && let Err(e) = sync_stories(db, gh, ctx, notices).await
    {
        warnings.push(format!("Hacker News: {e}"));
    }
    Ok(())
}

// These are named functions so that their futures stay `Send`.

async fn fetch_activity(
    gh: &GitHub,
    ids: Vec<String>,
    page: u32,
) -> Result<Vec<Activity>, ApiError> {
    gh.activity(&ids, page).await
}

async fn fetch_releases(gh: &GitHub, ids: Vec<String>) -> Result<Vec<RepoReleases>, ApiError> {
    gh.releases(&ids).await
}

async fn fetch_traffic<'a>(gh: &GitHub, repo: &'a Repo) -> (&'a Repo, Result<Traffic, ApiError>) {
    (repo, gh.traffic(&repo.full_name).await)
}

/// Looks up the comment or item a notification points at, when `lookup` allows it.
async fn fetch_target(gh: &GitHub, thread: Thread, lookup: bool) -> InboxItem {
    let link = thread
        .subject
        .latest_comment_url
        .as_deref()
        .or(thread.subject.url.as_deref());
    let target = match link {
        Some(url) if lookup => gh.get::<ThreadTarget>(url).await.ok(),
        _ => None,
    };
    InboxItem { thread, target }
}

/// Turns a failure in an optional step into a warning, unless it would fail everything.
fn soften(e: anyhow::Error, step: &str, warnings: &mut Vec<String>) -> Result<()> {
    if e.downcast_ref::<ApiError>().is_some_and(is_fatal) {
        return Err(e);
    }
    warnings.push(format!("{step}: {e}"));
    Ok(())
}

async fn sync_followers(
    db: &Mutex<Connection>,
    gh: &GitHub,
    ctx: &Ctx<'_>,
    viewer: &User,
    progress: &(dyn Fn(&str) + Sync),
    notices: &mut Vec<Notice>,
) -> Result<()> {
    let (count, checked_at) = {
        let conn = db.lock().unwrap();
        (
            db::meta_get(&conn, "followers_count")?.and_then(|v| v.parse::<i64>().ok()),
            db::meta_get(&conn, "followers_checked_at")?,
        )
    };
    if count == Some(viewer.followers) && !older_than(checked_at.as_deref(), Duration::hours(1)) {
        return Ok(());
    }
    progress("Checking followers");
    let (list, complete) = gh.followers(FOLLOWER_PAGES).await?;
    let mut conn = db.lock().unwrap();
    let tx = conn.transaction()?;
    apply::followers(&tx, ctx, &list, complete, viewer.followers, notices)?;
    tx.commit()?;
    Ok(())
}

async fn sync_inbox(
    db: &Mutex<Connection>,
    gh: &GitHub,
    ctx: &Ctx<'_>,
    progress: &(dyn Fn(&str) + Sync),
    notices: &mut Vec<Notice>,
) -> Result<()> {
    progress("Checking notifications");
    let stamp = db::meta_get(&db.lock().unwrap(), "inbox_last_modified")?;
    let Some((threads, new_stamp)) = gh.inbox(stamp.as_deref()).await? else {
        return Ok(());
    };

    let unseen = {
        let conn = db.lock().unwrap();
        let mut unseen = Vec::new();
        for thread in threads {
            if !apply::event_exists(&conn, &apply::inbox_key(&thread))? {
                unseen.push(thread);
            }
        }
        unseen
    };
    let lookups: Vec<_> = unseen
        .into_iter()
        .enumerate()
        .map(|(i, thread)| fetch_target(gh, thread, i < INBOX_LOOKUPS))
        .collect();
    let items: Vec<InboxItem> = stream::iter(lookups).buffer_unordered(5).collect().await;

    let mut conn = db.lock().unwrap();
    let tx = conn.transaction()?;
    apply::inbox(&tx, ctx, &items, notices)?;
    match new_stamp {
        Some(stamp) => db::meta_set(&tx, "inbox_last_modified", &stamp)?,
        None => db::meta_delete(&tx, "inbox_last_modified")?,
    }
    tx.commit()?;
    Ok(())
}

async fn sync_traffic(
    db: &Mutex<Connection>,
    gh: &GitHub,
    ctx: &Ctx<'_>,
    repos: &[Repo],
    stored: &HashMap<String, db::StoredRepo>,
    progress: &(dyn Fn(&str) + Sync),
    notices: &mut Vec<Notice>,
) -> Result<()> {
    let due: Vec<&Repo> = repos
        .iter()
        .filter(|r| r.permissions.push)
        .filter(|r| {
            let stamps = stored.get(&r.node_id);
            let read = stamps.and_then(|s| s.traffic_at.as_deref());
            let tried = stamps.and_then(|s| s.traffic_tried_at.as_deref());
            older_than(read.max(tried), Duration::hours(3))
        })
        .collect();
    if due.is_empty() {
        return Ok(());
    }
    progress("Updating traffic");
    let jobs: Vec<_> = due.into_iter().map(|r| fetch_traffic(gh, r)).collect();
    let results: Vec<(&Repo, Result<Traffic, ApiError>)> =
        stream::iter(jobs).buffer_unordered(4).collect().await;

    let mut failed = 0;
    let mut conn = db.lock().unwrap();
    let tx = conn.transaction()?;
    for (repo, result) in results {
        match result {
            Ok(traffic) => apply::traffic(
                &tx,
                ctx,
                &repo.node_id,
                &repo.full_name,
                &repo.html_url,
                &traffic,
                notices,
            )?,
            Err(e) if is_fatal(&e) => return Err(e.into()),
            Err(_) => {
                failed += 1;
                tx.execute(
                    "UPDATE repos SET traffic_tried_at = ?1 WHERE id = ?2",
                    rusqlite::params![ctx.now, repo.node_id],
                )?;
            }
        }
    }
    tx.commit()?;
    if failed > 0 {
        anyhow::bail!("could not read traffic for {failed} repositories");
    }
    Ok(())
}

/// Stores the account's own contributions for each day of the last year.
async fn sync_contributions(db: &Mutex<Connection>, gh: &GitHub) -> Result<()> {
    let days = gh.contributions().await?;
    let mut conn = db.lock().unwrap();
    let tx = conn.transaction()?;
    for (date, count) in days {
        tx.execute(
            "INSERT INTO contributions (date, count) VALUES (?1, ?2)
             ON CONFLICT (date) DO UPDATE SET count = excluded.count",
            rusqlite::params![date, count],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// Reads the public profile of visitors not looked up yet, newest first.
async fn sync_people(
    db: &Mutex<Connection>,
    gh: &GitHub,
    progress: &(dyn Fn(&str) + Sync),
) -> Result<()> {
    let wanted: Vec<String> = {
        let conn = db.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT actor FROM events
             WHERE actor IS NOT NULL AND kind IN ('star', 'fork', 'follower', 'issue', 'pr', 'inbox')
               AND actor NOT IN (SELECT login FROM people)
             GROUP BY actor ORDER BY MAX(occurred_at) DESC LIMIT ?1",
        )?;
        stmt.query_map([PEOPLE_PER_SYNC], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?
    };
    if wanted.len() > PEOPLE_CHUNK {
        progress("Getting to know your visitors");
    }
    for chunk in wanted.chunks(PEOPLE_CHUNK) {
        // A login with other characters is a bot, which has no profile to ask for.
        let askable: Vec<String> = chunk
            .iter()
            .filter(|login| login.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
            .cloned()
            .collect();
        let found = if askable.is_empty() {
            Vec::new()
        } else {
            gh.people(&askable).await?
        };
        let conn = db.lock().unwrap();
        let now = now_utc();
        for login in chunk {
            let person = found.iter().find(|p| p.login.eq_ignore_ascii_case(login));
            conn.execute(
                "INSERT OR REPLACE INTO people (login, name, company, followers, fetched_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    login,
                    person.and_then(|p| p.name.as_deref()),
                    person.and_then(|p| p.company.as_deref()),
                    person.map(|p| p.followers.total),
                    now
                ],
            )?;
        }
    }
    Ok(())
}

/// Looks on Hacker News for links to the account's repos, at most every half hour.
async fn sync_stories(
    db: &Mutex<Connection>,
    gh: &GitHub,
    ctx: &Ctx<'_>,
    notices: &mut Vec<Notice>,
) -> Result<()> {
    let checked = db::meta_get(&db.lock().unwrap(), "stories_checked_at")?;
    if !older_than(checked.as_deref(), Duration::minutes(30)) {
        return Ok(());
    }
    let stories = hackernews::stories(gh.http(), ctx.viewer).await?;
    let conn = db.lock().unwrap();
    highlights::note_stories(&conn, ctx, &stories, notices)?;
    db::meta_set(&conn, "stories_checked_at", ctx.now)?;
    Ok(())
}

/// Pages in older stargazers for repos whose history did not fit the first page.
async fn backfill_stars(
    db: &Mutex<Connection>,
    gh: &GitHub,
    progress: &(dyn Fn(&str) + Sync),
) -> Result<()> {
    let pending: Vec<(String, String, String)> = {
        let conn = db.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, star_cursor FROM repos WHERE star_cursor IS NOT NULL AND gone = 0",
        )?;
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?
    };
    if pending.is_empty() {
        return Ok(());
    }
    progress("Reading star history");
    for (id, name, mut cursor) in pending {
        for _ in 0..STAR_BACKFILL_PAGES {
            let page = gh.stargazer_page(&id, &cursor).await?;
            let conn = db.lock().unwrap();
            let next = match &page {
                Some(page) => {
                    for edge in &page.edges {
                        db::insert_event(
                            &conn,
                            &apply::star_event_for(&id, &name, &edge.node, &edge.starred_at, true),
                        )?;
                    }
                    page.page_info
                        .end_cursor
                        .clone()
                        .filter(|_| page.page_info.has_next_page)
                }
                None => None,
            };
            conn.execute(
                "UPDATE repos SET star_cursor = ?1 WHERE id = ?2",
                rusqlite::params![next, id],
            )?;
            match next {
                Some(next) => cursor = next,
                None => break,
            }
        }
    }
    Ok(())
}
