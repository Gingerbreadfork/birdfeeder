use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenSource {
    /// A token the user pasted, which the app keeps.
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

pub use store::{clear_token, gh_token, keep_in, save_token, saved_token};

#[cfg(desktop)]
mod store {
    use anyhow::{Result, anyhow};
    use tokio::process::Command;

    const KEYRING_SERVICE: &str = "birdfeeder";
    const KEYRING_USER: &str = "github-token";

    /// The keyring needs no folder of its own.
    pub fn keep_in(_dir: &std::path::Path) {}

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
}

/// A phone keeps the token in a file inside the app's private storage, which
/// no other app can read and which is left out of backups.
#[cfg(mobile)]
mod store {
    use anyhow::{Context, Result};
    use std::path::{Path, PathBuf};
    use std::sync::OnceLock;

    static PATH: OnceLock<PathBuf> = OnceLock::new();

    pub fn keep_in(dir: &Path) {
        let _ = PATH.set(dir.join("github-token"));
    }

    fn path() -> Result<PathBuf> {
        PATH.get()
            .cloned()
            .context("The app's storage is not ready yet")
    }

    async fn blocking<T: Send + 'static>(
        work: impl FnOnce(PathBuf) -> std::io::Result<T> + Send + 'static,
    ) -> Result<T> {
        let path = path()?;
        Ok(tokio::task::spawn_blocking(move || work(path)).await??)
    }

    /// There is no GitHub CLI on a phone.
    pub async fn gh_token() -> Option<String> {
        None
    }

    pub async fn saved_token() -> Option<String> {
        let token = blocking(std::fs::read_to_string).await.ok()?;
        Some(token.trim().to_string()).filter(|token| !token.is_empty())
    }

    /// Writes beside the old file and renames over it, so a token is never half written.
    pub async fn save_token(token: String) -> Result<()> {
        blocking(move |path| {
            use std::io::Write;
            use std::os::unix::fs::OpenOptionsExt;
            let partial = path.with_extension("new");
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&partial)?;
            file.write_all(token.as_bytes())?;
            file.sync_all()?;
            std::fs::rename(partial, path)
        })
        .await
    }

    pub async fn clear_token() -> Result<()> {
        blocking(|path| match std::fs::remove_file(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            other => other,
        })
        .await
    }
}
