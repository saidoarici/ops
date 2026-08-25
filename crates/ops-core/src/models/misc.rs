use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::enums::{
    Actor, AuditResult, NotificationChannel, Origin, ReminderStatus, RepeatRule, RiskLevel,
};
use crate::serde_util::double_option;

// ---------------------------------------------------------------- Reminder

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reminder {
    pub id: String,
    pub task_id: Option<String>,
    pub title: String,
    pub notes: String,
    pub remind_at: DateTime<Utc>,
    pub repeat_rule: RepeatRule,
    pub channels: Vec<NotificationChannel>,
    pub status: ReminderStatus,
    pub fired_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReminderCreate {
    pub title: String,
    pub remind_at: DateTime<Utc>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub task_id: Option<String>,
    #[serde(default)]
    pub repeat_rule: Option<RepeatRule>,
    #[serde(default)]
    pub channels: Option<Vec<NotificationChannel>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReminderPatch {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub remind_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub repeat_rule: Option<RepeatRule>,
    #[serde(default)]
    pub channels: Option<Vec<NotificationChannel>>,
    #[serde(default)]
    pub status: Option<ReminderStatus>,
    #[serde(default, deserialize_with = "double_option")]
    pub task_id: Option<Option<String>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReminderFilter {
    #[serde(default)]
    pub statuses: Option<Vec<ReminderStatus>>,
    #[serde(default)]
    pub from: Option<DateTime<Utc>>,
    #[serde(default)]
    pub to: Option<DateTime<Utc>>,
    #[serde(default)]
    pub limit: Option<i64>,
}

// ---------------------------------------------------------------- Audit

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEvent {
    pub id: String,
    pub seq: i64,
    pub timestamp: DateTime<Utc>,
    pub actor: Actor,
    pub origin: Origin,
    pub action: String,
    pub target: Option<String>,
    pub risk_level: RiskLevel,
    pub capability: Option<String>,
    pub result: AuditResult,
    pub metadata: serde_json::Value,
    pub previous_hash: String,
    pub hash: String,
}

/// Audit'e yazılacak yeni kayıt (hash alanları store'da hesaplanır).
#[derive(Debug, Clone)]
pub struct NewAudit {
    pub actor: Actor,
    pub origin: Origin,
    pub action: String,
    pub target: Option<String>,
    pub risk_level: RiskLevel,
    pub capability: Option<String>,
    pub result: AuditResult,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditVerifyReport {
    pub ok: bool,
    pub checked: i64,
    /// Zincirin kırıldığı ilk seq (ok=false iken).
    pub broken_at_seq: Option<i64>,
    pub message: String,
}

/// İşlem bağlamı: kim, nereden. Store'daki her mutasyon bunu ister.
#[derive(Debug, Clone, Copy)]
pub struct Ctx {
    pub actor: Actor,
    pub origin: Origin,
}

impl Ctx {
    pub const LOCAL_USER: Ctx = Ctx { actor: Actor::User, origin: Origin::LocalUi };
    pub const DAEMON: Ctx = Ctx { actor: Actor::Daemon, origin: Origin::Daemon };
    pub const SCHEDULER: Ctx = Ctx { actor: Actor::Scheduler, origin: Origin::Daemon };
    pub const CLI: Ctx = Ctx { actor: Actor::User, origin: Origin::Cli };
}

// ---------------------------------------------------------------- Backup

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub file_name: String,
    pub path: String,
    pub size_bytes: u64,
    pub created_at: Option<DateTime<Utc>>,
}
