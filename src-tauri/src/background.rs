//! One check made by Android's job scheduler while the app is closed or out of sight.
//! `CheckJob.kt` calls in here and posts whatever notifications come back.

use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use jni::JNIEnv;
use jni::objects::{JClass, JString};
use jni::sys::jstring;
use serde::Serialize;

use crate::github::{self, GitHub};
use crate::settings::Settings;
use crate::{auth, db, sync, watch};

/// A check this soon after the last one finds nothing worth the request.
const FRESH: Duration = Duration::from_secs(5 * 60);

#[derive(Default, Serialize)]
struct Answer {
    /// Whether to keep scheduling checks. False once the setting is off, or for sample data.
    keep: bool,
    notices: Vec<Shown>,
    /// Why the check failed, for the log.
    error: Option<String>,
}

#[derive(Serialize)]
struct Shown {
    title: String,
    body: String,
}

/// Called by `CheckJob.checkOnce(dir)`, with the folder the app keeps its data in.
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_gingerbreadfork_birdfeeder_CheckJob_checkOnce<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    dir: JString<'local>,
) -> jstring {
    let dir: String = env.get_string(&dir).map(Into::into).unwrap_or_default();
    let answer = check_once(Path::new(&dir)).unwrap_or_else(|e| Answer {
        keep: true,
        error: Some(format!("{e:#}")),
        ..Answer::default()
    });
    let json = serde_json::to_string(&answer).unwrap_or_else(|_| "{}".into());
    env.new_string(json)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

fn check_once(dir: &Path) -> anyhow::Result<Answer> {
    // Sample data never contacts GitHub.
    if crate::wants_demo() {
        return Ok(Answer::default());
    }
    let path = dir.join(db::FILE_NAME);
    // The app has not set itself up yet, so there is nothing to check.
    if !path.exists() {
        return Ok(Answer {
            keep: true,
            ..Answer::default()
        });
    }
    auth::keep_in(dir);
    let conn = db::open(&path)?;
    let settings = Settings::load(&conn);
    if !settings.keep_running {
        return Ok(Answer::default());
    }
    let recent = db::meta_get(&conn, "last_sync_at")?
        .and_then(|at| chrono::DateTime::parse_from_rfc3339(&at).ok())
        .is_some_and(|at| {
            chrono::Utc::now()
                .signed_duration_since(at)
                .to_std()
                .unwrap_or_default()
                < FRESH
        });
    if recent {
        return Ok(Answer {
            keep: true,
            ..Answer::default()
        });
    }

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let db = Mutex::new(conn);
    let notices = runtime.block_on(async {
        // The app is checking right now, so this one is not needed.
        let Ok(_syncing) = watch::SYNCING.try_lock() else {
            return Ok(Vec::new());
        };
        let Some((token, _)) = auth::resolve().await else {
            return Ok(Vec::new());
        };
        let gh = GitHub::new(github::http_client(), token);
        let report = sync::run(&db, &gh, &settings, &|_| {}).await?;
        if report.cut_short.is_none() {
            db::meta_set(&db.lock().unwrap(), "last_sync_at", &sync::now_utc())?;
        }
        anyhow::Ok(watch::to_show(&settings, &report.notices))
    })?;

    Ok(Answer {
        keep: true,
        notices: notices
            .into_iter()
            .map(|(title, body)| Shown { title, body })
            .collect(),
        ..Answer::default()
    })
}
