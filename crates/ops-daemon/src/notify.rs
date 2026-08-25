//! Bildirim teslimi: macOS banner + yapılandırılmışsa Telegram/WhatsApp.
//! Hatırlatmalar ve rutinler aynı yoldan geçer; her kanal teslimi audit'lenir.
//!
//! Güvenlik (docs/threat-model.md T8): başlık ve gövde osascript'e argv olarak
//! geçirilir, AppleScript kaynak koduna gömülmez. Böylece kullanıcı girdisi
//! (`$(...)`, tırnaklar, AppleScript ifadeleri) veri olarak kalır.

use std::io::Write;
use std::process::{Command, Stdio};

use tracing::{error, info, warn};

use ops_core::models::{Actor, AuditResult, NewAudit, NotificationChannel, Origin, RiskLevel};

use crate::AppState;

const OSASCRIPT: &str = "/usr/bin/osascript";
const SCRIPT: &str =
    "on run argv\n  display notification (item 2 of argv) with title (item 1 of argv)\nend run";

pub fn send_macos(title: &str, body: &str) -> std::io::Result<()> {
    let title = truncate(title, 200);
    let body = truncate(body, 500);
    let mut child = Command::new(OSASCRIPT)
        .arg("-") // script stdin'den; sonraki argümanlar `on run argv`e gider
        .arg(title)
        .arg(body)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(SCRIPT.as_bytes())?;
    }
    drop(child.stdin.take());
    let status = child.wait()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!("osascript çıkış kodu: {status}")))
    }
}

async fn send(
    state: &AppState,
    channel: NotificationChannel,
    title: &str,
    body: &str,
) -> Result<(), String> {
    match channel {
        NotificationChannel::Macos => send_macos(title, body).map_err(|e| e.to_string()),
        NotificationChannel::Telegram => {
            state.remote.send_telegram(&format!("{title}\n{body}")).await.map_err(|e| e.to_string())
        }
        NotificationChannel::Whatsapp => {
            state.remote.send_whatsapp(&format!("{title}\n{body}")).await.map_err(|e| e.to_string())
        }
    }
}

/// Verilen kanallara teslim eder, her kanal için `SEND_NOTIFICATION` audit
/// kaydı yazar ve başarılı kanalları döner. `target` audit hedefidir
/// (ör. `reminder:<id>`).
pub async fn deliver(
    state: &AppState,
    channels: &[NotificationChannel],
    title: &str,
    body: &str,
    target: &str,
) -> Vec<NotificationChannel> {
    let mut delivered = Vec::new();
    for &channel in channels {
        let result = match send(state, channel, title, body).await {
            Ok(()) => {
                info!(target, channel = channel.as_ref(), "bildirim gönderildi");
                delivered.push(channel);
                AuditResult::Ok
            }
            Err(e) => {
                warn!(target, channel = channel.as_ref(), error = %e, "bildirim gönderilemedi");
                AuditResult::Error
            }
        };
        if let Err(e) = state.store.append_audit(NewAudit {
            actor: Actor::Scheduler,
            origin: Origin::Daemon,
            action: "SEND_NOTIFICATION".into(),
            target: Some(target.to_string()),
            risk_level: RiskLevel::R0,
            capability: Some("SEND_NOTIFICATION".into()),
            result,
            metadata: serde_json::json!({ "channel": channel.as_ref() }),
        }) {
            error!(error = %e, "bildirim audit kaydı yazılamadı");
        }
    }
    delivered
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{cut}…")
    }
}
