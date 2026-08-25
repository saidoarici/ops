//! Tüm domain enum'larının tek doğruluk kaynağı.
//! Wire (JSON) ve DB temsili aynıdır: SCREAMING_SNAKE_CASE string.
//! TS aynası: apps/desktop/src/lib/types.ts

use serde::{Deserialize, Serialize};
use strum::{AsRefStr, EnumString};

macro_rules! db_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumString, AsRefStr)]
        #[serde(rename_all = "SCREAMING_SNAKE_CASE")]
        #[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
        pub enum $name { $($variant),+ }
    };
}

db_enum!(TaskStatus {
    Inbox,
    Planned,
    Next,
    InProgress,
    Waiting,
    Blocked,
    Someday,
    Done,
    Cancelled,
});

impl TaskStatus {
    /// Açık uçlu (tamamlanmamış) statuslar.
    pub fn is_open(&self) -> bool {
        !matches!(self, TaskStatus::Done | TaskStatus::Cancelled)
    }
}

db_enum!(TaskSource { LocalUi, QuickCapture, Telegram, Whatsapp, AgentChat, AiDetected });

db_enum!(EnergyLevel { Low, Medium, High });

db_enum!(ProjectState { Active, Paused, Archived, Completed });

db_enum!(ProjectHealth { Active, Quiet, Stale, Blocked, Waiting, AtRisk, Completed });

db_enum!(ReminderStatus { Scheduled, Fired, Dismissed, Missed });

db_enum!(RepeatRule { None, Daily, Weekdays, Weekly, Monthly });

db_enum!(NotificationChannel { Macos, Telegram, Whatsapp });

db_enum!(RoutineAction { MorningBrief, EveningReview, WeeklyReview });

db_enum!(RemoteChannel { Telegram, Whatsapp });

db_enum!(RemoteAuthState { Authenticated, RejectedSender });

db_enum!(RemoteReplayState { New, Replayed });

db_enum!(RemoteProcessingStatus { Pending, Processed, Rejected });

db_enum!(AgentProviderKind { Claude, Codex });

db_enum!(AgentMode { Ask, Read, Edit, Act, Full });

impl AgentMode {
    /// Modun gerektirdiği capability (ASK için yok) ve risk seviyesi.
    pub fn capability(&self) -> (Option<&'static str>, RiskLevel) {
        match self {
            AgentMode::Ask => (None, RiskLevel::R0),
            AgentMode::Read => (Some("READ_PROJECT_FILES"), RiskLevel::R1),
            AgentMode::Edit => (Some("WRITE_PROJECT_FILES"), RiskLevel::R2),
            AgentMode::Act => (Some("RUN_APPROVED_TEST"), RiskLevel::R2),
            AgentMode::Full => (Some("FULL_LOCAL_ACCESS"), RiskLevel::R4),
        }
    }
}

db_enum!(AgentSessionStatus { Running, Completed, Failed, Cancelled });

db_enum!(AgentMessageRole { User, Assistant, Tool, System, Error });

db_enum!(EvidenceType { GitCommit, FileChange, AiSession, RoutineResult });

db_enum!(DetectedKind { UncommittedChanges, UnpushedCommits, StaleTask });

db_enum!(DetectedStatus { Open, Dismissed, Converted, Resolved });

db_enum!(Actor { User, Daemon, Scheduler, Remote });

db_enum!(Origin { LocalUi, Daemon, Cli, Telegram, Whatsapp });

db_enum!(RiskLevel { R0, R1, R2, R3, R4 });

db_enum!(AuditResult { Ok, Denied, Error });

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn screaming_snake_roundtrip() {
        assert_eq!(TaskStatus::InProgress.as_ref(), "IN_PROGRESS");
        assert_eq!(TaskStatus::from_str("IN_PROGRESS").unwrap(), TaskStatus::InProgress);
        assert_eq!(serde_json::to_string(&TaskStatus::InProgress).unwrap(), "\"IN_PROGRESS\"");
        assert_eq!(Origin::LocalUi.as_ref(), "LOCAL_UI");
        assert_eq!(RemoteAuthState::RejectedSender.as_ref(), "REJECTED_SENDER");
        assert_eq!(RiskLevel::R0.as_ref(), "R0");
    }
}
