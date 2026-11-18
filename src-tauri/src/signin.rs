//! Signing in through GitHub in a browser, with GitHub's device flow: the app shows a short
//! code, the user enters it on GitHub and approves, and the app is handed a token.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use reqwest::{Client, header};
use serde::{Deserialize, Serialize};

/// Birdfeeder's OAuth app on GitHub. It is not a secret: the device flow has none.
const CLIENT_ID: &str = "Ov23li5mHOEXY0jAm7MA";
/// Private repositories, traffic and the inbox all come with this one scope.
const SCOPE: &str = "repo";
const CODE_URL: &str = "https://github.com/login/device/code";
const TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
const GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";

/// What the user enters on GitHub, and where.
#[derive(Debug, Clone, Serialize)]
pub struct Code {
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
}

#[derive(Deserialize)]
struct Issued {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: u64,
}

/// GitHub's answer while the code waits to be approved.
#[derive(Debug, Default, Deserialize)]
pub struct Answer {
    pub access_token: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
    pub interval: Option<u64>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    Done(String),
    /// Ask again after this many seconds.
    Wait(u64),
    Fail(String),
}

/// What to do with an answer, given how long the app has been waiting between questions.
pub fn next(answer: Answer, interval: u64) -> Step {
    if let Some(token) = answer.access_token.filter(|t| !t.is_empty()) {
        return Step::Done(token);
    }
    match answer.error.as_deref() {
        Some("authorization_pending") => Step::Wait(interval),
        // GitHub asks for five more seconds between questions each time it says this.
        Some("slow_down") => Step::Wait(answer.interval.unwrap_or(interval + 5)),
        Some("expired_token") => Step::Fail(EXPIRED.into()),
        Some("access_denied") => Step::Fail("Signing in was cancelled on GitHub.".into()),
        _ => Step::Fail(
            answer
                .error_description
                .or(answer.error)
                .unwrap_or_else(|| "GitHub did not answer as expected.".into()),
        ),
    }
}

const EXPIRED: &str = "The code expired before it was entered. Start again for a new one.";
const CANCELLED: &str = "Signing in was cancelled.";

struct Pending {
    round: u64,
    device_code: String,
    interval: u64,
    until: Instant,
}

/// The sign-in under way, if any. Starting again or cancelling ends the one before.
#[derive(Default)]
pub struct Flow {
    pending: Mutex<Option<Pending>>,
    round: AtomicU64,
}

impl Flow {
    /// Asks GitHub for a code for the user to enter.
    pub async fn start(&self, http: &Client) -> Result<Code> {
        let answer: serde_json::Value = http
            .post(CODE_URL)
            .header(header::ACCEPT, "application/json")
            .json(&serde_json::json!({ "client_id": CLIENT_ID, "scope": SCOPE }))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        if let Some(error) = answer.get("error_description").or(answer.get("error")) {
            bail!(
                "GitHub would not start signing in: {}",
                error.as_str().unwrap_or("no reason given")
            );
        }
        let issued: Issued = serde_json::from_value(answer)?;
        let round = self.round.fetch_add(1, Ordering::SeqCst) + 1;
        *self.pending.lock().unwrap() = Some(Pending {
            round,
            device_code: issued.device_code,
            interval: issued.interval.max(1),
            until: Instant::now() + Duration::from_secs(issued.expires_in),
        });
        Ok(Code {
            user_code: issued.user_code,
            verification_uri: issued.verification_uri,
            expires_in: issued.expires_in,
        })
    }

    /// Waits for the code to be approved on GitHub and returns the token it was given.
    pub async fn wait(&self, http: &Client) -> Result<String> {
        let (round, device_code, mut interval, until) = {
            let pending = self.pending.lock().unwrap();
            let p = pending
                .as_ref()
                .ok_or_else(|| anyhow!("Start signing in first."))?;
            (p.round, p.device_code.clone(), p.interval, p.until)
        };
        loop {
            tokio::time::sleep(Duration::from_secs(interval)).await;
            if self.round.load(Ordering::SeqCst) != round {
                bail!(CANCELLED);
            }
            if Instant::now() >= until {
                self.end(round);
                bail!(EXPIRED);
            }
            // A phone can be out of reach of the network while the browser has the screen,
            // so a question that gets no answer is simply asked again.
            let Ok(answer) = ask(http, &device_code).await else {
                continue;
            };
            match next(answer, interval) {
                Step::Done(token) => {
                    self.end(round);
                    return Ok(token);
                }
                Step::Wait(seconds) => interval = seconds,
                Step::Fail(message) => {
                    self.end(round);
                    bail!(message);
                }
            }
        }
    }

    pub fn cancel(&self) {
        self.round.fetch_add(1, Ordering::SeqCst);
        *self.pending.lock().unwrap() = None;
    }

    fn end(&self, round: u64) {
        let mut pending = self.pending.lock().unwrap();
        if pending.as_ref().is_some_and(|p| p.round == round) {
            *pending = None;
        }
    }
}

async fn ask(http: &Client, device_code: &str) -> Result<Answer> {
    Ok(http
        .post(TOKEN_URL)
        .header(header::ACCEPT, "application/json")
        .json(&serde_json::json!({
            "client_id": CLIENT_ID,
            "device_code": device_code,
            "grant_type": GRANT,
        }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
}

/// Whether a saved token came from signing in here rather than being pasted.
/// GitHub starts the tokens it hands to apps with this prefix.
pub fn from_app(token: &str) -> bool {
    token.starts_with("gho_")
}
