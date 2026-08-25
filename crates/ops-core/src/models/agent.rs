use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::enums::{AgentMessageRole, AgentMode, AgentProviderKind, AgentSessionStatus};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSession {
    pub id: String,
    pub provider: AgentProviderKind,
    pub project_id: Option<String>,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub mode: AgentMode,
    pub working_directory: Option<String>,
    pub status: AgentSessionStatus,
    pub summary: Option<String>,
    pub evidence_ids: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub provider_session_id: Option<String>,
    pub last_activity_at: Option<DateTime<Utc>>,
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentMessage {
    pub id: String,
    pub session_id: String,
    pub seq: i64,
    pub role: AgentMessageRole,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

/// UI'dan gelen chat isteği. ACT modu için `confirm_act` zorunludur.
/// FULL parolası yalnızca daemon belleğinde doğrulanır; mesaj, DB, audit,
/// log veya provider prompt'una hiçbir zaman yazılmaz (Debug çıktısı dahil).
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentChatRequest {
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub provider: Option<AgentProviderKind>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub mode: Option<AgentMode>,
    pub prompt: String,
    #[serde(default)]
    pub confirm_act: bool,
    #[serde(default)]
    pub full_access_password: Option<String>,
}

impl std::fmt::Debug for AgentChatRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentChatRequest")
            .field("session_id", &self.session_id)
            .field("provider", &self.provider)
            .field("project_id", &self.project_id)
            .field("mode", &self.mode)
            .field("prompt_chars", &self.prompt.chars().count())
            .field("confirm_act", &self.confirm_act)
            .field("full_access_password", &self.full_access_password.as_ref().map(|_| "***"))
            .finish()
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInfo {
    pub installed: bool,
    pub path: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDetectReport {
    pub claude: ProviderInfo,
    pub codex: ProviderInfo,
    pub checked_at: DateTime<Utc>,
}
