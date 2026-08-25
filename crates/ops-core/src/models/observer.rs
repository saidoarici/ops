use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::enums::{DetectedKind, DetectedStatus, EvidenceType};

// ---------------------------------------------------------------- Evidence

/// Bir gözlemin kaydı: commit, dosya hareketi, AI oturumu, rutin sonucu.
/// Observer yalnızca metadata yazar (özet, ad, sayı) — dosya içeriği asla.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Evidence {
    pub id: String,
    pub task_id: Option<String>,
    pub project_id: Option<String>,
    #[serde(rename = "type")]
    pub kind: EvidenceType,
    pub source: String,
    pub timestamp: DateTime<Utc>,
    pub summary: String,
    pub confidence: Option<f64>,
    pub source_reference: Option<String>,
    pub content_hash: Option<String>,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
}

/// Yeni evidence girdisi; `content_hash` doluysa aynı hash'li ikinci kayıt
/// sessizce yoksayılır (dedupe).
#[derive(Debug, Clone)]
pub struct NewEvidence {
    pub task_id: Option<String>,
    pub project_id: Option<String>,
    pub kind: EvidenceType,
    pub source: String,
    pub timestamp: DateTime<Utc>,
    pub summary: String,
    pub confidence: Option<f64>,
    pub source_reference: Option<String>,
    pub content_hash: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceFilter {
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub task_id: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
}

// ------------------------------------------------------------ DetectedWork

/// "Yarım kalan iş" tespiti: sistem görev yaratmaz, öneri sunar; kullanıcı
/// dönüştürür ya da yoksayar.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedWork {
    pub id: String,
    pub project_id: Option<String>,
    pub task_id: Option<String>,
    pub kind: DetectedKind,
    pub title: String,
    pub detail: String,
    pub evidence_ids: Vec<String>,
    pub confidence: f64,
    pub status: DetectedStatus,
    pub suggested_task_title: Option<String>,
    pub dedupe_key: String,
    pub first_detected_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewDetected {
    pub project_id: Option<String>,
    pub task_id: Option<String>,
    pub kind: DetectedKind,
    pub title: String,
    pub detail: String,
    pub evidence_ids: Vec<String>,
    pub confidence: f64,
    pub suggested_task_title: Option<String>,
    pub dedupe_key: String,
}

// --------------------------------------------------------------- RepoState

/// Bir git reposunun son bilinen durumu; scan'ler arası fark evidence üretir.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoState {
    pub project_id: String,
    pub repo_path: String,
    pub branch: Option<String>,
    pub head_commit: Option<String>,
    pub dirty_files: i64,
    pub dirty_since: Option<DateTime<Utc>>,
    pub ahead: i64,
    pub last_commit_at: Option<DateTime<Utc>>,
    pub last_scan_at: DateTime<Utc>,
}
