//! Read-only queries behind the app's screens.

use std::collections::HashMap;

use anyhow::Result;
use chrono::{Duration, Local, SecondsFormat, Utc};
use rusqlite::{Connection, params};
use serde::Serialize;

use crate::apply::glob_match;
use crate::db;
use crate::github::{Asset, Referrer, User};
use crate::highlights::{self, Goal, Records, Sighting, Streak, Visitor};

#[derive(Debug, Serialize)]
pub struct Point {
    pub date: String,
    pub value: i64,
}

#[derive(Debug, Default, Serialize)]
pub struct Totals {
    pub repos: i64,
    pub stars: i64,
    pub forks: i64,
    pub watchers: i64,
    pub open_issues: i64,
    pub open_prs: i64,
    pub downloads: i64,
    pub followers: i64,
}

/// Changes over the last seven days. `None` where nothing was tracked yet.
#[derive(Debug, Default, Serialize)]
pub struct Week {
    pub stars: i64,
    pub forks: i64,
    pub issues: i64,
    pub prs: i64,
    pub followers: Option<i64>,
    pub downloads: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct DayCount {
    pub date: String,
    pub kind: String,
    pub count: i64,
}

#[derive(Debug, Serialize)]
pub struct Overview {
    pub account: Option<User>,
    pub totals: Totals,
    pub week: Week,
    pub star_series: Vec<Point>,
    pub days: Vec<DayCount>,
    /// The account's own contributions on each day the grid shows.
    pub contributions: Vec<Point>,
    pub goals: Vec<Goal>,
    pub records: Records,
    pub streak: Option<Streak>,
}

/// Kinds whose `count` is a size rather than a number of things that happened.
const ONE_EACH: &str = "('milestone', 'traffic', 'sighting', 'record', 'trend', 'story')";

fn utc_ago(days: i64) -> String {
    (Utc::now() - Duration::days(days)).to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn local_day_ago(days: i64) -> String {
    (Local::now() - Duration::days(days))
        .format("%Y-%m-%d")
        .to_string()
}

fn totals(conn: &Connection, followers: i64) -> Result<Totals> {
    Ok(conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(stars), 0), COALESCE(SUM(forks), 0),
                COALESCE(SUM(watchers), 0), COALESCE(SUM(open_issues), 0),
                COALESCE(SUM(open_prs), 0), COALESCE(SUM(downloads), 0)
         FROM repos WHERE gone = 0",
        [],
        |r| {
            Ok(Totals {
                repos: r.get(0)?,
                stars: r.get(1)?,
                forks: r.get(2)?,
                watchers: r.get(3)?,
                open_issues: r.get(4)?,
                open_prs: r.get(5)?,
                downloads: r.get(6)?,
                followers,
            })
        },
    )?)
}

fn event_sum(conn: &Connection, kind: &str, since: &str) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COALESCE(SUM(count), 0) FROM events WHERE kind = ?1 AND occurred_at >= ?2",
        params![kind, since],
        |r| r.get(0),
    )?)
}

/// Star count over time, rebuilt from star events and pinned to the current total.
fn star_series(conn: &Connection, repo_id: Option<&str>, current: i64) -> Result<Vec<Point>> {
    let mut stmt = conn.prepare(
        "SELECT date(e.occurred_at, 'localtime') AS day,
                SUM(CASE e.kind WHEN 'star' THEN e.count ELSE -e.count END)
         FROM events e JOIN repos r ON r.id = e.repo_id AND r.gone = 0
         WHERE e.kind IN ('star', 'unstar') AND (?1 IS NULL OR e.repo_id = ?1)
         GROUP BY day ORDER BY day",
    )?;
    let changes: Vec<(String, i64)> = stmt
        .query_map([repo_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;

    // Stars from people the history does not name are assumed to predate it.
    let untracked = current - changes.iter().map(|(_, n)| n).sum::<i64>();
    let mut running = untracked;
    let mut series: Vec<Point> = changes
        .into_iter()
        .map(|(date, n)| {
            running += n;
            Point {
                date,
                value: running.max(0),
            }
        })
        .collect();
    let today = Local::now().format("%Y-%m-%d").to_string();
    if series.last().is_none_or(|p| p.date != today) {
        series.push(Point {
            date: today,
            value: current,
        });
    }
    Ok(series)
}

pub fn overview(conn: &Connection, days: i64) -> Result<Overview> {
    let account: Option<User> =
        db::meta_get(conn, "account")?.and_then(|raw| serde_json::from_str(&raw).ok());
    let totals = totals(conn, account.as_ref().map_or(0, |a| a.followers))?;
    let tracking_since = db::meta_get(conn, "tracking_since")?;

    let since = utc_ago(7);
    let week = Week {
        stars: event_sum(conn, "star", &since)? - event_sum(conn, "unstar", &since)?,
        forks: event_sum(conn, "fork", &since)?,
        issues: event_sum(conn, "issue", &since)?,
        prs: event_sum(conn, "pr", &since)?,
        followers: db::meta_get(conn, "followers_seeded_at")?
            .map(|_| -> Result<i64> {
                Ok(event_sum(conn, "follower", &since)? - event_sum(conn, "unfollow", &since)?)
            })
            .transpose()?,
        downloads: tracking_since
            .as_ref()
            .map(|_| event_sum(conn, "downloads", &since))
            .transpose()?,
    };

    let mut stmt = conn.prepare(&format!(
        "SELECT date(occurred_at, 'localtime') AS day, kind,
                    SUM(CASE WHEN kind IN {ONE_EACH} THEN 1 ELSE count END)
             FROM events WHERE date(occurred_at, 'localtime') >= ?1
             GROUP BY day, kind ORDER BY day"
    ))?;
    let day_counts = stmt
        .query_map([local_day_ago(days - 1)], |r| {
            Ok(DayCount {
                date: r.get(0)?,
                kind: r.get(1)?,
                count: r.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;

    let mut stmt =
        conn.prepare("SELECT date, count FROM contributions WHERE date >= ?1 ORDER BY date")?;
    let contributions = stmt
        .query_map([local_day_ago(days - 1)], |r| {
            Ok(Point {
                date: r.get(0)?,
                value: r.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;

    Ok(Overview {
        star_series: star_series(conn, None, totals.stars)?,
        account,
        totals,
        week,
        days: day_counts,
        contributions,
        goals: highlights::goals(conn, 4)?,
        records: highlights::records(conn, &Local::now().format("%Y-%m-%d").to_string())?,
        streak: highlights::contribution_streak(conn)?,
    })
}

#[derive(Debug, Serialize)]
pub struct RepoRow {
    pub id: String,
    pub name: String,
    pub url: String,
    pub description: Option<String>,
    pub is_private: bool,
    pub is_fork: bool,
    pub is_archived: bool,
    pub language: Option<String>,
    pub stars: i64,
    pub forks: i64,
    pub watchers: i64,
    pub open_issues: i64,
    pub open_prs: i64,
    pub downloads: i64,
    pub release_count: i64,
    pub stars_30d: i64,
    /// Daily views for the last 14 days, oldest first. Empty when traffic is not tracked.
    pub views: Vec<i64>,
}

pub fn repos(conn: &Connection) -> Result<Vec<RepoRow>> {
    let dates: Vec<String> = (0..14).rev().map(utc_day_ago).collect();
    let mut traffic: HashMap<String, HashMap<String, i64>> = HashMap::new();
    let mut stmt =
        conn.prepare("SELECT repo_id, date, views FROM traffic_daily WHERE date >= ?1")?;
    let rows = stmt.query_map([&dates[0]], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
        ))
    })?;
    for row in rows {
        let (repo_id, date, views) = row?;
        traffic.entry(repo_id).or_default().insert(date, views);
    }

    let mut stmt = conn.prepare(
        "SELECT r.id, r.name, r.url, r.description, r.is_private, r.is_fork, r.is_archived,
                r.language, r.stars, r.forks, r.watchers, r.open_issues, r.open_prs, r.downloads,
                r.release_count,
                (SELECT COALESCE(SUM(CASE e.kind WHEN 'star' THEN e.count ELSE -e.count END), 0)
                   FROM events e
                  WHERE e.repo_id = r.id AND e.kind IN ('star', 'unstar') AND e.occurred_at >= ?1),
                r.traffic_at IS NOT NULL
         FROM repos r WHERE r.gone = 0
         ORDER BY r.stars DESC, r.pushed_at DESC",
    )?;
    let rows = stmt.query_map([utc_ago(30)], |r| {
        let id: String = r.get(0)?;
        let tracked: bool = r.get(16)?;
        let by_day = traffic.remove(&id).unwrap_or_default();
        Ok(RepoRow {
            views: if tracked {
                dates
                    .iter()
                    .map(|d| by_day.get(d).copied().unwrap_or(0))
                    .collect()
            } else {
                Vec::new()
            },
            id,
            name: r.get(1)?,
            url: r.get(2)?,
            description: r.get(3)?,
            is_private: r.get(4)?,
            is_fork: r.get(5)?,
            is_archived: r.get(6)?,
            language: r.get(7)?,
            stars: r.get(8)?,
            forks: r.get(9)?,
            watchers: r.get(10)?,
            open_issues: r.get(11)?,
            open_prs: r.get(12)?,
            downloads: r.get(13)?,
            release_count: r.get(14)?,
            stars_30d: r.get(15)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn utc_day_ago(days: i64) -> String {
    (Utc::now() - Duration::days(days))
        .format("%Y-%m-%d")
        .to_string()
}

#[derive(Debug, Serialize)]
pub struct AssetRow {
    pub name: String,
    pub downloads: i64,
    pub size: i64,
    /// Left out of download totals by the ignore list.
    pub ignored: bool,
}

#[derive(Debug, Serialize)]
pub struct ReleaseRow {
    pub tag: String,
    pub name: Option<String>,
    pub url: Option<String>,
    pub published_at: Option<String>,
    pub is_prerelease: bool,
    pub downloads: i64,
    pub assets: Vec<AssetRow>,
}

fn releases(conn: &Connection, repo_id: &str, ignore: &[String]) -> Result<Vec<ReleaseRow>> {
    let mut stmt = conn.prepare(
        "SELECT tag, name, url, published_at, is_prerelease, downloads, assets
         FROM releases WHERE repo_id = ?1 ORDER BY published_at DESC",
    )?;
    let rows = stmt.query_map([repo_id], |r| {
        let raw: String = r.get(6)?;
        let mut assets: Vec<AssetRow> = serde_json::from_str::<Vec<Asset>>(&raw)
            .unwrap_or_default()
            .into_iter()
            .map(|a| AssetRow {
                ignored: ignore.iter().any(|p| glob_match(p, &a.name)),
                name: a.name,
                downloads: a.downloads,
                size: a.size,
            })
            .collect();
        assets.sort_by_key(|a| (a.ignored, -a.downloads));
        Ok(ReleaseRow {
            tag: r.get(0)?,
            name: r.get(1)?,
            url: r.get(2)?,
            published_at: r.get(3)?,
            is_prerelease: r.get(4)?,
            downloads: r.get(5)?,
            assets,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

#[derive(Debug, Serialize)]
pub struct TrafficDay {
    pub date: String,
    pub views: i64,
    pub view_uniques: i64,
    pub clones: i64,
    pub clone_uniques: i64,
}

#[derive(Debug, Serialize)]
pub struct RepoDetail {
    pub repo: RepoRow,
    pub star_series: Vec<Point>,
    pub releases: Vec<ReleaseRow>,
    pub traffic: Vec<TrafficDay>,
    pub referrers: Vec<Referrer>,
}

pub fn repo_detail(conn: &Connection, id: &str, ignore: &[String]) -> Result<Option<RepoDetail>> {
    let Some(repo) = repos(conn)?.into_iter().find(|r| r.id == id) else {
        return Ok(None);
    };

    let mut stmt = conn.prepare(
        "SELECT date, views, view_uniques, clones, clone_uniques
         FROM traffic_daily WHERE repo_id = ?1 AND date >= ?2 ORDER BY date",
    )?;
    let traffic = stmt
        .query_map(params![id, utc_day_ago(89)], |r| {
            Ok(TrafficDay {
                date: r.get(0)?,
                views: r.get(1)?,
                view_uniques: r.get(2)?,
                clones: r.get(3)?,
                clone_uniques: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;

    let mut stmt = conn.prepare(
        "SELECT referrer, count, uniques FROM referrers WHERE repo_id = ?1 ORDER BY count DESC",
    )?;
    let referrers = stmt
        .query_map([id], |r| {
            Ok(Referrer {
                referrer: r.get(0)?,
                count: r.get(1)?,
                uniques: r.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;

    Ok(Some(RepoDetail {
        star_series: star_series(conn, Some(id), repo.stars)?,
        releases: releases(conn, id, ignore)?,
        traffic,
        referrers,
        repo,
    }))
}

#[derive(Debug, Serialize)]
pub struct RepoDownloads {
    pub id: String,
    pub name: String,
    pub url: String,
    pub total: i64,
    pub releases: Vec<ReleaseRow>,
}

#[derive(Debug, Serialize)]
pub struct Downloads {
    pub total: i64,
    pub tracking_since: Option<String>,
    /// Downloads gained per local day since tracking began.
    pub gained: Vec<Point>,
    pub repos: Vec<RepoDownloads>,
    pub ignore_assets: Vec<String>,
}

pub fn downloads(conn: &Connection, ignore: &[String]) -> Result<Downloads> {
    let mut stmt = conn.prepare(
        "SELECT id, name, url, downloads FROM repos
         WHERE gone = 0 AND id IN (SELECT repo_id FROM releases)
         ORDER BY downloads DESC, name",
    )?;
    let listed: Vec<(String, String, String, i64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut repos = Vec::with_capacity(listed.len());
    for (id, name, url, total) in listed {
        repos.push(RepoDownloads {
            releases: releases(conn, &id, ignore)?,
            id,
            name,
            url,
            total,
        });
    }

    let mut stmt = conn.prepare(
        "SELECT date(occurred_at, 'localtime') AS day, SUM(count)
         FROM events WHERE kind = 'downloads' GROUP BY day ORDER BY day",
    )?;
    let gained = stmt
        .query_map([], |r| {
            Ok(Point {
                date: r.get(0)?,
                value: r.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;

    Ok(Downloads {
        total: repos.iter().map(|r| r.total).sum(),
        tracking_since: db::meta_get(conn, "tracking_since")?,
        gained,
        repos,
        ignore_assets: ignore.to_vec(),
    })
}

#[derive(Debug, Serialize)]
pub struct LifeList {
    pub sightings: Vec<Sighting>,
    /// The best-followed people who have starred, forked or followed.
    pub visitors: Vec<Visitor>,
}

pub fn life_list(conn: &Connection) -> Result<LifeList> {
    Ok(LifeList {
        sightings: highlights::life_list(conn)?,
        visitors: highlights::notable_visitors(conn, 12)?,
    })
}
