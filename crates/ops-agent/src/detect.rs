//! Kurulu resmi CLI tespiti. Credential'lara dokunulmaz; yalnızca binary yolu
//! ve `--version` çıktısı okunur.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use ops_core::models::{AgentDetectReport, ProviderInfo};
use ops_core::time;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

fn is_executable(p: &Path) -> bool {
    p.is_file()
        && std::fs::metadata(p).map(|m| m.permissions().mode() & 0o111 != 0).unwrap_or(false)
}

/// PATH + bilinen kurulum yerlerinde arar. launchd altındaki daemon'ın PATH'i
/// minimal olduğundan (~/.local/bin vb. içermez) açık adaylar şart.
pub fn find_binary(name: &str) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(path) = std::env::var("PATH") {
        for part in path.split(':').filter(|s| !s.is_empty()) {
            candidates.push(Path::new(part).join(name));
        }
    }
    if let Some(home) = dirs::home_dir() {
        candidates.push(home.join(".local/bin").join(name));
        candidates.push(home.join("bin").join(name));
        candidates.push(home.join(".bun/bin").join(name));
    }
    for extra in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"] {
        candidates.push(Path::new(extra).join(name));
    }
    candidates.into_iter().find(|p| is_executable(p))
}

pub async fn probe_version(path: &Path) -> Option<String> {
    let out = timeout(
        Duration::from_secs(10),
        Command::new(path).arg("--version").env_clear().envs(super::minimal_env(path)).output(),
    )
    .await
    .ok()?
    .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().next().unwrap_or("").trim();
    (!line.is_empty()).then(|| line.chars().take(80).collect())
}

async fn probe(name: &str) -> ProviderInfo {
    match find_binary(name) {
        Some(path) => {
            let version = probe_version(&path).await;
            ProviderInfo {
                installed: version.is_some(),
                path: Some(path.display().to_string()),
                version,
            }
        }
        None => ProviderInfo { installed: false, path: None, version: None },
    }
}

pub async fn detect_all() -> AgentDetectReport {
    let (claude, codex) = tokio::join!(probe("claude"), probe("codex"));
    AgentDetectReport { claude, codex, checked_at: time::now() }
}
