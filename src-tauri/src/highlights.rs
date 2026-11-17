//! Goals, records, streaks and the life list: the things worth looking forward to.
//!
//! Everything here is worked out from what is already stored, so it has no network code.

use std::collections::HashMap;

use anyhow::Result;
use chrono::{Datelike, Duration, NaiveDate, SecondsFormat, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use serde_json::json;

use crate::apply::{
    Ctx, DOWNLOAD_MILESTONES, FOLLOWER_MILESTONES, Notice, STAR_MILESTONES, crossed, group_digits,
    plural,
};
use crate::db::{self, NewEvent};
use crate::github::User;
use crate::hackernews::Story;

/// Followers a visitor needs before they count as a rare bird, and as a very rare one.
pub const RARE: i64 = 1_000;
pub const VERY_RARE: i64 = 10_000;

const STREAK_MARKS: &[i64] = &[7, 14, 30, 50, 100, 200, 365, 500, 730, 1_000];
/// A day has to bring at least this many stars before it can be called a record.
const RECORD_FLOOR: i64 = 3;
/// A repo is picking up at this many stars in a week and this multiple of its usual week.
const TREND_FLOOR: i64 = 5;
const TREND_FACTOR: f64 = 3.0;

fn day(text: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").ok()
}

fn ago(days: i64) -> String {
    (Utc::now() - Duration::days(days)).to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn account(conn: &Connection) -> Option<User> {
    db::meta_get(conn, "account")
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str(&raw).ok())
}

/// Stars gained on each local day that had any, oldest first. The owner's own do not count.
fn star_days(conn: &Connection) -> Result<Vec<(NaiveDate, i64)>> {
    let owner = account(conn).map_or_else(String::new, |a| a.login);
    let mut stmt = conn.prepare(
        "SELECT date(occurred_at, 'localtime') AS day, SUM(count)
         FROM events
         WHERE kind = 'star' AND (actor IS NULL OR lower(actor) != lower(?1))
         GROUP BY day ORDER BY day",
    )?;
    let rows = stmt.query_map([owner], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
    })?;
    let mut days = Vec::new();
    for row in rows {
        let (text, count) = row?;
        if let Some(date) = day(&text) {
            days.push((date, count));
        }
    }
    Ok(days)
}

/// The longest run of consecutive days, and the day a run of `length` was first completed.
fn longest_run(days: &[NaiveDate], length: i64) -> (i64, Option<NaiveDate>) {
    let (mut best, mut run, mut reached) = (0, 0, None);
    let mut previous: Option<NaiveDate> = None;
    for &date in days {
        run = match previous {
            Some(p) if date == p + Duration::days(1) => run + 1,
            _ => 1,
        };
        best = best.max(run);
        if run == length && reached.is_none() {
            reached = Some(date);
        }
        previous = Some(date);
    }
    (best, reached)
}

/// The run of consecutive days that ends on `today`, or on the day before it.
fn current_run(days: &[NaiveDate], today: NaiveDate) -> i64 {
    let mut expect = match days.last() {
        Some(&last) if last == today || last == today - Duration::days(1) => last,
        _ => return 0,
    };
    let mut run = 0;
    for &date in days.iter().rev() {
        if date != expect {
            break;
        }
        run += 1;
        expect = date - Duration::days(1);
    }
    run
}

#[derive(Debug, Clone, Serialize)]
pub struct Best {
    pub date: String,
    pub count: i64,
}

#[derive(Debug, Default, Serialize)]
pub struct Records {
    /// The day with the most stars.
    pub best_day: Option<Best>,
    /// The week with the most stars, named by its Monday.
    pub best_week: Option<Best>,
    /// Days in a row with at least one star, up to today.
    pub star_streak: i64,
    pub best_star_streak: i64,
}

pub fn records(conn: &Connection, today: &str) -> Result<Records> {
    let days = star_days(conn)?;
    let dates: Vec<NaiveDate> = days.iter().map(|(d, _)| *d).collect();
    let best_day = days
        .iter()
        .max_by_key(|(d, n)| (*n, *d))
        .map(|(d, n)| Best {
            date: d.to_string(),
            count: *n,
        });

    let mut weeks: Vec<(NaiveDate, i64)> = Vec::new();
    for (date, count) in &days {
        let monday = *date - Duration::days(i64::from(date.weekday().num_days_from_monday()));
        match weeks.last_mut() {
            Some((week, total)) if *week == monday => *total += count,
            _ => weeks.push((monday, *count)),
        }
    }
    let best_week = weeks
        .iter()
        .max_by_key(|(d, n)| (*n, *d))
        .map(|(d, n)| Best {
            date: d.to_string(),
            count: *n,
        });

    Ok(Records {
        best_day,
        best_week,
        star_streak: day(today).map_or(0, |t| current_run(&dates, t)),
        best_star_streak: longest_run(&dates, 0).0,
    })
}

/// The account owner's own run of days with a contribution.
#[derive(Debug, Serialize)]
pub struct Streak {
    pub current: i64,
    pub best: i64,
    /// Contributions on the latest day GitHub reports.
    pub today: i64,
    pub year: i64,
}

pub fn contribution_streak(conn: &Connection) -> Result<Option<Streak>> {
    let mut stmt = conn.prepare("SELECT date, count FROM contributions ORDER BY date")?;
    let rows: Vec<(String, i64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let Some(latest) = rows.last().and_then(|(d, _)| day(d)) else {
        return Ok(None);
    };
    let active: Vec<NaiveDate> = rows
        .iter()
        .filter(|(_, n)| *n > 0)
        .filter_map(|(d, _)| day(d))
        .collect();
    let since = latest - Duration::days(364);
    Ok(Some(Streak {
        current: current_run(&active, latest),
        best: longest_run(&active, 0).0,
        today: rows.last().map_or(0, |(_, n)| *n),
        year: rows
            .iter()
            .filter(|(d, _)| day(d).is_some_and(|d| d >= since))
            .map(|(_, n)| n)
            .sum(),
    }))
}

/// A milestone that has not been reached yet, and how far along it is.
#[derive(Debug, Serialize)]
pub struct Goal {
    /// `stars`, `all-stars`, `downloads` or `followers`.
    pub what: &'static str,
    pub repo_id: Option<String>,
    pub repo: Option<String>,
    pub current: i64,
    /// The milestone already behind, where the progress bar starts.
    pub from: i64,
    pub target: i64,
    /// Days until the target at the pace of the last month, when there is a pace.
    pub eta_days: Option<i64>,
}

fn next_mark(marks: &[i64], current: i64) -> Option<(i64, i64)> {
    let target = marks.iter().copied().find(|&m| m > current)?;
    let from = marks.iter().copied().rfind(|&m| m <= current).unwrap_or(0);
    Some((from, target))
}

fn eta(remaining: i64, gained: i64, over_days: i64) -> Option<i64> {
    if gained <= 0 {
        return None;
    }
    let days = (remaining as f64 * over_days.max(1) as f64 / gained as f64).ceil() as i64;
    (days <= 730).then_some(days.max(1))
}

/// What the last 30 days brought, by repo and kind of event.
struct Recent(HashMap<(String, String), i64>);

impl Recent {
    fn read(conn: &Connection) -> Result<Self> {
        let mut stmt = conn.prepare(
            "SELECT COALESCE(repo_id, ''), kind, SUM(count) FROM events
             WHERE occurred_at >= ?1
               AND kind IN ('star', 'unstar', 'downloads', 'follower', 'unfollow')
             GROUP BY repo_id, kind",
        )?;
        let rows = stmt.query_map([ago(30)], |r| {
            Ok(((r.get(0)?, r.get(1)?), r.get::<_, i64>(2)?))
        })?;
        Ok(Self(rows.collect::<rusqlite::Result<_>>()?))
    }

    fn of(&self, repo_id: &str, kind: &str) -> i64 {
        self.0
            .get(&(repo_id.to_string(), kind.to_string()))
            .copied()
            .unwrap_or(0)
    }

    /// The count for a kind across every repo.
    fn all(&self, kind: &str) -> i64 {
        self.0
            .iter()
            .filter(|((_, k), _)| k == kind)
            .map(|(_, n)| n)
            .sum()
    }
}

/// Days since something started being counted, capped at the 30 the pace is measured over.
fn counted_days(conn: &Connection, key: &str) -> i64 {
    db::meta_get(conn, key)
        .ok()
        .flatten()
        .and_then(|at| chrono::DateTime::parse_from_rfc3339(&at).ok())
        .map_or(30, |at| {
            (Utc::now() - at.with_timezone(&Utc))
                .num_days()
                .clamp(1, 30)
        })
}

/// The milestones most likely to be reached next: those with a pace first, soonest first.
pub fn goals(conn: &Connection, limit: usize) -> Result<Vec<Goal>> {
    let mut goals = Vec::new();
    let tracked = counted_days(conn, "tracking_since");
    let recent = Recent::read(conn)?;

    let mut stmt = conn.prepare(
        "SELECT id, name, stars, downloads FROM repos WHERE gone = 0 AND is_archived = 0",
    )?;
    let repos: Vec<(String, String, i64, i64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<rusqlite::Result<_>>()?;
    for (id, name, stars, downloads) in &repos {
        if let Some((from, target)) = next_mark(STAR_MILESTONES, *stars).filter(|_| *stars > 0) {
            let gained = recent.of(id, "star") - recent.of(id, "unstar");
            goals.push(Goal {
                what: "stars",
                repo_id: Some(id.clone()),
                repo: Some(name.clone()),
                current: *stars,
                from,
                target,
                eta_days: eta(target - stars, gained, 30),
            });
        }
        if let Some((from, target)) =
            next_mark(DOWNLOAD_MILESTONES, *downloads).filter(|_| *downloads > 0)
        {
            goals.push(Goal {
                what: "downloads",
                repo_id: Some(id.clone()),
                repo: Some(name.clone()),
                current: *downloads,
                from,
                target,
                eta_days: eta(target - downloads, recent.of(id, "downloads"), tracked),
            });
        }
    }

    let all_stars: i64 = conn.query_row(
        "SELECT COALESCE(SUM(stars), 0) FROM repos WHERE gone = 0",
        [],
        |r| r.get(0),
    )?;
    if let Some((from, target)) = next_mark(STAR_MILESTONES, all_stars).filter(|_| all_stars > 0) {
        let gained = recent.all("star") - recent.all("unstar");
        goals.push(Goal {
            what: "all-stars",
            repo_id: None,
            repo: None,
            current: all_stars,
            from,
            target,
            eta_days: eta(target - all_stars, gained, 30),
        });
    }
    if let Some(followers) = account(conn).map(|a| a.followers)
        && let Some((from, target)) = next_mark(FOLLOWER_MILESTONES, followers)
    {
        let gained = recent.all("follower") - recent.all("unfollow");
        goals.push(Goal {
            what: "followers",
            repo_id: None,
            repo: None,
            current: followers,
            from,
            target,
            eta_days: eta(
                target - followers,
                gained,
                counted_days(conn, "followers_seeded_at"),
            ),
        });
    }

    // A repo that has every star it needs for the account total would say the same thing twice.
    goals.retain(|g| g.what != "all-stars" || repos.iter().filter(|r| r.2 > 0).count() > 1);
    goals.sort_by_key(|g| (g.eta_days.is_none(), g.eta_days, g.target - g.current));
    goals.truncate(limit);
    Ok(goals)
}

/// One line of the life list: something worth seeing once.
#[derive(Debug, Serialize)]
pub struct Sighting {
    pub id: &'static str,
    pub title: &'static str,
    /// What has to happen for it to be ticked.
    pub how: &'static str,
    pub seen: bool,
    /// When it happened, where the history says.
    pub when: Option<String>,
    pub detail: Option<String>,
    /// How far along an unticked entry is, as the count so far and the count needed.
    pub progress: Option<(i64, i64)>,
    /// The unread event that brought it about, if there is one.
    #[serde(skip)]
    pub cause: Option<i64>,
}

struct Seen {
    when: Option<String>,
    detail: Option<String>,
    /// The unread event that brought it about, which can carry the news itself.
    cause: Option<i64>,
}

/// The earliest event that matches, as when it happened and who or what it was.
fn first_event(conn: &Connection, filter: &str, viewer: &str) -> Result<Option<Seen>> {
    let sql = format!(
        "SELECT e.occurred_at, e.actor, COALESCE(r.name, e.repo), p.followers, e.id, e.read
         FROM events e
         LEFT JOIN repos r ON r.id = e.repo_id
         LEFT JOIN people p ON p.login = e.actor
         WHERE {filter} ORDER BY e.occurred_at LIMIT 1"
    );
    Ok(conn
        .query_row(&sql, [viewer], |r| {
            let actor: Option<String> = r.get(1)?;
            let repo: Option<String> = r.get(2)?;
            let followers: Option<i64> = r.get(3)?;
            let short = repo.map(|full| match full.split_once('/') {
                Some((owner, name)) if owner.eq_ignore_ascii_case(viewer) => name.to_string(),
                _ => full,
            });
            let (id, read): (i64, bool) = (r.get(4)?, r.get(5)?);
            Ok(Seen {
                when: r.get(0)?,
                cause: (!read).then_some(id),
                detail: match (actor, short) {
                    (Some(who), _) if followers.is_some_and(|n| n >= RARE) => Some(format!(
                        "{who}, {} followers",
                        group_digits(followers.unwrap_or(0))
                    )),
                    (Some(who), Some(repo)) => Some(format!("{who}, in {repo}")),
                    (Some(who), None) => Some(who),
                    (None, repo) => repo,
                },
            })
        })
        .optional()?)
}

fn scalar(conn: &Connection, sql: &str) -> Result<i64> {
    Ok(conn.query_row(sql, [], |r| r.get(0))?)
}

/// The repo that leads on a count, with that count.
fn leader(conn: &Connection, column: &str) -> Result<(i64, Option<String>)> {
    let sql = format!(
        "SELECT {column}, name FROM repos WHERE gone = 0 ORDER BY {column} DESC, name LIMIT 1"
    );
    Ok(conn
        .query_row(&sql, [], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?
        .unwrap_or((0, None)))
}

/// Ticked when the count reaches the target, and otherwise shown with how far it has got.
fn counted(count: i64, target: i64, detail: Option<String>) -> (Option<Seen>, Option<(i64, i64)>) {
    if count >= target {
        (
            Some(Seen {
                when: None,
                detail,
                cause: None,
            }),
            None,
        )
    } else {
        (None, Some((count, target)))
    }
}

pub fn life_list(conn: &Connection) -> Result<Vec<Sighting>> {
    let account = account(conn);
    let viewer = account.as_ref().map_or("", |a| a.login.as_str());
    let followers = account.as_ref().map_or(0, |a| a.followers);
    let short = |full: Option<String>| {
        full.map(|f| match f.split_once('/') {
            Some((owner, name)) if owner.eq_ignore_ascii_case(viewer) => name.to_string(),
            _ => f,
        })
    };

    let days = star_days(conn)?;
    let dates: Vec<NaiveDate> = days.iter().map(|(d, _)| *d).collect();
    let best_day = days.iter().map(|(_, n)| *n).max().unwrap_or(0);
    let day_with = |n: i64| {
        days.iter()
            .find(|(_, count)| *count >= n)
            .map(|(d, _)| Seen {
                when: Some(d.to_string()),
                detail: None,
                cause: None,
            })
    };
    let (star_run, week_of_stars) = longest_run(&dates, 7);
    let (most_stars, starriest) = leader(conn, "stars")?;
    let (most_forks, most_forked) = leader(conn, "forks")?;
    let downloads = scalar(
        conn,
        "SELECT COALESCE(SUM(downloads), 0) FROM repos WHERE gone = 0",
    )?;
    let tens = scalar(
        conn,
        "SELECT COUNT(*) FROM repos WHERE gone = 0 AND stars >= 10",
    )?;
    let streak = contribution_streak(conn)?.map_or(0, |s| s.best);
    let other = "(e.actor IS NULL OR lower(e.actor) != lower(?1))";
    let people = format!("e.kind IN ('star', 'fork', 'follower') AND {other}");
    let hour = format!(
        "e.kind = 'star' AND {other} \
         AND CAST(strftime('%H', e.occurred_at, 'localtime') AS INTEGER)"
    );
    let stranger = "e.actor IS NOT NULL AND e.actor != ?1 AND e.actor NOT LIKE '%[bot]' \
                    AND json_extract(e.meta, '$.bot') IS NULL";

    let mut list = Vec::new();
    let mut add = |id, title, how, found: (Option<Seen>, Option<(i64, i64)>)| {
        let (seen, progress) = found;
        list.push(Sighting {
            id,
            title,
            how,
            seen: seen.is_some(),
            when: seen.as_ref().and_then(|s| s.when.clone()),
            cause: seen.as_ref().and_then(|s| s.cause),
            detail: seen.and_then(|s| s.detail),
            progress,
        });
    };

    add(
        "first-visitor",
        "First visitor",
        "Someone stars one of your repositories.",
        (
            first_event(conn, &format!("e.kind = 'star' AND {other}"), viewer)?,
            None,
        ),
    );
    add(
        "nest-box",
        "Nest box",
        "Someone forks one of your repositories.",
        (
            first_event(conn, &format!("e.kind = 'fork' AND {other}"), viewer)?,
            None,
        ),
    );
    add(
        "first-chirp",
        "First chirp",
        "Someone else opens an issue.",
        (
            first_event(conn, &format!("e.kind = 'issue' AND {stranger}"), viewer)?,
            None,
        ),
    );
    add(
        "a-twig",
        "A twig for the nest",
        "Someone else opens a pull request.",
        (
            first_event(conn, &format!("e.kind = 'pr' AND {stranger}"), viewer)?,
            None,
        ),
    );
    add(
        "first-seed",
        "First seed taken",
        "A release file is downloaded.",
        (
            (downloads > 0).then_some(Seen {
                when: None,
                detail: None,
                cause: None,
            }),
            None,
        ),
    );
    add(
        "early-bird",
        "Early bird",
        "A star arrives between five and seven in the morning.",
        (
            first_event(conn, &format!("{hour} BETWEEN 5 AND 6"), viewer)?,
            None,
        ),
    );
    add(
        "night-owl",
        "Night owl",
        "A star arrives between midnight and four in the morning.",
        (
            first_event(conn, &format!("{hour} BETWEEN 0 AND 3"), viewer)?,
            None,
        ),
    );
    add(
        "flock",
        "A flock",
        "Ten stars in one day.",
        match day_with(10) {
            Some(seen) => (Some(seen), None),
            None => (None, Some((best_day, 10))),
        },
    );
    add(
        "murmuration",
        "Murmuration",
        "Fifty stars in one day.",
        match day_with(50) {
            Some(seen) => (Some(seen), None),
            None => (None, Some((best_day, 50))),
        },
    );
    add(
        "regulars",
        "Regulars",
        "A star every day for a week.",
        match week_of_stars {
            Some(date) => (
                Some(Seen {
                    when: Some(date.to_string()),
                    detail: None,
                    cause: None,
                }),
                None,
            ),
            None => (None, Some((star_run, 7))),
        },
    );
    add(
        "rare-bird",
        "Rare bird",
        "A star, fork or follow from someone with 1,000 followers of their own.",
        (
            first_event(conn, &format!("{people} AND p.followers >= {RARE}"), viewer)?,
            None,
        ),
    );
    add(
        "very-rare-bird",
        "Very rare bird",
        "A star, fork or follow from someone with 10,000 followers.",
        (
            first_event(
                conn,
                &format!("{people} AND p.followers >= {VERY_RARE}"),
                viewer,
            )?,
            None,
        ),
    );
    add(
        "century",
        "Century",
        "One repository reaches 100 stars.",
        counted(most_stars, 100, short(starriest.clone())),
    );
    add(
        "thousand",
        "A thousand",
        "One repository reaches 1,000 stars.",
        counted(most_stars, 1_000, short(starriest)),
    );
    add(
        "colony",
        "Colony",
        "One repository is forked ten times.",
        counted(most_forks, 10, short(most_forked)),
    );
    add(
        "full-garden",
        "Full garden",
        "Five repositories with ten stars each.",
        counted(tens, 5, None),
    );
    add(
        "thousand-seeds",
        "A thousand seeds",
        "1,000 downloads across your releases.",
        counted(downloads, 1_000, None),
    );
    add(
        "a-following",
        "A following",
        "A hundred people follow you.",
        counted(followers, 100, None),
    );
    add(
        "front-page",
        "Front page",
        "A burst of visitors in one day, or a Hacker News story with 100 points.",
        (
            first_event(
                conn,
                "(e.kind = 'traffic' OR (e.kind = 'story' AND e.count >= 100)) AND ?1 = ?1",
                viewer,
            )?,
            None,
        ),
    );
    add(
        "creature-of-habit",
        "Creature of habit",
        "You contribute thirty days in a row.",
        counted(streak, 30, None),
    );
    Ok(list)
}

/// The time a number of hours before `now`, in the form timestamps are stored in.
fn earlier(now: &str, hours: i64) -> String {
    chrono::DateTime::parse_from_rfc3339(now).map_or_else(
        |_| now.to_string(),
        |at| {
            (at.with_timezone(&Utc) - Duration::hours(hours))
                .to_rfc3339_opts(SecondsFormat::Secs, true)
        },
    )
}

/// An unread event from the last day that says what a counted entry says.
fn carrier(conn: &Connection, ctx: &Ctx, id: &str) -> Result<Option<i64>> {
    let filter = match id {
        "century" => "kind = 'milestone' AND subject = 'stars' AND count = 100",
        "thousand" => "kind = 'milestone' AND subject = 'stars' AND count = 1000",
        "a-following" => "kind = 'milestone' AND subject = 'followers' AND count = 100",
        "creature-of-habit" => "kind = 'milestone' AND subject = 'streak' AND count = 30",
        "flock" | "murmuration" => "kind = 'record'",
        "colony" => "kind = 'fork'",
        "full-garden" => "kind = 'star'",
        "first-seed" | "thousand-seeds" => "kind = 'downloads'",
        _ => return Ok(None),
    };
    let sql = format!(
        "SELECT id FROM events WHERE read = 0 AND occurred_at >= ?1 AND {filter}
         ORDER BY occurred_at DESC LIMIT 1"
    );
    Ok(conn
        .query_row(&sql, [earlier(ctx.now, 24)], |r| r.get(0))
        .optional()?)
}

/// Whether a newly ticked entry is news, and the unread event that can carry it.
///
/// History keeps arriving after the first look: older stars, profiles, repos
/// that join later. An entry that turns out to have been true all along is
/// ticked without a word.
fn news(conn: &Connection, ctx: &Ctx, sighting: &Sighting) -> Result<(bool, Option<i64>)> {
    if let Some(cause) = sighting.cause {
        return Ok((true, Some(cause)));
    }
    let carrier = carrier(conn, ctx, sighting.id)?;
    // These are dated by the day they came true, which has to be today or yesterday.
    if matches!(sighting.id, "flock" | "murmuration" | "regulars") {
        let yesterday = day(ctx.today).map(|d| (d - Duration::days(1)).to_string());
        let recent = sighting.when.as_deref() >= yesterday.as_deref() && yesterday.is_some();
        return Ok((recent, carrier.filter(|_| recent)));
    }
    Ok((carrier.is_some(), carrier))
}

/// Ticks newly seen entries off the life list, and says so for the ones that just happened.
///
/// When an unread event caused the sighting, that event carries the news, so the
/// activity list does not say the same thing twice.
pub fn note_sightings(conn: &Connection, ctx: &Ctx, notices: &mut Vec<Notice>) -> Result<()> {
    let seeded = db::meta_get(conn, "life_list_seeded_at")?.is_some();
    for sighting in life_list(conn)?.into_iter().filter(|s| s.seen) {
        let fresh = conn.execute(
            "INSERT OR IGNORE INTO sightings (id, noticed_at) VALUES (?1, ?2)",
            params![sighting.id, ctx.now],
        )? > 0;
        if !fresh || !seeded {
            continue;
        }
        let (is_news, cause) = news(conn, ctx, &sighting)?;
        if !is_news {
            continue;
        }
        match cause {
            Some(id) => {
                conn.execute(
                    "UPDATE events SET meta = json_set(COALESCE(meta, '{}'), '$.sighting', ?1)
                     WHERE id = ?2",
                    params![sighting.title, id],
                )?;
            }
            None => {
                db::insert_event(
                    conn,
                    &NewEvent {
                        key: format!("sighting:{}", sighting.id),
                        kind: "sighting",
                        subject: Some(sighting.title.to_string()),
                        meta: Some(json!({
                            "id": sighting.id,
                            "how": sighting.how,
                            "detail": sighting.detail,
                        })),
                        occurred_at: ctx.now.to_string(),
                        count: 1,
                        ..Default::default()
                    },
                )?;
            }
        }
        notices.push(Notice {
            kind: "sighting",
            title: format!("New on your life list: {}", sighting.title),
            body: sighting.detail.unwrap_or_else(|| sighting.how.to_string()),
            actor: None,
        });
    }
    if !seeded {
        db::meta_set(conn, "life_list_seeded_at", ctx.now)?;
    }
    Ok(())
}

/// Reports a day that has brought more stars than any day before it.
pub fn note_record(conn: &Connection, ctx: &Ctx, notices: &mut Vec<Notice>) -> Result<()> {
    let Some(stored) = db::meta_get(conn, "best_star_day")? else {
        // The first look only writes the record down.
        let best = records(conn, ctx.today)?.best_day;
        let known = best.map_or_else(|| json!({ "date": "", "count": 0 }), |b| json!(b));
        return db::meta_set(conn, "best_star_day", &known.to_string());
    };
    let stored: serde_json::Value = serde_json::from_str(&stored).unwrap_or_default();
    let (mut old_count, mut old_date) = (
        stored["count"].as_i64().unwrap_or(0),
        stored["date"].as_str().unwrap_or("").to_string(),
    );
    let days = star_days(conn)?;
    let this_day = day(ctx.today);
    let today = days
        .iter()
        .find(|(d, _)| Some(*d) == this_day)
        .map_or(0, |(_, n)| *n);
    // Older stars can be read in after the record was written down.
    if let Some((date, count)) = days
        .iter()
        .filter(|(d, _)| Some(*d) != this_day)
        .max_by_key(|(d, n)| (*n, *d))
        && *count > old_count
    {
        (old_count, old_date) = (*count, date.to_string());
    }
    if today < RECORD_FLOOR || today <= old_count {
        return Ok(());
    }
    db::meta_set(
        conn,
        "best_star_day",
        &json!({ "date": ctx.today, "count": today }).to_string(),
    )?;
    let key = format!("record:star-day:{}", ctx.today);
    let inserted = db::insert_event(
        conn,
        &NewEvent {
            key: key.clone(),
            kind: "record",
            subject: Some("star-day".into()),
            count: today,
            meta: Some(json!({ "previous": old_count, "previous_date": old_date })),
            occurred_at: ctx.now.to_string(),
            ..Default::default()
        },
    )?;
    if !inserted {
        conn.execute(
            "UPDATE events SET count = ?1 WHERE key = ?2",
            params![today, key],
        )?;
    } else if old_date != ctx.today {
        notices.push(Notice {
            kind: "record",
            title: format!("A new best day: {}", plural(today, "star", "stars")),
            body: match old_count {
                0 => String::new(),
                n => format!("The record was {n}."),
            },
            actor: None,
        });
    }
    Ok(())
}

/// Reports repos whose stars this week run well ahead of their usual week.
pub fn note_trends(conn: &Connection, ctx: &Ctx, notices: &mut Vec<Notice>) -> Result<()> {
    let seeded = db::meta_get(conn, "trends_seeded_at")?.is_some();
    let week = day(ctx.today).map_or_else(String::new, |d| {
        format!("{}-{:02}", d.iso_week().year(), d.iso_week().week())
    });
    let mut stmt = conn.prepare(
        "SELECT r.id, r.name, r.url,
                COALESCE(SUM(CASE WHEN e.occurred_at >= ?1
                                   AND e.occurred_at > COALESCE(r.seeded_at, '9')
                                  THEN e.count END), 0),
                COALESCE(SUM(CASE WHEN e.occurred_at < ?1 THEN e.count END), 0)
         FROM repos r JOIN events e ON e.repo_id = r.id AND e.kind = 'star' AND e.occurred_at >= ?2
         WHERE r.gone = 0 GROUP BY r.id",
    )?;
    let rows: Vec<(String, String, String, i64, i64)> = stmt
        .query_map([ago(7), ago(35)], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?
        .collect::<rusqlite::Result<_>>()?;

    for (id, name, url, this_week, month_before) in rows {
        let usual = month_before as f64 / 4.0;
        if this_week < TREND_FLOOR || (this_week as f64) < TREND_FACTOR * usual.max(1.0) {
            continue;
        }
        let key = format!("trend:{id}:{week}");
        // The same burst is still inside the window when a new week begins.
        let said: bool = conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM events
               WHERE kind = 'trend' AND repo_id = ?1 AND key != ?2 AND occurred_at >= ?3)",
            params![id, key, earlier(ctx.now, 24 * 7)],
            |r| r.get(0),
        )?;
        if said {
            continue;
        }
        let inserted = db::insert_event(
            conn,
            &NewEvent {
                key: key.clone(),
                kind: "trend",
                repo_id: Some(id),
                repo: Some(name.clone()),
                count: this_week,
                url: Some(format!("{url}/stargazers")),
                meta: Some(json!({ "usual": usual.round() as i64 })),
                occurred_at: ctx.now.to_string(),
                read: !seeded,
                ..Default::default()
            },
        )?;
        if !inserted {
            conn.execute(
                "UPDATE events SET count = ?1 WHERE key = ?2",
                params![this_week, key],
            )?;
        } else if seeded {
            let short = match name.split_once('/') {
                Some((owner, repo)) if owner.eq_ignore_ascii_case(ctx.viewer) => repo,
                _ => &name,
            };
            notices.push(Notice {
                kind: "trend",
                title: format!("{short} is picking up"),
                body: match usual.round() as i64 {
                    0 => format!("{} this week.", plural(this_week, "star", "stars")),
                    n => format!(
                        "{} this week, up from about {n} a week.",
                        plural(this_week, "star", "stars")
                    ),
                },
                actor: None,
            });
        }
    }
    if !seeded {
        db::meta_set(conn, "trends_seeded_at", ctx.now)?;
    }
    Ok(())
}

/// Reports a contribution streak reaching a round number of days.
pub fn note_streak(conn: &Connection, ctx: &Ctx, notices: &mut Vec<Notice>) -> Result<()> {
    let Some(streak) = contribution_streak(conn)? else {
        return Ok(());
    };
    let before: Option<i64> = db::meta_get(conn, "streak_seen")?.and_then(|v| v.parse().ok());
    db::meta_set(conn, "streak_seen", &streak.current.to_string())?;
    let Some(mark) = before.and_then(|before| crossed(STREAK_MARKS, before, streak.current)) else {
        return Ok(());
    };
    let inserted = db::insert_event(
        conn,
        &NewEvent {
            key: format!("milestone:streak:{mark}:{}", ctx.today),
            kind: "milestone",
            subject: Some("streak".into()),
            count: mark,
            occurred_at: ctx.now.to_string(),
            ..Default::default()
        },
    )?;
    if inserted {
        notices.push(Notice {
            kind: "milestone",
            title: format!("You reached a {mark}-day streak"),
            body: format!(
                "{} in the last year.",
                plural(streak.year, "contribution", "contributions")
            ),
            actor: None,
        });
    }
    Ok(())
}

/// Records Hacker News stories that link to a watched repo, and follows their score.
pub fn note_stories(
    conn: &Connection,
    ctx: &Ctx,
    stories: &[Story],
    notices: &mut Vec<Notice>,
) -> Result<()> {
    let seeded = db::meta_get(conn, "stories_seeded_at")?;
    for story in stories {
        let repo: Option<(String, String)> = conn
            .query_row(
                "SELECT id, name FROM repos
                 WHERE gone = 0 AND is_private = 0 AND lower(name) = lower(?1)",
                [&story.repo],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let Some((repo_id, name)) = repo else {
            continue;
        };
        let short = match name.split_once('/') {
            Some((owner, repo)) if owner.eq_ignore_ascii_case(ctx.viewer) => repo,
            _ => name.as_str(),
        };
        let key = format!("story:{}", story.id);
        let meta = json!({ "comments": story.comments });
        let points_before: Option<i64> = conn
            .query_row("SELECT count FROM events WHERE key = ?1", [&key], |r| {
                r.get(0)
            })
            .optional()?;

        let Some(before) = points_before else {
            let fresh = seeded
                .as_deref()
                .is_some_and(|s| story.posted_at.as_str() > s);
            db::insert_event(
                conn,
                &NewEvent {
                    key,
                    kind: "story",
                    repo_id: Some(repo_id),
                    repo: Some(name.clone()),
                    subject: Some(story.title.clone()),
                    count: story.points,
                    url: Some(format!("https://news.ycombinator.com/item?id={}", story.id)),
                    meta: Some(meta),
                    occurred_at: story.posted_at.clone(),
                    read: !fresh,
                    ..Default::default()
                },
            )?;
            if fresh {
                notices.push(Notice {
                    kind: "story",
                    title: format!("{short} is on Hacker News"),
                    body: story.title.clone(),
                    actor: None,
                });
            }
            continue;
        };
        // A story that climbs past a hundred points comes back to the top as unread.
        let climbed = seeded.is_some() && before < 100 && story.points >= 100;
        conn.execute(
            "UPDATE events SET count = ?1, meta = ?2, read = CASE WHEN ?3 THEN 0 ELSE read END
             WHERE key = ?4",
            params![story.points.max(1), meta.to_string(), climbed, key],
        )?;
        if climbed {
            notices.push(Notice {
                kind: "story",
                title: format!("{short} passed 100 points on Hacker News"),
                body: story.title.clone(),
                actor: None,
            });
        }
    }
    if seeded.is_none() {
        db::meta_set(conn, "stories_seeded_at", ctx.now)?;
    }
    Ok(())
}

/// A visitor worth pointing out, with what they last did.
#[derive(Debug, Serialize)]
pub struct Visitor {
    pub login: String,
    pub avatar: Option<String>,
    pub name: Option<String>,
    pub company: Option<String>,
    pub followers: i64,
    /// `star`, `fork` or `follower`.
    pub kind: String,
    pub repo: Option<String>,
    pub when: String,
}

/// The visitors with the most followers of their own.
pub fn notable_visitors(conn: &Connection, limit: i64) -> Result<Vec<Visitor>> {
    let mut stmt = conn.prepare(
        "SELECT e.actor, e.actor_avatar, p.name, p.company, p.followers, e.kind,
                COALESCE(r.name, e.repo), MAX(e.occurred_at)
         FROM events e
         JOIN people p ON p.login = e.actor
         LEFT JOIN repos r ON r.id = e.repo_id
         WHERE e.kind IN ('star', 'fork', 'follower') AND p.followers >= 100
         GROUP BY e.actor ORDER BY p.followers DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit], |r| {
        Ok(Visitor {
            login: r.get(0)?,
            avatar: r.get(1)?,
            name: r.get(2)?,
            company: r.get(3)?,
            followers: r.get(4)?,
            kind: r.get(5)?,
            repo: r.get(6)?,
            when: r.get(7)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// What to add to a notification about someone, given how well followed they are.
pub fn introduce(conn: &Connection, notice: &mut Notice) -> Result<()> {
    let Some(login) = &notice.actor else {
        return Ok(());
    };
    let found: Option<(Option<String>, Option<String>, Option<i64>)> = conn
        .query_row(
            "SELECT name, company, followers FROM people WHERE login = ?1",
            [login],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let Some((name, company, Some(followers))) = found else {
        return Ok(());
    };
    if followers < 100 {
        return Ok(());
    }
    let who = name
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| login.clone());
    let mut line = format!("{who} is followed by {}", group_digits(followers));
    if let Some(company) = company.map(|c| c.trim().trim_start_matches('@').to_string())
        && !company.is_empty()
    {
        line += &format!(" and works at {company}");
    }
    notice.body = format!("{line}.");
    if followers >= RARE {
        notice.title = format!("Rare bird: {}", notice.title);
    }
    Ok(())
}
