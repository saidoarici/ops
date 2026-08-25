//! UDS üzerinde NDJSON protokolü (docs/architecture.md, "Daemon protocol").
//! Bir satır = bir JSON mesaj. `id` istemcinin verdiği değerle aynen yankılanır.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::OpsError;

#[derive(Debug, Deserialize)]
pub struct Request {
    #[serde(default)]
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
}

/// Başarılı yanıtı tek satır JSON string'e serileştirir.
pub fn ok_line(id: &Option<Value>, result: &Value) -> String {
    serde_json::json!({ "id": id, "result": result }).to_string()
}

pub fn err_line(id: &Option<Value>, code: &str, message: &str) -> String {
    serde_json::json!({ "id": id, "error": { "code": code, "message": message } }).to_string()
}

pub fn err_line_from(id: &Option<Value>, e: &OpsError) -> String {
    err_line(id, e.code(), &e.to_string())
}

// Metod parametre şemaları. Her metod typed bir şemadan geçer; ham string
// yürütme yoktur. Bilinmeyen alanlar yalnızca mutasyon şemalarında reddedilir.

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdParams {
    pub id: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LimitParams {
    #[serde(default)]
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskUpdateParams {
    pub id: String,
    pub patch: crate::models::TaskPatch,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectUpdateParams {
    pub id: String,
    pub patch: crate::models::ProjectPatch,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReminderUpdateParams {
    pub id: String,
    pub patch: crate::models::ReminderPatch,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineUpdateParams {
    pub id: String,
    pub patch: crate::models::RoutinePatch,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectListParams {
    #[serde(default)]
    pub include_archived: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedListParams {
    #[serde(default)]
    pub include_closed: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditListParams {
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub before_seq: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSetParams {
    pub key: String,
    pub value: Value,
}

/// UI kendi lokal saat dilimi ofsetini (dakika) yollar; daemon gün sınırlarını
/// buna göre çizer. Verilmezse daemon'ın lokal ofseti kullanılır.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TodayParams {
    #[serde(default)]
    pub utc_offset_minutes: Option<i32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentMessagesParams {
    pub session_id: String,
    #[serde(default)]
    pub after_seq: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDetectParams {
    #[serde(default)]
    pub force: bool,
}

/// Parola yalnızca doğrulama için bellekte tutulur; Debug çıktısına girmez.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FullAccessConfigureParams {
    pub new_password: String,
    #[serde(default)]
    pub current_password: Option<String>,
}

impl std::fmt::Debug for FullAccessConfigureParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FullAccessConfigureParams").finish_non_exhaustive()
    }
}

/// Token yalnızca Keychain'e yazılır; Debug çıktısına girmez.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TelegramConfigureParams {
    pub token: String,
    pub allowed_user_id: String,
    pub allowed_chat_id: String,
}

impl std::fmt::Debug for TelegramConfigureParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TelegramConfigureParams")
            .field("allowed_user_id", &self.allowed_user_id)
            .field("allowed_chat_id", &self.allowed_chat_id)
            .finish_non_exhaustive()
    }
}

/// API anahtarı yalnızca Keychain'e yazılır; Debug çıktısına girmez.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WhatsAppConfigureParams {
    pub base_url: String,
    pub api_key: String,
    pub phone_number: String,
}

impl std::fmt::Debug for WhatsAppConfigureParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WhatsAppConfigureParams")
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_bearing_params_do_not_leak_in_debug() {
        let p: FullAccessConfigureParams =
            serde_json::from_str(r#"{"newPassword":"hunter2-hunter2"}"#).unwrap();
        assert!(!format!("{p:?}").contains("hunter2"));
        let t: TelegramConfigureParams = serde_json::from_str(
            r#"{"token":"123:SECRETSECRETSECRETSECRET","allowedUserId":"1","allowedChatId":"2"}"#,
        )
        .unwrap();
        assert!(!format!("{t:?}").contains("SECRET"));
        let w: WhatsAppConfigureParams = serde_json::from_str(
            r#"{"baseUrl":"https://x","apiKey":"KEY-VALUE","phoneNumber":"905551234567"}"#,
        )
        .unwrap();
        assert!(!format!("{w:?}").contains("KEY-VALUE"));
    }
}
