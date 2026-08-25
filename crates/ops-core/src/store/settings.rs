use std::collections::BTreeMap;

use rusqlite::params;

use crate::models::{AuditResult, Ctx, NewAudit, RiskLevel};
use crate::store::{audit, Store};
use crate::{time, OpsError, Result};

/// Ayar anahtarları allowlist'i. Secret'lar bu tabloya giremez — token ve
/// benzerleri macOS Keychain'de yaşar (docs/threat-model.md T14).
/// Telegram anahtarları kimlik yapılandırmasıdır (user/chat id), secret değildir.
const ALLOWED_KEYS: &[&str] = &[
    "display_name",
    "telegram_enabled",
    "telegram_allowed_user_id",
    "telegram_allowed_chat_id",
    "telegram_last_update_id",
    "whatsapp_config",
];

const FORBIDDEN_SUBSTRINGS: &[&str] = &["token", "secret", "password", "credential", "api_key"];

const MAX_VALUE_BYTES: usize = 4096;

fn validate_key(key: &str) -> Result<String> {
    let key_l = key.to_lowercase();
    if FORBIDDEN_SUBSTRINGS.iter().any(|f| key_l.contains(f)) {
        return Err(OpsError::Security(format!(
            "'{key}' secret benzeri bir anahtar; ayarlara yazılamaz (Keychain kullanılır)"
        )));
    }
    if !ALLOWED_KEYS.contains(&key_l.as_str()) {
        return Err(OpsError::Validation(format!("bilinmeyen ayar anahtarı: {key}")));
    }
    Ok(key_l)
}

fn upsert(conn: &rusqlite::Connection, key: &str, value: &serde_json::Value) -> Result<()> {
    let serialized = value.to_string();
    if serialized.len() > MAX_VALUE_BYTES {
        return Err(OpsError::Validation("ayar değeri çok büyük (max 4 KB)".into()));
    }
    conn.execute(
        "INSERT INTO settings(key, value, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![key, serialized, time::to_db(&time::now())],
    )?;
    Ok(())
}

impl Store {
    pub fn get_settings(&self) -> Result<BTreeMap<String, serde_json::Value>> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut map = BTreeMap::new();
        for row in rows {
            let (k, v) = row?;
            map.insert(k, serde_json::from_str(&v).unwrap_or(serde_json::Value::Null));
        }
        Ok(map)
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<serde_json::Value>> {
        let conn = self.db.conn();
        let raw: Option<String> = conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        Ok(raw.and_then(|s| serde_json::from_str(&s).ok()))
    }

    /// Kullanıcı kaynaklı ayar yazımı: allowlist + audit.
    pub fn set_setting(&self, ctx: &Ctx, key: &str, value: serde_json::Value) -> Result<()> {
        let key = validate_key(key)?;
        let mut conn = self.db.conn();
        let tx = conn.transaction()?;
        upsert(&tx, &key, &value)?;
        audit::append_tx(
            &tx,
            &NewAudit {
                actor: ctx.actor,
                origin: ctx.origin,
                action: "SETTINGS_SET".into(),
                target: Some(format!("setting:{key}")),
                risk_level: RiskLevel::R0,
                capability: None,
                result: AuditResult::Ok,
                metadata: serde_json::json!({}),
            },
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Audit'siz iç yazım — yüksek frekanslı, güvenlik açısından nötr değerler
    /// için (ör. Telegram poll imleci). Anahtar allowlist'i yine uygulanır.
    pub fn set_setting_unaudited(&self, key: &str, value: serde_json::Value) -> Result<()> {
        let key = validate_key(key)?;
        upsert(&self.db.conn(), &key, &value)
    }
}
