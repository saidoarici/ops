use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::enums::{ProjectHealth, ProjectState};
use crate::serde_util::double_option;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub description: String,
    pub state: ProjectState,
    pub health: ProjectHealth,
    pub priority: i64,
    pub local_paths: Vec<String>,
    pub git_repositories: Vec<String>,
    pub keywords: Vec<String>,
    pub related_contacts: Vec<String>,
    pub last_activity_at: Option<DateTime<Utc>>,
    pub stale_threshold_days: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Liste görünümü: proje + türetilmiş sayaçlar.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWithStats {
    #[serde(flatten)]
    pub project: Project,
    pub open_tasks: i64,
    pub waiting_tasks: i64,
    pub inbox_tasks: i64,
    pub last_task_activity: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectCreate {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub priority: Option<i64>,
    #[serde(default)]
    pub local_paths: Option<Vec<String>>,
    #[serde(default)]
    pub git_repositories: Option<Vec<String>>,
    #[serde(default)]
    pub keywords: Option<Vec<String>>,
    #[serde(default)]
    pub related_contacts: Option<Vec<String>>,
    #[serde(default)]
    pub stale_threshold_days: Option<i64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectPatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub state: Option<ProjectState>,
    #[serde(default)]
    pub health: Option<ProjectHealth>,
    #[serde(default)]
    pub priority: Option<i64>,
    #[serde(default)]
    pub local_paths: Option<Vec<String>>,
    #[serde(default)]
    pub git_repositories: Option<Vec<String>>,
    #[serde(default)]
    pub keywords: Option<Vec<String>>,
    #[serde(default)]
    pub related_contacts: Option<Vec<String>>,
    #[serde(default)]
    pub stale_threshold_days: Option<i64>,
    #[serde(default, deserialize_with = "double_option")]
    pub last_activity_at: Option<Option<DateTime<Utc>>>,
}
