use anyhow::{Result, anyhow};
use serde::Serialize;
use tokio::process::Command;

const KEYRING_SERVICE: &str = "birdfeeder";
const KEYRING_USER: &str = "github-token";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenSource {
    /// A token the user pasted, kept in the system keyring.
    Saved,
    /// The GitHub CLI's own login.
    GhCli,
    Env,
}

/// Finds a token, preferring one the user saved over the GitHub CLI login.
pub async fn resolve() -> Option<(String, TokenSource)> {
    if let Some(token) = saved_token().await {
        return Some((token, TokenSource::Saved));
    }
    if let Some(token) = gh_token().await {
        return Some((token, TokenSource::GhCli));
    }
    ["GH_TOKEN", "GITHUB_TOKEN"]
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .map(|value| value.trim().to_string())
        .find(|value| !value.is_empty())
        .map(|token| (token, TokenSource::Env))
}

pub async fn gh_token() -> Option<String> {
    let home = std::env::var("HOME").unwrap_or_default();
    let candidates = [
        "gh".to_string(),
        "/usr/bin/gh".to_string(),
        "/usr/local/bin/gh".to_string(),
        "/opt/homebrew/bin/gh".to_string(),
        format!("{home}/.local/bin/gh"),
    ];
    for bin in candidates {
        let Ok(output) = Command::new(&bin).args(["auth", "token"]).output().await else {
            continue;
        };
        let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if output.status.success() && !token.is_empty() {
            return Some(token);
        }
    }
    None
}

async fn with_entry<T: Send + 'static>(
    f: impl FnOnce(keyring::Entry) -> keyring::Result<T> + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(move || {
        let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)?;
        f(entry)
    })
    .await?
    .map_err(|e| anyhow!("System keyring: {e}"))
}

pub async fn saved_token() -> Option<String> {
    with_entry(|entry| entry.get_password())
        .await
        .ok()
        .filter(|token| !token.is_empty())
}

pub async fn save_token(token: String) -> Result<()> {
    with_entry(move |entry| entry.set_password(&token)).await
}

pub async fn clear_token() -> Result<()> {
    with_entry(|entry| match entry.delete_credential() {
        Err(keyring::Error::NoEntry) => Ok(()),
        other => other,
    })
    .await
}
