use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::enums::{EnergyLevel, TaskSource, TaskStatus};
use crate::serde_util::double_option;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub title: String,
    pub description: String,
    pub project_id: Option<String>,
    pub status: TaskStatus,
    pub priority: i64,
    pub importance: i64,
    pub urgency: i64,
    pub due_at: Option<DateTime<Utc>>,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub parent_task_id: Option<String>,
    pub tags: Vec<String>,
    pub source: TaskSource,
    pub waiting_for: Option<String>,
    pub waiting_since: Option<DateTime<Utc>>,
    pub followup_at: Option<DateTime<Utc>>,
    pub blocked_by: Option<String>,
    pub estimated_minutes: Option<i64>,
    pub energy_level: Option<EnergyLevel>,
    pub archived: bool,
    /// Listelerde kolaylık: proje adı (JOIN ile doldurulur, DB kolonu değil).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskCreate {
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub status: Option<TaskStatus>,
    #[serde(default)]
    pub priority: Option<i64>,
    #[serde(default)]
    pub importance: Option<i64>,
    #[serde(default)]
    pub urgency: Option<i64>,
    #[serde(default)]
    pub due_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub scheduled_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub parent_task_id: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub source: Option<TaskSource>,
    #[serde(default)]
    pub waiting_for: Option<String>,
    #[serde(default)]
    pub followup_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub blocked_by: Option<String>,
    #[serde(default)]
    pub estimated_minutes: Option<i64>,
    #[serde(default)]
    pub energy_level: Option<EnergyLevel>,
}

/// PATCH: alan yoksa dokunulmaz; `null` temizler (yalnızca temizlenebilir alanlarda).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskPatch {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub status: Option<TaskStatus>,
    #[serde(default)]
    pub priority: Option<i64>,
    #[serde(default)]
    pub importance: Option<i64>,
    #[serde(default)]
    pub urgency: Option<i64>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub project_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub due_at: Option<Option<DateTime<Utc>>>,
    #[serde(default, deserialize_with = "double_option")]
    pub scheduled_at: Option<Option<DateTime<Utc>>>,
    #[serde(default, deserialize_with = "double_option")]
    pub parent_task_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub waiting_for: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub followup_at: Option<Option<DateTime<Utc>>>,
    #[serde(default, deserialize_with = "double_option")]
    pub blocked_by: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub estimated_minutes: Option<Option<i64>>,
    #[serde(default, deserialize_with = "double_option")]
    pub energy_level: Option<Option<EnergyLevel>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskFilter {
    #[serde(default)]
    pub statuses: Option<Vec<TaskStatus>>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub include_archived: bool,
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
}
