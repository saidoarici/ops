use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::enums::{
    Origin, RemoteAuthState, RemoteChannel, RemoteProcessingStatus, RemoteReplayState, TaskSource,
};

/// Remote mesajın üretebileceği izinli intent'lerin tamamı.
///
/// GÜVENLİK DEĞİŞMEZİ: Bu enum'a execution-benzeri hiçbir varyant eklenemez.
/// `RUN_COMMAND`, `EXECUTE`, `START_AGENT`, `WRITE_FILE`, `APPROVE`,
/// `SET_MODE` gibi türler veri modelinde tanımsızdır; serde bilinmeyen tipi
/// reddeder. Remote dünya ile executor arasındaki sınır budur
/// (docs/threat-model.md T1/T19).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RemoteIntent {
    CreateTask {
        title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    CreateReminderProposal {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        requested_time: Option<String>,
    },
    QueryTask {
        query: String,
    },
    AddNote {
        text: String,
    },
}

impl RemoteIntent {
    pub fn kind(&self) -> &'static str {
        match self {
            RemoteIntent::CreateTask { .. } => "CREATE_TASK",
            RemoteIntent::CreateReminderProposal { .. } => "CREATE_REMINDER_PROPOSAL",
            RemoteIntent::QueryTask { .. } => "QUERY_TASK",
            RemoteIntent::AddNote { .. } => "ADD_NOTE",
        }
    }
}

impl RemoteChannel {
    pub fn origin(self) -> Origin {
        match self {
            RemoteChannel::Telegram => Origin::Telegram,
            RemoteChannel::Whatsapp => Origin::Whatsapp,
        }
    }

    pub fn task_source(self) -> TaskSource {
        match self {
            RemoteChannel::Telegram => TaskSource::Telegram,
            RemoteChannel::Whatsapp => TaskSource::Whatsapp,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteMessage {
    pub id: String,
    pub channel: RemoteChannel,
    pub external_message_id: String,
    pub sender_id: String,
    pub received_at: DateTime<Utc>,
    /// Allowlist dışı göndericide içerik saklanmaz (boş kalır).
    pub raw_text: String,
    pub authentication_state: RemoteAuthState,
    pub replay_state: RemoteReplayState,
    pub parsed_intent: Option<RemoteIntent>,
    pub resulting_inbox_item_id: Option<String>,
    pub processing_status: RemoteProcessingStatus,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewRemoteMessage {
    pub channel: RemoteChannel,
    pub external_message_id: String,
    pub sender_id: String,
    pub raw_text: String,
    pub authentication_state: RemoteAuthState,
}
