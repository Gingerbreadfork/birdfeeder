use rusqlite::Connection;
use serde_json::{Value, json};

use crate::apply::{self, Ctx, Fetched, InboxItem, Notice, ReleaseSet};
use crate::db::{self, EventQuery};
use crate::github::{Activity, Actor, Release, Repo, Thread, ThreadTarget};
use crate::sync;
use crate::views;

const SEEDED: &str = "2026-01-10T00:00:00Z";
const LATER: &str = "2026-01-11T00:00:00Z";
const BEFORE: &str = "2025-06-01T12:00:00Z";
const AFTER: &str = "2026-01-10T08:00:00Z";

fn ignore() -> Vec<String> {
    vec!["*.sig".into(), "latest.json".into()]
}

fn ctx<'a>(now: &'a str, ignore: &'a [String]) -> Ctx<'a> {
    Ctx {
        now,
        today: &now[..10],
        viewer: "me",
        ignore_assets: ignore,
    }
}

fn repo(stars: i64) -> Repo {
    serde_json::from_value(json!({
        "node_id": "R_1",
        "full_name": "me/tool",
        "html_url": "https://github.com/me/tool",
        "description": null,
        "private": false,
        "fork": false,
        "archived": false,
        "language": "Rust",
        "stargazers_count": stars,
        "forks_count": 0,
        "open_issues_count": 0,
        "pushed_at": "2026-01-01T00:00:00Z",
        "updated_at": "2026-01-01T00:00:00Z",
        "created_at": "2025-01-01T00:00:00Z",
        "permissions": { "push": true }
    }))
    .unwrap()
}

fn star(login: &str, at: &str) -> Value {
    json!({ "starredAt": at, "node": { "login": login, "avatarUrl": null } })
}

fn ticket(number: i64, login: &str, typename: &str, at: &str) -> Value {
    json!({
        "number": number,
        "title": format!("Ticket {number}"),
        "url": format!("https://github.com/me/tool/issues/{number}"),
        "createdAt": at,
        "author": { "__typename": typename, "login": login, "avatarUrl": null }
    })
}

fn activity(stars: Vec<Value>, issues: Vec<Value>, prs: Vec<Value>) -> Activity {
    serde_json::from_value(json!({
        "id": "R_1",
        "watchers": { "totalCount": 1 },
        "openIssues": { "totalCount": issues.len() },
        "openPullRequests": { "totalCount": prs.len() },
        "releases": { "totalCount": 0 },
        "stargazers": { "pageInfo": { "hasNextPage": false, "endCursor": null }, "edges": stars },
        "issues": { "nodes": issues },
        "pullRequests": { "nodes": prs },
        "forks": { "nodes": [] }
    }))
    .unwrap()
}

fn release(tag: &str, at: &str, assets: &[(&str, i64)]) -> Release {
    let assets: Vec<Value> = assets
        .iter()
        .map(|(name, n)| json!({ "name": name, "downloadCount": n, "size": 10 }))
        .collect();
    serde_json::from_value(json!({
        "tagName": tag,
        "name": tag,
        "url": format!("https://github.com/me/tool/releases/tag/{tag}"),
        "isDraft": false,
        "isPrerelease": false,
        "publishedAt": at,
        "author": { "__typename": "User", "login": "github-actions[bot]", "avatarUrl": null },
        "releaseAssets": { "nodes": assets }
    }))
    .unwrap()
}

fn set(nodes: Vec<Release>) -> ReleaseSet {
    ReleaseSet {
        nodes,
        complete: true,
        paged: false,
    }
}

/// Runs one sync of the single test repo and returns the notices it raised.
fn sync(
    conn: &Connection,
    now: &str,
    repo: &Repo,
    activity: Option<&Activity>,
    releases: Option<&ReleaseSet>,
) -> Vec<Notice> {
    let ignore = ignore();
    let ctx = ctx(now, &ignore);
    let stored = db::stored_repos(conn).unwrap();
    let flagged = apply::needs_activity(stored.get(&repo.node_id), repo);
    let mut notices = Vec::new();
    apply::begin(conn).unwrap();
    let fetched = Fetched {
        repo,
        stored: stored.get(&repo.node_id),
        flagged,
        activity,
        releases,
    };
    apply::repo(conn, &ctx, &fetched, &mut notices).unwrap();
    apply::finish(conn, &ctx, &stored, 0, &mut notices).unwrap();
    notices
}

fn events(conn: &Connection, kind: &str) -> Vec<db::Event> {
    db::events(
        conn,
        &EventQuery {
            kinds: vec![kind.to_string()],
            ..Default::default()
        },
    )
    .unwrap()
}

#[test]
fn first_sync_records_history_quietly() {
    let conn = db::open_in_memory().unwrap();
    let act = activity(
        vec![star("ann", BEFORE), star("bob", BEFORE)],
        vec![ticket(1, "ann", "User", BEFORE)],
        vec![],
    );
    let notices = sync(&conn, SEEDED, &repo(2), Some(&act), None);

    assert!(notices.is_empty());
    assert_eq!(events(&conn, "star").len(), 2);
    assert_eq!(events(&conn, "issue").len(), 1);
    assert_eq!(db::unread_count(&conn).unwrap(), 0);
}

#[test]
fn new_star_is_unread_and_announced() {
    let conn = db::open_in_memory().unwrap();
    let first = activity(vec![star("ann", BEFORE)], vec![], vec![]);
    sync(&conn, SEEDED, &repo(1), Some(&first), None);

    let second = activity(
        vec![star("cat", AFTER), star("ann", BEFORE)],
        vec![],
        vec![],
    );
    let notices = sync(&conn, LATER, &repo(2), Some(&second), None);

    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].title, "cat starred tool");
    assert_eq!(notices[0].body, "2 stars");
    assert_eq!(db::unread_count(&conn).unwrap(), 1);
    assert!(events(&conn, "unstar").is_empty());

    // Seeing the same star again changes nothing.
    let again = sync(&conn, LATER, &repo(2), Some(&second), None);
    assert!(again.is_empty());
    assert_eq!(events(&conn, "star").len(), 2);
}

#[test]
fn lost_and_unnamed_stars_are_counted() {
    let conn = db::open_in_memory().unwrap();
    let act = activity(vec![star("ann", BEFORE)], vec![], vec![]);
    sync(&conn, SEEDED, &repo(5), Some(&act), None);

    sync(&conn, LATER, &repo(3), Some(&act), None);
    let lost = events(&conn, "unstar");
    assert_eq!(lost.len(), 1);
    assert_eq!(lost[0].count, 2);
    assert!(lost[0].read);

    let notices = sync(&conn, "2026-01-12T00:00:00Z", &repo(7), Some(&act), None);
    assert_eq!(notices[0].title, "tool gained 4 more stars");
}

#[test]
fn own_and_bot_activity_stays_quiet() {
    let conn = db::open_in_memory().unwrap();
    sync(
        &conn,
        SEEDED,
        &repo(0),
        Some(&activity(vec![], vec![], vec![])),
        None,
    );

    let mut changed = repo(0);
    changed.open_issues_count = 3;
    let act = activity(
        vec![],
        vec![
            ticket(1, "me", "User", AFTER),
            ticket(2, "dan", "User", AFTER),
        ],
        vec![ticket(3, "dependabot", "Bot", AFTER)],
    );
    let notices = sync(&conn, LATER, &changed, Some(&act), None);

    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].title, "New issue in tool");
    assert_eq!(notices[0].body, "dan opened #2: Ticket 2");
    assert_eq!(db::unread_count(&conn).unwrap(), 1);
    assert_eq!(events(&conn, "pr").len(), 1);
}

#[test]
fn failed_activity_read_is_retried() {
    let conn = db::open_in_memory().unwrap();
    sync(
        &conn,
        SEEDED,
        &repo(1),
        Some(&activity(vec![], vec![], vec![])),
        None,
    );

    // The star count changed but the detail request failed.
    sync(&conn, LATER, &repo(2), None, None);
    let stored = db::stored_repos(&conn).unwrap();
    assert_eq!(stored["R_1"].stars, 1);
    assert!(!stored["R_1"].gone);
    assert!(apply::needs_activity(stored.get("R_1"), &repo(2)));
}

#[test]
fn downloads_are_tallied_per_day_without_ignored_assets() {
    let conn = db::open_in_memory().unwrap();
    let act = activity(vec![], vec![], vec![]);
    let v1 = |app, sig| {
        release(
            "v1",
            BEFORE,
            &[("app.tar.gz", app), ("app.tar.gz.sig", sig)],
        )
    };

    let notices = sync(
        &conn,
        SEEDED,
        &repo(0),
        Some(&act),
        Some(&set(vec![v1(40, 40)])),
    );
    assert!(notices.is_empty());
    assert!(events(&conn, "downloads").is_empty());
    assert_eq!(db::stored_repos(&conn).unwrap()["R_1"].downloads, 40);

    let notices = sync(
        &conn,
        "2026-01-11T09:00:00Z",
        &repo(0),
        None,
        Some(&set(vec![v1(43, 90)])),
    );
    assert_eq!(notices[0].title, "tool: 3 new downloads");
    assert_eq!(notices[0].body, "v1 +3");

    sync(
        &conn,
        "2026-01-11T10:00:00Z",
        &repo(0),
        None,
        Some(&set(vec![v1(45, 91)])),
    );
    let tally = events(&conn, "downloads");
    assert_eq!(tally.len(), 1);
    assert_eq!(tally[0].count, 5);
    assert_eq!(tally[0].meta.as_ref().unwrap()["releases"]["v1"], 5);
    assert!(!tally[0].read);
    assert_eq!(db::stored_repos(&conn).unwrap()["R_1"].downloads, 45);
}

#[test]
fn new_release_counts_its_first_downloads() {
    let conn = db::open_in_memory().unwrap();
    let act = activity(vec![], vec![], vec![]);
    let v1 = release("v1", BEFORE, &[("app.tar.gz", 98)]);
    sync(
        &conn,
        SEEDED,
        &repo(0),
        Some(&act),
        Some(&set(vec![v1.clone()])),
    );

    let v2 = release("v2", AFTER, &[("app.tar.gz", 4)]);
    let notices = sync(&conn, LATER, &repo(0), None, Some(&set(vec![v2, v1])));

    let titles: Vec<&str> = notices.iter().map(|n| n.title.as_str()).collect();
    assert_eq!(
        titles,
        ["tool: 4 new downloads", "tool reached 100 downloads"]
    );
    // The release itself was published by a bot, so it is recorded as read.
    assert!(events(&conn, "release").iter().all(|e| e.read));
    assert_eq!(events(&conn, "release").len(), 2);
}

#[test]
fn deleted_release_is_dropped_from_totals() {
    let conn = db::open_in_memory().unwrap();
    let act = activity(vec![], vec![], vec![]);
    let v1 = release("v1", BEFORE, &[("app.tar.gz", 10)]);
    let v2 = release("v2", BEFORE, &[("app.tar.gz", 5)]);
    sync(
        &conn,
        SEEDED,
        &repo(0),
        Some(&act),
        Some(&set(vec![v2.clone(), v1])),
    );
    sync(&conn, LATER, &repo(0), None, Some(&set(vec![v2])));
    assert_eq!(db::stored_repos(&conn).unwrap()["R_1"].downloads, 5);
}

#[test]
fn star_milestone_is_reported_once() {
    let conn = db::open_in_memory().unwrap();
    let act = activity(vec![], vec![], vec![]);
    sync(&conn, SEEDED, &repo(9), Some(&act), None);
    sync(&conn, LATER, &repo(11), Some(&act), None);
    sync(&conn, "2026-01-12T00:00:00Z", &repo(9), Some(&act), None);
    sync(&conn, "2026-01-13T00:00:00Z", &repo(10), Some(&act), None);

    let marks = events(&conn, "milestone");
    assert_eq!(marks.len(), 1);
    assert_eq!(marks[0].count, 10);
}

fn actor(login: &str) -> Actor {
    serde_json::from_value(json!({ "login": login, "avatarUrl": null })).unwrap()
}

#[test]
fn followers_are_compared_with_the_stored_list() {
    let conn = db::open_in_memory().unwrap();
    let ignore = ignore();
    let mut notices = Vec::new();

    let first = [actor("ann"), actor("bob")];
    apply::followers(&conn, &ctx(SEEDED, &ignore), &first, true, 2, &mut notices).unwrap();
    assert!(notices.is_empty());
    assert!(events(&conn, "follower").is_empty());

    let second = [actor("ann"), actor("cat")];
    apply::followers(&conn, &ctx(LATER, &ignore), &second, true, 2, &mut notices).unwrap();
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].title, "cat followed you");
    assert_eq!(events(&conn, "unfollow")[0].actor.as_deref(), Some("bob"));

    // A list that was cut short cannot prove anyone left.
    apply::followers(
        &conn,
        &ctx(LATER, &ignore),
        &[actor("ann")],
        false,
        2,
        &mut notices,
    )
    .unwrap();
    assert_eq!(events(&conn, "unfollow").len(), 1);
}

fn thread(id: &str, reason: &str, updated_at: &str, number: i64, comment: Option<&str>) -> Thread {
    let subject = format!("https://api.github.com/repos/me/tool/issues/{number}");
    serde_json::from_value(json!({
        "id": id,
        "reason": reason,
        "updated_at": updated_at,
        "subject": {
            "title": format!("Ticket {number}"),
            "url": subject,
            "latest_comment_url": comment.map(str::to_owned).unwrap_or(subject.clone()),
            "type": "Issue"
        },
        "repository": {
            "node_id": "R_1",
            "full_name": "me/tool",
            "html_url": "https://github.com/me/tool"
        }
    }))
    .unwrap()
}

#[test]
fn inbox_skips_echoes_and_reports_comments() {
    let conn = db::open_in_memory().unwrap();
    let ignore = ignore();
    sync(
        &conn,
        SEEDED,
        &repo(0),
        Some(&activity(vec![], vec![], vec![])),
        None,
    );
    let mut notices = Vec::new();
    apply::inbox(&conn, &ctx(SEEDED, &ignore), &[], &mut notices).unwrap();

    let mut changed = repo(0);
    changed.open_issues_count = 1;
    let act = activity(vec![], vec![ticket(7, "dan", "User", AFTER)], vec![]);
    sync(&conn, LATER, &changed, Some(&act), None);

    let comment: ThreadTarget = serde_json::from_value(json!({
        "html_url": "https://github.com/me/tool/issues/7#issuecomment-1",
        "body": "Same   here,\nplease fix",
        "user": { "login": "eve", "avatar_url": null }
    }))
    .unwrap();
    let items = [
        // Arrives seconds after the issue was opened and says nothing new.
        InboxItem {
            thread: thread("t1", "subscribed", "2026-01-10T08:00:20Z", 7, None),
            target: None,
        },
        InboxItem {
            thread: thread(
                "t1",
                "mention",
                "2026-01-10T09:00:00Z",
                7,
                Some("https://api.github.com/repos/me/tool/issues/comments/1"),
            ),
            target: Some(comment),
        },
    ];
    apply::inbox(&conn, &ctx(LATER, &ignore), &items, &mut notices).unwrap();

    let inbox = events(&conn, "inbox");
    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0].actor.as_deref(), Some("eve"));
    assert_eq!(
        inbox[0].url.as_deref(),
        Some("https://github.com/me/tool/issues/7#issuecomment-1")
    );
    assert_eq!(
        inbox[0].meta.as_ref().unwrap()["snippet"],
        "Same here, please fix"
    );
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].title, "You were mentioned in tool");
    assert_eq!(notices[0].body, "eve: Same here, please fix");
}

#[test]
fn marking_read_follows_the_filters() {
    let conn = db::open_in_memory().unwrap();
    for (who, at) in [
        ("ann", "2026-01-03T12:00:00Z"),
        ("bob", "2026-01-04T12:00:00Z"),
    ] {
        db::insert_event(
            &conn,
            &apply::star_event_for("R_1", "me/tool", &actor(who), at, false),
        )
        .unwrap();
        db::insert_event(
            &conn,
            &apply::star_event_for("R_2", "me/other", &actor(who), at, false),
        )
        .unwrap();
    }
    let day = events(&conn, "star")
        .iter()
        .map(|e| e.occurred_at.clone())
        .min()
        .map(|at| {
            chrono::DateTime::parse_from_rfc3339(&at)
                .unwrap()
                .with_timezone(&chrono::Local)
                .format("%Y-%m-%d")
                .to_string()
        });

    let narrow = EventQuery {
        kinds: vec!["star".into()],
        repo_id: Some("R_1".into()),
        day,
        ..Default::default()
    };
    db::mark_matching_read(&conn, &narrow).unwrap();
    assert_eq!(db::unread_count(&conn).unwrap(), 3);

    db::mark_matching_read(&conn, &EventQuery::default()).unwrap();
    assert_eq!(db::unread_count(&conn).unwrap(), 0);
}

#[test]
fn a_sighting_rides_on_the_unread_event_that_caused_it() {
    use crate::highlights::note_sightings;

    let conn = db::open_in_memory().unwrap();
    let ignore = ignore();
    sign_in(&conn, 18);
    sync(
        &conn,
        SEEDED,
        &repo(1),
        Some(&activity(vec![], vec![], vec![])),
        None,
    );
    let mut notices = Vec::new();
    note_sightings(&conn, &ctx(SEEDED, &ignore), &mut notices).unwrap();

    db::insert_event(
        &conn,
        &apply::star_event_for("R_1", "me/tool", &actor("hawk"), &noon(0), false),
    )
    .unwrap();
    know(&conn, "hawk", 4_200, None);
    note_sightings(&conn, &ctx(LATER, &ignore), &mut notices).unwrap();

    let titles: Vec<&str> = notices.iter().map(|n| n.title.as_str()).collect();
    assert!(titles.contains(&"New on your life list: Rare bird"));
    assert!(
        events(&conn, "sighting")
            .iter()
            .all(|e| e.subject.as_deref() != Some("Rare bird"))
    );
    let star = &events(&conn, "star")[0];
    assert_eq!(star.meta.as_ref().unwrap()["sighting"], "Rare bird");
}

#[test]
fn glob_patterns() {
    assert!(apply::glob_match("*.sig", "App_1.0_amd64.deb.SIG"));
    assert!(apply::glob_match("latest.json", "latest.json"));
    assert!(apply::glob_match("latest*.yml", "latest-mac.yml"));
    assert!(apply::glob_match("SHA256SUMS*", "SHA256SUMS"));
    assert!(apply::glob_match("*", "anything"));
    assert!(!apply::glob_match("*.sig", "app.signature"));
    assert!(!apply::glob_match("latest.json", "latest.json.bak"));
    assert!(!apply::glob_match("a*b*c", "ac"));
    assert!(apply::glob_match("a*b*c", "a-b-c"));
    // A name shorter in bytes than the pattern's ends must not be cut mid-character.
    assert!(!apply::glob_match("*.sig", "é.gz"));
    assert!(!apply::glob_match("ab*cd", "aé"));
    assert!(apply::glob_match("*.sig", "résumé.sig"));
}

/// Runs one sync over several repos that all report the same activity.
fn sync_many(conn: &Connection, now: &str, repos: &[Repo], act: &Activity) -> Vec<Notice> {
    let ignore = ignore();
    let ctx = ctx(now, &ignore);
    let stored = db::stored_repos(conn).unwrap();
    let mut notices = Vec::new();
    apply::begin(conn).unwrap();
    for repo in repos {
        let fetched = Fetched {
            repo,
            stored: stored.get(&repo.node_id),
            flagged: apply::needs_activity(stored.get(&repo.node_id), repo),
            activity: Some(act),
            releases: None,
        };
        apply::repo(conn, &ctx, &fetched, &mut notices).unwrap();
    }
    apply::finish(conn, &ctx, &stored, 0, &mut notices).unwrap();
    notices
}

fn second_repo(stars: i64) -> Repo {
    let mut other = repo(stars);
    other.node_id = "R_2".into();
    other.full_name = "me/other".into();
    other
}

#[test]
fn a_repo_that_joins_with_stars_is_not_a_milestone() {
    let conn = db::open_in_memory().unwrap();
    let act = activity(vec![], vec![], vec![]);
    sync(&conn, SEEDED, &repo(60), Some(&act), None);

    let joined = sync_many(&conn, LATER, &[repo(60), second_repo(50)], &act);
    assert!(joined.is_empty());
    assert!(events(&conn, "milestone").is_empty());

    // Once both are watched, stars that arrive do add up to a mark.
    let grown = sync_many(
        &conn,
        "2026-01-12T00:00:00Z",
        &[repo(60), second_repo(140)],
        &act,
    );
    let titles: Vec<&str> = grown.iter().map(|n| n.title.as_str()).collect();
    assert!(titles.contains(&"other reached 100 stars"), "{titles:?}");
    let grown = sync_many(
        &conn,
        "2026-01-13T00:00:00Z",
        &[repo(120), second_repo(140)],
        &act,
    );
    let titles: Vec<&str> = grown.iter().map(|n| n.title.as_str()).collect();
    assert!(
        titles.iter().any(|t| t.contains("250 stars across")),
        "{titles:?}"
    );
}

#[test]
fn a_repo_that_comes_back_starts_over_quietly() {
    let conn = db::open_in_memory().unwrap();
    sync(
        &conn,
        SEEDED,
        &repo(5),
        Some(&activity(vec![], vec![], vec![])),
        None,
    );

    // A sync that does not list the repo leaves it marked as gone.
    apply::begin(&conn).unwrap();
    assert!(db::stored_repos(&conn).unwrap()["R_1"].gone);

    let while_away = activity(vec![star("ann", AFTER)], vec![], vec![]);
    let back = sync(&conn, LATER, &repo(12), Some(&while_away), None);
    assert!(
        back.is_empty(),
        "{:?}",
        back.iter().map(|n| &n.title).collect::<Vec<_>>()
    );
    assert!(events(&conn, "milestone").is_empty());
    assert_eq!(db::unread_count(&conn).unwrap(), 0);
    assert_eq!(
        db::stored_repos(&conn).unwrap()["R_1"].seeded_at.as_deref(),
        Some(LATER)
    );

    let fresh = activity(vec![star("bob", "2026-01-11T09:00:00Z")], vec![], vec![]);
    let next = sync(&conn, "2026-01-12T00:00:00Z", &repo(13), Some(&fresh), None);
    assert_eq!(next[0].title, "bob starred tool");
}

#[test]
fn the_star_count_comes_from_the_same_answer_as_the_names() {
    let conn = db::open_in_memory().unwrap();
    sync(
        &conn,
        SEEDED,
        &repo(5),
        Some(&activity(vec![], vec![], vec![])),
        None,
    );

    // The repo list still says five when the stargazers already include a sixth.
    let mut ahead = activity(vec![star("ann", AFTER)], vec![], vec![]);
    ahead.stargazer_count = Some(6);
    let first = sync(&conn, LATER, &repo(5), Some(&ahead), None);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].title, "ann starred tool");
    assert!(events(&conn, "unstar").is_empty());
    assert_eq!(db::stored_repos(&conn).unwrap()["R_1"].stars, 6);

    let caught_up = sync(&conn, "2026-01-12T00:00:00Z", &repo(6), Some(&ahead), None);
    assert!(caught_up.is_empty());
    assert_eq!(events(&conn, "star").len(), 1);
}

#[test]
fn a_second_account_is_refused() {
    let conn = db::open_in_memory().unwrap();
    let user = |id: i64, login: &str| -> crate::github::User {
        serde_json::from_value(json!({
            "id": id, "login": login, "name": null, "avatar_url": "", "html_url": "",
            "followers": 0, "following": 0
        }))
        .unwrap()
    };
    assert!(
        sync::same_account(&conn, &user(1, "me")).is_ok(),
        "nothing stored yet"
    );

    db::meta_set(
        &conn,
        "account",
        &serde_json::to_string(&user(1, "me")).unwrap(),
    )
    .unwrap();
    assert!(sync::same_account(&conn, &user(1, "renamed")).is_ok());
    let refused = sync::same_account(&conn, &user(2, "other")).unwrap_err();
    assert!(refused.is::<sync::OtherAccount>());
    assert_eq!(
        refused.to_string(),
        "Birdfeeder has been watching me, and this token belongs to other."
    );

    // Data stored before ids were kept is matched by login.
    sign_in(&conn, 0);
    assert!(sync::same_account(&conn, &user(1, "Me")).is_ok());
    assert!(sync::same_account(&conn, &user(2, "other")).is_err());
}

#[test]
fn starting_over_keeps_only_the_settings() {
    let mut conn = db::open_in_memory().unwrap();
    let mut settings = crate::settings::Settings::load(&conn);
    settings.poll_minutes = 30;
    settings.save(&conn).unwrap();
    sign_in(&conn, 3);
    sync(
        &conn,
        SEEDED,
        &repo(5),
        Some(&activity(vec![star("ann", BEFORE)], vec![], vec![])),
        None,
    );

    db::forget_account(&mut conn).unwrap();
    assert!(db::stored_repos(&conn).unwrap().is_empty());
    assert!(events(&conn, "star").is_empty());
    assert!(db::meta_get(&conn, "account").unwrap().is_none());
    assert_eq!(crate::settings::Settings::load(&conn).poll_minutes, 30);
}

#[test]
fn a_restart_comes_back_the_way_the_window_was_left() {
    let conn = db::open_in_memory().unwrap();
    assert_eq!(db::take_window_note(&conn).unwrap(), None);

    db::note_window(&conn, false).unwrap();
    assert_eq!(db::take_window_note(&conn).unwrap(), Some(false));
    assert_eq!(
        db::take_window_note(&conn).unwrap(),
        None,
        "a note is used once"
    );

    db::note_window(&conn, true).unwrap();
    assert_eq!(db::take_window_note(&conn).unwrap(), Some(true));

    // A note left behind by a restart that never happened is not believed later.
    db::meta_set(&conn, "window_before_restart", "hidden 1000").unwrap();
    assert_eq!(db::take_window_note(&conn).unwrap(), None);
}

#[test]
fn settings_saved_by_an_older_version_keep_their_values() {
    let conn = db::open_in_memory().unwrap();
    db::meta_set(
        &conn,
        "settings",
        r#"{"poll_minutes": 30, "keep_running": false}"#,
    )
    .unwrap();
    let settings = crate::settings::Settings::load(&conn);
    assert_eq!(settings.poll_minutes, 30);
    assert!(!settings.keep_running);
    assert!(settings.check_updates && !settings.install_updates);
}

#[test]
fn the_list_pages_past_events_that_share_a_time() {
    let conn = db::open_in_memory().unwrap();
    for n in 0..7 {
        star_at(&conn, &format!("fan{n}"), AFTER);
    }
    let mut seen = Vec::new();
    let mut before = None;
    loop {
        let page = db::events(
            &conn,
            &EventQuery {
                before: before.clone(),
                limit: Some(3),
                ..Default::default()
            },
        )
        .unwrap();
        let Some(last) = page.last() else { break };
        before = Some((last.occurred_at.clone(), last.id));
        seen.extend(page.iter().map(|e| e.id));
    }
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen.len(), 7);

    assert_eq!(db::mark_seen(&conn).unwrap(), *seen.last().unwrap());
    assert_eq!(db::marks(&conn).unwrap().seen, *seen.last().unwrap());
}

#[test]
fn star_history_ends_at_the_current_total() {
    let conn = db::open_in_memory().unwrap();
    // Three stars now, but only two stargazers are named.
    let act = activity(
        vec![
            star("bob", "2025-08-01T12:00:00Z"),
            star("ann", "2025-06-01T12:00:00Z"),
        ],
        vec![],
        vec![],
    );
    sync(&conn, SEEDED, &repo(3), Some(&act), None);

    let overview = views::overview(&conn, 91).unwrap();
    let values: Vec<i64> = overview.star_series.iter().map(|p| p.value).collect();
    assert_eq!(values, [2, 3, 3]);
    assert_eq!(overview.totals.stars, 3);
    assert_eq!(views::repos(&conn).unwrap().len(), 1);
}

#[test]
fn a_burst_of_views_is_reported_once() {
    use crate::github::{Referrer, Traffic, TrafficPoint};
    use chrono::{Duration, Utc};

    let conn = db::open_in_memory().unwrap();
    let ignore = ignore();
    let ctx = ctx(LATER, &ignore);
    sync(
        &conn,
        SEEDED,
        &repo(0),
        Some(&activity(vec![], vec![], vec![])),
        None,
    );

    let day = |ago: i64, count: i64| TrafficPoint {
        timestamp: (Utc::now() - Duration::days(ago))
            .format("%Y-%m-%dT00:00:00Z")
            .to_string(),
        count,
        uniques: count / 2,
    };
    let read = |today: i64| Traffic {
        views: (0..14)
            .map(|ago| day(ago, if ago == 0 { today } else { 6 }))
            .collect(),
        clones: Vec::new(),
        referrers: vec![Referrer {
            referrer: "news.ycombinator.com".into(),
            count: 180,
            uniques: 150,
        }],
    };
    let mut notices = Vec::new();
    let record = |traffic: &Traffic, notices: &mut Vec<Notice>| {
        apply::traffic(
            &conn,
            &ctx,
            "R_1",
            "me/tool",
            "https://github.com/me/tool",
            traffic,
            notices,
        )
        .unwrap();
    };

    // The first reading has nothing to compare with, and an ordinary day is not news.
    record(&read(240), &mut notices);
    record(&read(9), &mut notices);
    assert!(notices.is_empty());

    record(&read(240), &mut notices);
    record(&read(260), &mut notices);
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].title, "tool is getting more visitors");
    assert_eq!(
        notices[0].body,
        "240 views today, mostly from news.ycombinator.com"
    );
    let bursts = events(&conn, "traffic");
    assert_eq!(bursts.len(), 1);
    assert_eq!(bursts[0].count, 260);
}

/// Records a star on the test repo at an exact time.
fn star_at(conn: &Connection, who: &str, at: &str) {
    db::insert_event(
        conn,
        &apply::star_event_for("R_1", "me/tool", &actor(who), at, true),
    )
    .unwrap();
}

fn know(conn: &Connection, login: &str, followers: i64, company: Option<&str>) {
    conn.execute(
        "INSERT INTO people (login, company, followers, fetched_at) VALUES (?1, ?2, ?3, '')",
        rusqlite::params![login, company, followers],
    )
    .unwrap();
}

fn sign_in(conn: &Connection, followers: i64) {
    let account = json!({
        "login": "me", "name": null, "avatar_url": "", "html_url": "",
        "followers": followers, "following": 0
    });
    db::meta_set(conn, "account", &account.to_string()).unwrap();
}

/// A timestamp `days` before now, at noon so the local day is not in doubt.
fn noon(days: i64) -> String {
    (chrono::Local::now() - chrono::Duration::days(days))
        .format("%Y-%m-%dT12:00:00%:z")
        .to_string()
}

#[test]
fn goals_start_at_the_last_milestone_and_estimate_the_pace() {
    let conn = db::open_in_memory().unwrap();
    sign_in(&conn, 18);
    sync(
        &conn,
        SEEDED,
        &repo(37),
        Some(&activity(vec![], vec![], vec![])),
        None,
    );
    for n in 0..15 {
        star_at(&conn, &format!("fan{n}"), &noon(n));
    }

    let goals = crate::highlights::goals(&conn, 5).unwrap();
    let stars = goals.iter().find(|g| g.what == "stars").unwrap();
    assert_eq!((stars.from, stars.current, stars.target), (25, 37, 50));
    // Thirteen to go at fifteen a month.
    assert_eq!(stars.eta_days, Some(26));
    let followers = goals.iter().find(|g| g.what == "followers").unwrap();
    assert_eq!(
        (followers.from, followers.target, followers.eta_days),
        (10, 25, None)
    );
    // With a single starred repo the account total would only repeat it.
    assert!(goals.iter().all(|g| g.what != "all-stars"));
}

#[test]
fn records_and_streaks_come_from_star_days() {
    let conn = db::open_in_memory().unwrap();
    for (days_ago, stars) in [(12, 1), (11, 4), (10, 2), (5, 1), (1, 2), (0, 1)] {
        for n in 0..stars {
            star_at(&conn, &format!("fan{days_ago}-{n}"), &noon(days_ago));
        }
    }
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let records = crate::highlights::records(&conn, &today).unwrap();
    assert_eq!(records.best_day.unwrap().count, 4);
    assert_eq!(records.best_star_streak, 3);
    assert_eq!(records.star_streak, 2);
}

#[test]
fn life_list_ticks_once_and_tracks_progress() {
    use crate::highlights::{life_list, note_sightings};

    let conn = db::open_in_memory().unwrap();
    let ignore = ignore();
    sign_in(&conn, 18);
    sync(
        &conn,
        SEEDED,
        &repo(8),
        Some(&activity(vec![], vec![], vec![])),
        None,
    );
    star_at(&conn, "ann", &noon(3));
    let mut notices = Vec::new();
    note_sightings(&conn, &ctx(SEEDED, &ignore), &mut notices).unwrap();
    assert!(notices.is_empty(), "what was already true is not news");

    let entry = |id: &str| {
        life_list(&conn)
            .unwrap()
            .into_iter()
            .find(|s| s.id == id)
            .unwrap()
    };
    assert!(entry("first-visitor").seen);
    assert_eq!(
        entry("first-visitor").detail.as_deref(),
        Some("ann, in tool")
    );
    assert!(!entry("rare-bird").seen);
    assert_eq!(entry("flock").progress, Some((1, 10)));
    assert_eq!(entry("a-following").progress, Some((18, 100)));

    db::insert_event(
        &conn,
        &apply::star_event_for("R_1", "me/tool", &actor("hawk"), &noon(0), false),
    )
    .unwrap();
    know(&conn, "hawk", 4_200, None);
    note_sightings(&conn, &ctx(LATER, &ignore), &mut notices).unwrap();
    note_sightings(&conn, &ctx(LATER, &ignore), &mut notices).unwrap();
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].title, "New on your life list: Rare bird");
    assert_eq!(notices[0].body, "hawk, 4,200 followers");
}

#[test]
fn history_that_arrives_late_is_ticked_without_a_word() {
    use crate::highlights::{life_list, note_sightings};

    let conn = db::open_in_memory().unwrap();
    let ignore = ignore();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let now = sync::now_utc();
    let ctx = Ctx {
        now: &now,
        today: &today,
        viewer: "me",
        ignore_assets: &ignore,
    };
    sign_in(&conn, 18);
    sync(
        &conn,
        SEEDED,
        &repo(8),
        Some(&activity(vec![], vec![], vec![])),
        None,
    );
    star_at(&conn, "ann", &noon(90));
    let mut notices = Vec::new();
    note_sightings(&conn, &ctx, &mut notices).unwrap();

    // An old stargazer's profile is read on a later sync, and older stars are paged in.
    know(&conn, "ann", 52_000, None);
    for n in 0..10 {
        star_at(&conn, &format!("early{n}"), &noon(40));
    }
    note_sightings(&conn, &ctx, &mut notices).unwrap();
    assert!(
        notices.is_empty(),
        "{:?}",
        notices.iter().map(|n| &n.title).collect::<Vec<_>>()
    );
    let seen = |id: &str| {
        life_list(&conn)
            .unwrap()
            .into_iter()
            .any(|s| s.id == id && s.seen)
    };
    assert!(seen("very-rare-bird") && seen("flock"));
    assert!(events(&conn, "sighting").is_empty());

    // Fifty stars today is news whoever they came from.
    for n in 0..50 {
        star_at(&conn, &format!("crowd{n}"), &noon(0));
    }
    note_sightings(&conn, &ctx, &mut notices).unwrap();
    let titles: Vec<&str> = notices.iter().map(|n| n.title.as_str()).collect();
    assert_eq!(titles, ["New on your life list: Murmuration"]);
}

#[test]
fn a_best_day_and_a_busy_week_are_announced() {
    use crate::highlights::{note_record, note_trends};

    let conn = db::open_in_memory().unwrap();
    let ignore = ignore();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let now = sync::now_utc();
    let ctx = Ctx {
        now: &now,
        today: &today,
        viewer: "me",
        ignore_assets: &ignore,
    };
    sync(
        &conn,
        SEEDED,
        &repo(0),
        Some(&activity(vec![], vec![], vec![])),
        None,
    );
    for n in 0..4 {
        star_at(&conn, &format!("old{n}"), &noon(20));
    }
    let mut notices = Vec::new();
    note_record(&conn, &ctx, &mut notices).unwrap();
    note_trends(&conn, &ctx, &mut notices).unwrap();
    assert!(notices.is_empty());

    for n in 0..6 {
        star_at(&conn, &format!("new{n}"), &noon(0));
    }
    note_record(&conn, &ctx, &mut notices).unwrap();
    note_trends(&conn, &ctx, &mut notices).unwrap();
    note_record(&conn, &ctx, &mut notices).unwrap();
    note_trends(&conn, &ctx, &mut notices).unwrap();

    let titles: Vec<&str> = notices.iter().map(|n| n.title.as_str()).collect();
    assert_eq!(titles, ["A new best day: 6 stars", "tool is picking up"]);
    assert_eq!(notices[0].body, "The record was 4.");
    assert_eq!(
        notices[1].body,
        "6 stars this week, up from about 1 a week."
    );

    // The same burst is still in the window when the next week begins.
    let next_week = (chrono::Local::now() + chrono::Duration::days(7))
        .format("%Y-%m-%d")
        .to_string();
    let later = Ctx {
        today: &next_week,
        ..ctx
    };
    notices.clear();
    note_trends(&conn, &later, &mut notices).unwrap();
    assert!(notices.is_empty());
    assert_eq!(events(&conn, "trend").len(), 1);
}

#[test]
fn a_record_is_measured_against_history_read_in_later() {
    use crate::highlights::note_record;

    let conn = db::open_in_memory().unwrap();
    let ignore = ignore();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let now = sync::now_utc();
    let ctx = Ctx {
        now: &now,
        today: &today,
        viewer: "me",
        ignore_assets: &ignore,
    };
    for n in 0..4 {
        star_at(&conn, &format!("old{n}"), &noon(20));
    }
    let mut notices = Vec::new();
    note_record(&conn, &ctx, &mut notices).unwrap();

    // An older and better day turns up after the record of four was written down.
    for n in 0..9 {
        star_at(&conn, &format!("launch{n}"), &noon(200));
    }
    for n in 0..6 {
        star_at(&conn, &format!("new{n}"), &noon(0));
    }
    note_record(&conn, &ctx, &mut notices).unwrap();
    assert!(notices.is_empty());

    for n in 6..10 {
        star_at(&conn, &format!("new{n}"), &noon(0));
    }
    note_record(&conn, &ctx, &mut notices).unwrap();
    assert_eq!(notices[0].title, "A new best day: 10 stars");
    assert_eq!(notices[0].body, "The record was 9.");
}

#[test]
fn well_followed_visitors_are_introduced() {
    let conn = db::open_in_memory().unwrap();
    know(&conn, "hawk", 18_204, Some("@falconry"));
    know(&conn, "wren", 12, None);
    let notice = |who: &str| Notice {
        kind: "star",
        title: format!("{who} starred tool"),
        body: "38 stars".into(),
        actor: Some(who.into()),
    };

    let mut rare = notice("hawk");
    crate::highlights::introduce(&conn, &mut rare).unwrap();
    assert_eq!(rare.title, "Rare bird: hawk starred tool");
    assert_eq!(
        rare.body,
        "hawk is followed by 18,204 and works at falconry."
    );

    let mut common = notice("wren");
    crate::highlights::introduce(&conn, &mut common).unwrap();
    assert_eq!(common.title, "wren starred tool");
    assert_eq!(common.body, "38 stars");
}

#[test]
fn stories_are_reported_when_new_and_when_they_climb() {
    use crate::hackernews::Story;
    use crate::highlights::note_stories;

    let conn = db::open_in_memory().unwrap();
    let ignore = ignore();
    sync(
        &conn,
        SEEDED,
        &repo(0),
        Some(&activity(vec![], vec![], vec![])),
        None,
    );
    let story = |id: &str, posted_at: &str, points| Story {
        id: id.into(),
        title: "Show HN: Tool".into(),
        repo: "Me/Tool".into(),
        points,
        comments: 3,
        posted_at: posted_at.into(),
    };
    let mut notices = Vec::new();
    note_stories(
        &conn,
        &ctx(SEEDED, &ignore),
        &[story("1", BEFORE, 40)],
        &mut notices,
    )
    .unwrap();
    assert!(notices.is_empty());

    let later = [story("1", BEFORE, 41), story("2", AFTER, 12)];
    note_stories(&conn, &ctx(LATER, &ignore), &later, &mut notices).unwrap();
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].title, "tool is on Hacker News");

    let climbed = [story("2", AFTER, 140)];
    note_stories(&conn, &ctx(LATER, &ignore), &climbed, &mut notices).unwrap();
    assert_eq!(notices[1].title, "tool passed 100 points on Hacker News");
    let stored = events(&conn, "story");
    assert_eq!(stored.len(), 2);
    assert_eq!(stored.iter().map(|e| e.count).max(), Some(140));
}

#[test]
fn a_contribution_streak_is_counted_and_celebrated() {
    use crate::highlights::{contribution_streak, note_streak};

    let conn = db::open_in_memory().unwrap();
    let ignore = ignore();
    // Ten days back is a day off, then a run that stops `gap` days short of today.
    let fill = |gap: i64, today: i64| {
        conn.execute("DELETE FROM contributions", []).unwrap();
        for ago in 0..=10i64 {
            let count = match ago {
                0 => today,
                10 => 0,
                _ if ago <= gap => 0,
                _ => 2,
            };
            conn.execute(
                "INSERT INTO contributions (date, count) VALUES (date('now', ?1), ?2)",
                rusqlite::params![format!("-{ago} days"), count],
            )
            .unwrap();
        }
    };
    let mut notices = Vec::new();

    fill(3, 0);
    assert_eq!(contribution_streak(&conn).unwrap().unwrap().current, 0);
    fill(0, 0);
    let streak = contribution_streak(&conn).unwrap().unwrap();
    assert_eq!((streak.current, streak.best, streak.today), (9, 9, 0));
    note_streak(&conn, &ctx(SEEDED, &ignore), &mut notices).unwrap();
    assert!(notices.is_empty());

    fill(0, 5);
    assert_eq!(contribution_streak(&conn).unwrap().unwrap().current, 10);
    conn.execute("UPDATE meta SET value = '6' WHERE key = 'streak_seen'", [])
        .unwrap();
    note_streak(&conn, &ctx(LATER, &ignore), &mut notices).unwrap();
    assert_eq!(notices[0].title, "You reached a 7-day streak");
}

#[test]
fn sample_data_fills_every_screen() {
    let conn = db::open_in_memory().unwrap();
    crate::demo::fill(&conn).unwrap();

    let overview = views::overview(&conn, 91).unwrap();
    assert_eq!(overview.account.unwrap().login, "robin");
    assert_eq!(overview.totals.stars, 2065);
    assert_eq!(overview.star_series.last().unwrap().value, 2065);
    assert!(overview.week.stars > 0);
    assert_eq!(db::unread_count(&conn).unwrap(), 6);

    let ignore = crate::settings::Settings::default().ignore_assets;
    let downloads = views::downloads(&conn, &ignore).unwrap();
    assert_eq!(downloads.repos.len(), 3);
    assert!(downloads.gained.len() >= 45);
    let detail = views::repo_detail(&conn, "demo:seedcake", &ignore)
        .unwrap()
        .unwrap();
    assert_eq!(detail.releases.len(), 6);
    assert_eq!(detail.traffic.len(), 30);
    for kind in [
        "star",
        "issue",
        "pr",
        "fork",
        "follower",
        "inbox",
        "release",
        "downloads",
    ] {
        assert!(!events(&conn, kind).is_empty(), "no sample {kind} events");
    }

    assert_eq!(overview.streak.unwrap().current, 23);
    assert!(!overview.goals.is_empty());
    assert!(overview.records.best_day.unwrap().count >= 50);
    let list = views::life_list(&conn).unwrap();
    let seen = |id: &str| list.sightings.iter().find(|s| s.id == id).unwrap().seen;
    assert!(seen("rare-bird") && seen("century") && seen("front-page"));
    assert!(seen("very-rare-bird"), "the newest star carries this one");
    assert!(!seen("a-following") && !seen("creature-of-habit"));
    assert_eq!(list.visitors[0].login, "kestrel");
}

/// The app and a phone's background check can open the file at the same moment after an
/// update, and only one of them may apply the update's schema changes.
#[test]
fn two_connections_migrate_a_database_only_once() {
    let dir = std::env::temp_dir().join(format!("birdfeeder-migrate-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for round in 0..20 {
        let path = dir.join(format!("{round}.db"));
        let conn = db::open(&path).unwrap();
        conn.execute_batch(
            "ALTER TABLE repos DROP COLUMN traffic_tried_at; PRAGMA user_version = 0;",
        )
        .unwrap();
        drop(conn);
        let openers: Vec<_> = (0..2)
            .map(|_| {
                let path = path.clone();
                std::thread::spawn(move || db::open(&path).map(|_| ()))
            })
            .collect();
        for opener in openers {
            opener.join().unwrap().expect("both connections open");
        }
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_release_is_newer_by_its_numbers_not_its_text() {
    use crate::commands::newer;
    assert!(newer("0.1.10", "0.1.9"));
    assert!(newer("0.2.0", "0.1.12"));
    assert!(newer("v1.0.0", "0.9.9"));
    assert!(!newer("0.1.1", "0.1.1"));
    assert!(!newer("0.1.0", "0.1.1"));
    assert!(!newer("0.1.1-beta", "0.1.1"));
}

#[test]
fn a_card_is_named_after_its_repository() {
    use crate::commands::card_stem;
    assert_eq!(card_stem("seedcake"), "seedcake-card");
    assert_eq!(card_stem("my.tool_2"), "my.tool_2-card");
    assert_eq!(card_stem("../../etc/passwd"), "etc-passwd-card");
    assert_eq!(card_stem(".github"), "github-card");
    assert_eq!(card_stem(""), "repository-card");
    assert_eq!(card_stem("..."), "repository-card");
}

#[test]
fn a_saved_card_never_replaces_another() {
    use crate::commands::write_new;
    let dir = std::env::temp_dir().join(format!("birdfeeder-cards-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    let first = write_new(&dir, "seedcake-card", "png", b"one").unwrap();
    let second = write_new(&dir, "seedcake-card", "png", b"two").unwrap();
    let third = write_new(&dir, "seedcake-card", "png", b"three").unwrap();
    assert_eq!(first, dir.join("seedcake-card.png"));
    assert_eq!(second, dir.join("seedcake-card-2.png"));
    assert_eq!(third, dir.join("seedcake-card-3.png"));
    assert_eq!(std::fs::read(&first).unwrap(), b"one");
    assert_eq!(std::fs::read(&second).unwrap(), b"two");

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn notifications_follow_the_settings_and_fold_a_burst() {
    use crate::settings::Settings;
    use crate::watch::to_show;
    let notice = |kind: &'static str, title: &str| Notice {
        kind,
        title: title.into(),
        body: format!("{title} body"),
        actor: None,
    };
    let mut settings = Settings::default();
    settings.notify.downloads = false;

    let few = [notice("star", "A star"), notice("downloads", "Downloads")];
    assert_eq!(
        to_show(&settings, &few),
        vec![("A star".to_string(), "A star body".to_string())]
    );

    let many: Vec<Notice> = (1..=5)
        .map(|i| notice("star", &format!("Star {i}")))
        .collect();
    assert_eq!(
        to_show(&settings, &many),
        vec![(
            "5 new things on GitHub".to_string(),
            "Star 1; Star 2; Star 3, and 2 more".to_string()
        )]
    );

    settings.notifications = false;
    assert!(to_show(&settings, &many).is_empty());
}

#[test]
fn signing_in_waits_slows_down_and_stops_as_github_says() {
    use crate::signin::{Answer, Step, from_app, next};
    let said = |error: &str| Answer {
        error: Some(error.into()),
        ..Answer::default()
    };

    assert_eq!(next(said("authorization_pending"), 5), Step::Wait(5));
    assert_eq!(next(said("slow_down"), 5), Step::Wait(10));
    let slower = Answer {
        interval: Some(15),
        ..said("slow_down")
    };
    assert_eq!(next(slower, 5), Step::Wait(15));
    assert!(matches!(next(said("expired_token"), 5), Step::Fail(_)));
    assert!(matches!(next(said("access_denied"), 5), Step::Fail(_)));
    let odd = Answer {
        error_description: Some("Device flow is disabled".into()),
        ..said("device_flow_disabled")
    };
    assert_eq!(next(odd, 5), Step::Fail("Device flow is disabled".into()));
    let token = Answer {
        access_token: Some("gho_abc".into()),
        ..Answer::default()
    };
    assert_eq!(next(token, 5), Step::Done("gho_abc".into()));

    assert!(from_app("gho_abc"));
    assert!(!from_app("ghp_abc"));
    assert!(!from_app("github_pat_abc"));
}

/// Syncs against the real API with the GitHub CLI's token. Run with `--ignored`.
#[tokio::test]
#[ignore]
async fn live_sync() {
    use crate::github::{self, GitHub};
    use crate::settings::Settings;
    use std::sync::Mutex;

    let token = crate::auth::gh_token().await.expect("gh auth token");
    let gh = GitHub::new(github::http_client(), token);
    let db = Mutex::new(db::open_in_memory().unwrap());
    let settings = Settings::default();

    for round in 1..=2 {
        let started = std::time::Instant::now();
        let report = crate::sync::run(&db, &gh, &settings, &|step| println!("  {step}"))
            .await
            .expect("sync");
        let conn = db.lock().unwrap();
        let overview = views::overview(&conn, 91).unwrap();
        let downloads = views::downloads(&conn, &settings.ignore_assets).unwrap();
        println!(
            "round {round}: {:.1}s, {} repos, {} stars, {} downloads in {} repos, {} notices, {} unread, warnings {:?}",
            started.elapsed().as_secs_f32(),
            overview.totals.repos,
            overview.totals.stars,
            downloads.total,
            downloads.repos.len(),
            report.notices.len(),
            db::unread_count(&conn).unwrap(),
            report.warnings,
        );
        let mut kinds = std::collections::BTreeMap::new();
        for e in db::events(
            &conn,
            &EventQuery {
                limit: Some(500),
                ..Default::default()
            },
        )
        .unwrap()
        {
            *kinds.entry(e.kind).or_insert(0) += 1;
        }
        println!("  events by kind (latest 500): {kinds:?}");
        println!("  rates: {:?}", report.rates);
        if let Some(streak) = &overview.streak {
            println!(
                "  streak: {} days now, {} best, {} today",
                streak.current, streak.best, streak.today
            );
        }
        for goal in &overview.goals {
            println!(
                "  goal: {} {:?} {} of {} (from {}), eta {:?}",
                goal.what, goal.repo, goal.current, goal.target, goal.from, goal.eta_days
            );
        }
        println!("  records: {:?}", overview.records);
        let list = views::life_list(&conn).unwrap();
        for s in &list.sightings {
            println!(
                "  {} {:<20} {:?} {:?} {:?}",
                if s.seen { "x" } else { " " },
                s.title,
                s.when,
                s.detail,
                s.progress
            );
        }
        for v in list.visitors.iter().take(5) {
            println!(
                "  visitor: {} followers={} company={:?} {} {:?}",
                v.login, v.followers, v.company, v.kind, v.repo
            );
        }
        assert!(overview.totals.repos > 0);
        assert!(
            report.notices.is_empty(),
            "a first or repeated sync should be quiet"
        );
    }
}

/// Stores, reads and removes a token in the real system keyring. Run with `--ignored`.
#[tokio::test]
#[ignore]
async fn keyring_round_trip() {
    use crate::auth;

    assert!(
        auth::saved_token().await.is_none(),
        "a real token is saved; not touching it"
    );
    auth::save_token("test-token".into()).await.expect("save");
    assert_eq!(auth::saved_token().await.as_deref(), Some("test-token"));
    auth::clear_token().await.expect("clear");
    assert!(auth::saved_token().await.is_none());
    auth::clear_token().await.expect("clearing twice is fine");
}
