//! Sample data for `--demo`, so the app can be tried without a GitHub account.

use anyhow::Result;
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use rusqlite::{Connection, params};
use serde_json::json;

use crate::db::{self, NewEvent};
use crate::settings::Settings;
use crate::{apply, highlights};

const LOGIN: &str = "robin";

/// The visitors. Every one of them is a bird.
const BIRDS: &[&str] = &[
    "siskin",
    "waxwing",
    "nuthatch",
    "dunnock",
    "goldcrest",
    "linnet",
    "redpoll",
    "chiffchaff",
    "lapwing",
    "bullfinch",
    "treecreeper",
    "wagtail",
    "fieldfare",
    "brambling",
    "stonechat",
    "dipper",
    "kestrel",
    "curlew",
    "plover",
    "redstart",
    "wheatear",
    "pipit",
    "skylark",
    "swift",
    "starling",
    "jackdaw",
    "magpie",
    "firecrest",
    "blackcap",
    "yellowhammer",
    "hawfinch",
    "crossbill",
    "greenfinch",
    "chaffinch",
    "sandpiper",
    "whinchat",
];

struct Repo {
    name: &'static str,
    about: &'static str,
    language: &'static str,
    stars: i64,
    forks: i64,
    watchers: i64,
    open_issues: i64,
    open_prs: i64,
    private: bool,
    /// Typical page views per day.
    views: i64,
}

const REPOS: &[Repo] = &[
    Repo {
        name: "seedcake",
        about: "A static site generator that bakes Markdown into plain, fast HTML.",
        language: "Rust",
        stars: 1284,
        forks: 61,
        watchers: 23,
        open_issues: 14,
        open_prs: 3,
        private: false,
        views: 240,
    },
    Repo {
        name: "windowbox",
        about: "Keyboard-driven window tiling for people who like a tidy desktop.",
        language: "Rust",
        stars: 412,
        forks: 19,
        watchers: 9,
        open_issues: 6,
        open_prs: 1,
        private: false,
        views: 70,
    },
    Repo {
        name: "teapot",
        about: "A tiny HTTP server that only ever answers 418.",
        language: "Go",
        stars: 187,
        forks: 8,
        watchers: 4,
        open_issues: 1,
        open_prs: 0,
        private: false,
        views: 30,
    },
    Repo {
        name: "lantern",
        about: "Dark-mode colour schemes that survive a terminal with 16 colours.",
        language: "TypeScript",
        stars: 101,
        forks: 5,
        watchers: 3,
        open_issues: 2,
        open_prs: 1,
        private: false,
        views: 22,
    },
    Repo {
        name: "doormat",
        about: "A rate limiter that politely asks traffic to wait outside.",
        language: "Go",
        stars: 58,
        forks: 4,
        watchers: 2,
        open_issues: 0,
        open_prs: 0,
        private: false,
        views: 9,
    },
    Repo {
        name: "mittens",
        about: "Git hooks that keep your hands off main.",
        language: "Shell",
        stars: 23,
        forks: 1,
        watchers: 1,
        open_issues: 1,
        open_prs: 0,
        private: false,
        views: 4,
    },
    Repo {
        name: "hedgerow",
        about: "Notes and half-finished ideas.",
        language: "Markdown",
        stars: 0,
        forks: 0,
        watchers: 1,
        open_issues: 0,
        open_prs: 0,
        private: true,
        views: 0,
    },
];

/// A release as its tag, days since publishing, and downloads of the main files.
type Release = (&'static str, i64, i64);

const RELEASES: &[(&str, &[Release])] = &[
    (
        "seedcake",
        &[
            ("v2.4.0", 1, 412),
            ("v2.3.1", 16, 2841),
            ("v2.3.0", 31, 3105),
            ("v2.2.0", 67, 4460),
            ("v2.1.0", 112, 2952),
            ("v2.0.0", 170, 1874),
        ],
    ),
    (
        "windowbox",
        &[
            ("v0.9.2", 5, 96),
            ("v0.9.1", 24, 318),
            ("v0.9.0", 52, 502),
            ("v0.8.0", 120, 377),
        ],
    ),
    ("lantern", &[("v1.2.0", 11, 64), ("v1.1.0", 80, 141)]),
];

/// Issues as the repo they were opened in and their title.
const ISSUES: &[(&str, &str)] = &[
    ("seedcake", "Draft pages still show up in the sitemap"),
    ("windowbox", "Snap to grid ignores the panel height"),
    ("seedcake", "Build fails on Windows paths with spaces"),
    ("lantern", "Colours wash out in tmux"),
    ("seedcake", "Would love a flag to print the config it found"),
    ("teapot", "Should HEAD requests get a 418 too?"),
    ("seedcake", "Docs: the quick start skips a step"),
    (
        "windowbox",
        "Focus is lost after moving a window to another screen",
    ),
];

const PULLS: &[(&str, &str)] = &[
    ("seedcake", "Cache rendered pages between builds"),
    ("windowbox", "Remember the layout for each monitor"),
    ("seedcake", "Add a --quiet flag"),
    ("lantern", "Add a high-contrast variant"),
    ("seedcake", "Support fenced code blocks with titles"),
    ("teapot", "Brew for longer on slow connections"),
];

/// Inbox threads as reason, repo, title and the comment that arrived.
const REPLIES: &[(&str, &str, &str, &str)] = &[
    (
        "mention",
        "seedcake",
        "Build fails on Windows paths with spaces",
        "@robin could this be the same bug as the sitemap one? I can reproduce it on a fresh clone.",
    ),
    (
        "review_requested",
        "seedcake",
        "Cache rendered pages between builds",
        "Ready for another look. I split the cache key change into its own commit.",
    ),
    (
        "comment",
        "windowbox",
        "Snap to grid ignores the panel height",
        "Confirmed on a second machine. Happy to test a fix if you have a branch.",
    ),
    (
        "mention",
        "lantern",
        "Colours wash out in tmux",
        "Thanks @robin, setting the terminal override fixed it for me.",
    ),
    (
        "comment",
        "seedcake",
        "Would love a flag to print the config it found",
        "This would have saved me an evening. +1.",
    ),
];

/// A small repeatable random source, so the sample looks the same on every run.
struct Dice(u64);

impl Dice {
    fn roll(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// A number from 0 up to but not including `n`.
    fn below(&mut self, n: i64) -> i64 {
        (self.roll() % n.max(1) as u64) as i64
    }

    fn unit(&mut self) -> f64 {
        (self.roll() % 1_000_000) as f64 / 1_000_000.0
    }

    fn pick<T: Copy>(&mut self, from: &[T]) -> T {
        from[self.below(from.len() as i64) as usize]
    }
}

fn stamp(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn hash(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// An identicon like the ones GitHub gives accounts without a picture, as an inline image.
fn identicon(login: &str) -> String {
    let bits = hash(login);
    let hue = (bits >> 32) % 360;
    let mut cells = String::new();
    for row in 0..5u64 {
        for col in 0..3u64 {
            if bits >> (row * 3 + col) & 1 == 1 {
                for x in [col, 4 - col] {
                    cells += &format!(
                        "%3Crect x='{}' y='{}' width='1' height='1'/%3E",
                        x + 1,
                        row + 1
                    );
                }
            }
        }
    }
    format!(
        "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 7 7'%3E\
         %3Crect width='7' height='7' fill='%23eef0ea'/%3E\
         %3Cg fill='hsl({hue}, 45%25, 52%25)'%3E{cells}%3C/g%3E%3C/svg%3E"
    )
}

struct Sample<'a> {
    conn: &'a Connection,
    now: DateTime<Utc>,
    dice: Dice,
}

impl Sample<'_> {
    fn id(repo: &str) -> String {
        format!("demo:{repo}")
    }

    fn full(repo: &str) -> String {
        format!("{LOGIN}/{repo}")
    }

    /// Records one event that happened `minutes` ago.
    fn event(
        &self,
        kind: &'static str,
        minutes: i64,
        repo: Option<&str>,
        fill: impl FnOnce(&mut NewEvent),
    ) -> Result<()> {
        let at = stamp(self.now - Duration::minutes(minutes));
        let mut e = NewEvent {
            key: format!("{kind}:{}:{minutes}", repo.unwrap_or("account")),
            kind,
            repo_id: repo.map(Self::id),
            repo: repo.map(Self::full),
            count: 1,
            occurred_at: at,
            read: true,
            ..Default::default()
        };
        fill(&mut e);
        if let Some(actor) = &e.actor {
            e.key = format!("{}:{actor}", e.key);
            e.actor_avatar = Some(identicon(actor));
        }
        db::insert_event(self.conn, &e)?;
        Ok(())
    }

    fn by(&mut self) -> String {
        self.dice.pick(BIRDS).to_string()
    }

    fn repos(&mut self) -> Result<()> {
        let seeded = stamp(self.now - Duration::days(60));
        for repo in REPOS {
            self.conn.execute(
                "INSERT INTO repos (id, name, url, description, is_private, can_push, language, stars,
                                    forks, watchers, open_total, open_issues, open_prs, pushed_at,
                                    seeded_at, traffic_at)
                 VALUES (?1, ?2, '', ?3, ?4, 1, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?12)",
                params![
                    Self::id(repo.name),
                    Self::full(repo.name),
                    repo.about,
                    repo.private,
                    repo.language,
                    repo.stars,
                    repo.forks,
                    repo.watchers,
                    repo.open_issues + repo.open_prs,
                    repo.open_issues,
                    repo.open_prs,
                    stamp(self.now),
                    seeded,
                ],
            )?;
        }
        Ok(())
    }

    /// Stars arrive faster as a repo gets older, with one busy week for the biggest.
    fn stars(&mut self) -> Result<()> {
        for repo in REPOS {
            let years = if repo.stars > 500 { 3.2 } else { 1.6 };
            let mut ages: Vec<i64> = (0..repo.stars)
                .map(|i| {
                    let days = if repo.name == "seedcake" && i % 6 == 0 {
                        47.0 + self.dice.unit() * 4.0
                    } else {
                        years * 365.0 * (1.0 - self.dice.unit().sqrt())
                    };
                    (days * 1440.0) as i64 + 180
                })
                .collect();
            ages.sort_unstable();
            for (i, minutes) in ages.into_iter().enumerate() {
                let bird = BIRDS[(i * 7 + repo.name.len()) % BIRDS.len()];
                let who = match i / BIRDS.len() {
                    0 => bird.to_string(),
                    lap => format!("{bird}{}", lap + 1),
                };
                self.event("star", minutes, Some(repo.name), |e| e.actor = Some(who))?;
            }
        }
        Ok(())
    }

    fn releases(&mut self) -> Result<()> {
        for (repo, list) in RELEASES {
            for (tag, days, downloads) in *list {
                let base = format!("{repo}-{}", tag.trim_start_matches('v'));
                let assets = json!([
                    { "name": format!("{base}-x86_64-linux.tar.gz"), "downloads": downloads * 46 / 100, "size": 6_400_000 },
                    { "name": format!("{base}-aarch64-macos.tar.gz"), "downloads": downloads * 31 / 100, "size": 5_900_000 },
                    { "name": format!("{base}-x86_64-windows.zip"), "downloads": downloads * 23 / 100, "size": 7_100_000 },
                    { "name": "SHA256SUMS", "downloads": downloads / 9, "size": 310 },
                ]);
                self.conn.execute(
                    "INSERT INTO releases (repo_id, tag, name, published_at, assets) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        Self::id(repo),
                        tag,
                        format!("{} {}", capitalised(repo), tag.trim_start_matches('v')),
                        stamp(self.now - Duration::days(*days) - Duration::hours(3)),
                        assets.to_string(),
                    ],
                )?;
                self.event("release", days * 1440 + 180, Some(*repo), |e| {
                    e.subject = Some(tag.to_string());
                    e.meta = Some(json!({ "name": format!("{} {}", capitalised(repo), tag.trim_start_matches('v')) }));
                })?;
            }
            // A tally for each of the last 45 days, split across the two newest releases.
            let typical = list[1].2 / 40;
            for day in 0..45 {
                let gained = typical / 2 + self.dice.below(typical.max(2));
                if gained == 0 {
                    continue;
                }
                let older = gained / 4;
                let mut per_release = serde_json::Map::new();
                per_release.insert(list[0].0.to_string(), json!(gained - older));
                if older > 0 {
                    per_release.insert(list[1].0.to_string(), json!(older));
                }
                let minutes = 48 + self.dice.below(170);
                let at = self.now - Duration::days(day) - Duration::minutes(minutes);
                db::insert_event(
                    self.conn,
                    &NewEvent {
                        key: format!("downloads:{repo}:{day}"),
                        kind: "downloads",
                        repo_id: Some(Self::id(repo)),
                        repo: Some(Self::full(repo)),
                        count: gained,
                        meta: Some(json!({ "releases": per_release })),
                        occurred_at: stamp(at),
                        read: true,
                        ..Default::default()
                    },
                )?;
            }
        }
        self.conn.execute(
            "UPDATE repos SET release_count =
               (SELECT COUNT(*) FROM releases WHERE repo_id = repos.id)",
            [],
        )?;
        apply::recount_downloads(self.conn, &Settings::default().ignore_assets)
    }

    /// Issues, pull requests, forks, followers and replies spread over the last three months.
    fn chatter(&mut self) -> Result<()> {
        let busy = [
            "seedcake",
            "seedcake",
            "seedcake",
            "windowbox",
            "windowbox",
            "lantern",
            "teapot",
        ];
        for n in 0..26i64 {
            let minutes = 200 + n * 4700 + self.dice.below(2400);
            let (repo, title) = ISSUES[n as usize % ISSUES.len()];
            let who = self.by();
            self.event("issue", minutes, Some(repo), |e| {
                e.actor = Some(who);
                e.subject = Some(title.to_string());
                e.number = Some(430 - n * 3);
            })?;
        }
        for n in 0..18i64 {
            let minutes = 540 + n * 6900 + self.dice.below(3000);
            let (repo, title) = PULLS[n as usize % PULLS.len()];
            let who = self.by();
            self.event("pr", minutes, Some(repo), |e| {
                e.actor = Some(who);
                e.subject = Some(title.to_string());
                e.number = Some(431 - n * 3);
            })?;
        }
        for n in 0..24i64 {
            let minutes = 70 + n * 5200 + self.dice.below(2800);
            let repo = self.dice.pick(&busy);
            let who = self.by();
            self.event("fork", minutes, Some(repo), |e| e.actor = Some(who))?;
        }
        for n in 0..21i64 {
            let minutes = 300 + n * 6100 + self.dice.below(3600);
            let who = self.by();
            self.event("follower", minutes, None, |e| e.actor = Some(who))?;
        }
        for n in 0..14i64 {
            let minutes = 22 + n * 8600 + self.dice.below(n * 1900 + 1);
            let (reason, repo, title, text) = REPLIES[n as usize % REPLIES.len()];
            let who = self.by();
            self.event("inbox", minutes, Some(repo), |e| {
                e.actor = Some(who);
                e.subject = Some(title.to_string());
                e.number = Some(412 - n * 5);
                e.meta = Some(json!({ "reason": reason, "type": "Issue", "snippet": text }));
            })?;
        }
        self.event("milestone", 125, Some("lantern"), |e| {
            e.subject = Some("stars".into());
            e.count = 100;
        })?;
        self.event("milestone", 47 * 1440, Some("seedcake"), |e| {
            e.subject = Some("stars".into());
            e.count = 1000;
        })?;
        self.event("unstar", 4300, Some("windowbox"), |_| {})?;
        self.event("traffic", 1500, Some("teapot"), |e| {
            e.subject = Some("news.ycombinator.com".into());
            e.count = 412;
        })?;
        self.event("trend", 190, Some("windowbox"), |e| {
            e.count = 14;
            e.meta = Some(json!({ "usual": 4 }));
        })?;
        self.event("story", 49 * 1440, Some("seedcake"), |e| {
            e.subject = Some(
                "Seedcake: a static site generator that bakes Markdown into plain HTML".into(),
            );
            e.count = 187;
            e.meta = Some(json!({ "comments": 64 }));
        })?;
        self.event("record", 48 * 1440, None, |e| {
            e.subject = Some("star-day".into());
            e.count = 61;
            e.meta = Some(json!({ "previous": 9 }));
        })?;
        // A well-followed visitor, recent enough to be at the top of the list.
        self.event("star", 9, Some("seedcake"), |e| {
            e.actor = Some("kestrel".into());
            e.meta = Some(json!({ "sighting": "Very rare bird" }));
        })
    }

    /// Gives every bird a following, and a few of them a large one.
    fn people(&mut self) -> Result<()> {
        let known = [
            ("kestrel", 12_400, Some("@falconry")),
            ("starling", 3_610, Some("Murmuration")),
            ("goldcrest", 1_340, None),
            ("waxwing", 912, Some("@cedar-works")),
            ("jackdaw", 604, None),
        ];
        for (i, bird) in BIRDS.iter().enumerate() {
            let (followers, company) = known
                .iter()
                .find(|(login, ..)| login == bird)
                .map_or((3 + (hash(bird) % 190) as i64, None), |(_, n, c)| (*n, *c));
            self.conn.execute(
                "INSERT INTO people (login, company, followers, fetched_at) VALUES (?1, ?2, ?3, ?4)",
                params![bird, company, followers, stamp(self.now - Duration::days(i as i64))],
            )?;
        }
        Ok(())
    }

    /// A year of the owner's own contributions, ending on a 23-day run.
    fn contributions(&mut self) -> Result<()> {
        for ago in 0..365i64 {
            let rest_day = ago == 23 || (ago > 23 && (ago % 9 == 0 || self.dice.below(7) == 0));
            let count = if rest_day { 0 } else { 1 + self.dice.below(13) };
            self.conn.execute(
                "INSERT INTO contributions (date, count) VALUES (?1, ?2)",
                params![
                    (self.now - Duration::days(ago))
                        .format("%Y-%m-%d")
                        .to_string(),
                    count
                ],
            )?;
        }
        Ok(())
    }

    fn traffic(&mut self) -> Result<()> {
        for repo in REPOS.iter().filter(|r| r.views > 0) {
            for day in 0..30 {
                let spike = if repo.name == "teapot" && day == 1 {
                    400
                } else {
                    0
                };
                let views = repo.views / 2 + self.dice.below(repo.views) + spike;
                self.conn.execute(
                    "INSERT INTO traffic_daily (repo_id, date, views, view_uniques, clones, clone_uniques)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        Self::id(repo.name),
                        (self.now - Duration::days(day)).format("%Y-%m-%d").to_string(),
                        views,
                        views * 2 / 5,
                        views / 6 + self.dice.below(4),
                        views / 9 + 1,
                    ],
                )?;
            }
        }
        let sources = [
            ("news.ycombinator.com", 38),
            ("github.com", 24),
            ("Google", 17),
            ("lobste.rs", 9),
            ("reddit.com", 7),
            ("duckduckgo.com", 5),
        ];
        for (repo, scale) in [("seedcake", 60), ("teapot", 14), ("windowbox", 9)] {
            for (source, share) in sources {
                self.conn.execute(
                    "INSERT INTO referrers (repo_id, referrer, count, uniques) VALUES (?1, ?2, ?3, ?4)",
                    params![Self::id(repo), source, share * scale / 10, share * scale / 22 + 1],
                )?;
            }
        }
        Ok(())
    }
}

fn capitalised(word: &str) -> String {
    let mut letters = word.chars();
    match letters.next() {
        Some(first) => first.to_uppercase().chain(letters).collect(),
        None => String::new(),
    }
}

/// Fills an empty database with a believable account and three years of visitors.
pub fn fill(conn: &Connection) -> Result<()> {
    let now = Utc::now();
    let mut sample = Sample {
        conn,
        now,
        dice: Dice(0x9e37_79b9_7f4a_7c15),
    };
    sample.repos()?;
    sample.stars()?;
    sample.releases()?;
    sample.chatter()?;
    sample.traffic()?;
    sample.people()?;
    sample.contributions()?;

    // The newest few events are left unread, as they would be after a morning away.
    conn.execute(
        "UPDATE events SET read = 0 WHERE id IN (
           SELECT id FROM events WHERE kind NOT IN ('unstar', 'release')
           ORDER BY occurred_at DESC LIMIT 6)",
        [],
    )?;

    let account = json!({
        "login": LOGIN,
        "name": "Robin",
        "avatar_url": identicon(LOGIN),
        "html_url": "",
        "followers": 86,
        "following": 31,
    });
    db::meta_set(conn, "account", &account.to_string())?;
    db::meta_set(
        conn,
        "followers_seeded_at",
        &stamp(now - Duration::days(60)),
    )?;
    db::meta_set(conn, "tracking_since", &stamp(now - Duration::days(60)))?;
    db::meta_set(conn, "last_sync_at", &stamp(now))?;

    // Everything already on the life list counts as noticed.
    let ctx = apply::Ctx {
        now: &stamp(now),
        today: &chrono::Local::now().format("%Y-%m-%d").to_string(),
        viewer: LOGIN,
        ignore_assets: &[],
    };
    highlights::note_sightings(conn, &ctx, &mut Vec::new())
}
