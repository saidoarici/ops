//! Proje tarama orkestrasyonu: git anlık görüntüsü → önceki durumla fark →
//! evidence + tespitler + sağlık + aktivite. Tamamı deterministik ve salt-okunur;
//! observer proje dosyalarına asla YAZMAZ.

use std::collections::{BTreeSet, HashSet};
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use ops_core::models::{
    Actor, AuditResult, DetectedKind, EvidenceType, NewAudit, NewEvidence, Origin, Project,
    RepoState, RiskLevel, TaskFilter, TaskStatus,
};
use ops_core::store::Store;
use ops_core::{paths, OpsError};

use crate::{detect, gitscan, health};

#[derive(Debug, Default)]
pub struct ProjectScanOutcome {
    pub repos: usize,
    pub evidence_added: usize,
    pub errors: Vec<String>,
}

/// Onaylı köklerden repo köklerini çözer. `git_repositories` içindeki bir yol
/// onaylı köklerin (`local_paths`) dışına işaret ediyorsa gözlemlenmez
/// (`paths::ensure_within`; docs/threat-model.md T9/T10).
fn resolve_repo_roots(project: &Project) -> (Vec<PathBuf>, Vec<String>) {
    let mut errors = Vec::new();
    let mut approved: Vec<PathBuf> = Vec::new();
    for lp in &project.local_paths {
        match std::fs::canonicalize(lp) {
            Ok(p) if p.is_dir() => approved.push(p),
            Ok(_) => errors.push(format!("{lp}: klasör değil")),
            Err(e) => errors.push(format!("{lp}: erişilemedi ({e})")),
        }
    }
    let mut roots: BTreeSet<PathBuf> = BTreeSet::new();
    for ap in &approved {
        if ap.join(".git").exists() {
            roots.insert(ap.clone());
        }
    }
    'repo: for gr in &project.git_repositories {
        let candidate = PathBuf::from(gr);
        for ap in &approved {
            if let Ok(canon) = paths::ensure_within(ap, &candidate) {
                if canon.join(".git").exists() {
                    roots.insert(canon);
                } else {
                    errors.push(format!("{gr}: git reposu değil"));
                }
                continue 'repo;
            }
        }
        errors.push(format!("{gr}: onaylı proje kökleri dışında — gözlemlenmiyor"));
    }
    (roots.into_iter().collect(), errors)
}

pub fn scan_project(
    store: &Store,
    project: &Project,
    now: DateTime<Utc>,
) -> Result<ProjectScanOutcome, OpsError> {
    let mut out = ProjectScanOutcome::default();
    let (roots, mut path_errors) = resolve_repo_roots(project);
    out.errors.append(&mut path_errors);

    let mut active_keys: HashSet<String> = HashSet::new();

    for root in roots {
        out.repos += 1;
        let repo_path = root.display().to_string();
        let prev = store.get_repo_state(&project.id, &repo_path)?;
        let snap =
            match gitscan::snapshot(&root, prev.as_ref().and_then(|p| p.head_commit.as_deref())) {
                Ok(s) => s,
                Err(e) => {
                    out.errors.push(format!("{repo_path}: git okunamadı ({})", e.message()));
                    continue;
                }
            };

        // Yeni commit'ler → GIT_COMMIT evidence (ilk scan'de yalnızca son 5).
        let mut commit_evidence_ids: Vec<String> = Vec::new();
        let commits: Vec<&gitscan::CommitInfo> = if prev.is_none() {
            snap.new_commits.iter().take(5).collect()
        } else {
            snap.new_commits.iter().collect()
        };
        for c in &commits {
            if let Some(ev) = store.add_evidence(NewEvidence {
                task_id: None,
                project_id: Some(project.id.clone()),
                kind: EvidenceType::GitCommit,
                source: "observer:git".into(),
                timestamp: c.at,
                summary: format!("Commit: {}", c.summary),
                confidence: None,
                source_reference: Some(c.id.clone()),
                content_hash: Some(format!("commit:{}", c.id)),
            })? {
                out.evidence_added += 1;
                commit_evidence_ids.push(ev.id);
            }
        }

        // Temiz → kirli geçişi: dirty_since damgala + tek FILE_CHANGE evidence.
        let prev_dirty_since = prev.as_ref().and_then(|p| p.dirty_since);
        let dirty_since = if snap.dirty_files > 0 {
            match prev_dirty_since {
                Some(s) => Some(s),
                None => {
                    let day = now.format("%Y-%m-%d");
                    if store
                        .add_evidence(NewEvidence {
                            task_id: None,
                            project_id: Some(project.id.clone()),
                            kind: EvidenceType::FileChange,
                            source: "observer:git".into(),
                            timestamp: now,
                            summary: format!(
                                "Çalışma kopyasında {} dosya değişti (commit'lenmemiş)",
                                snap.dirty_files
                            ),
                            confidence: None,
                            source_reference: Some(repo_path.clone()),
                            content_hash: Some(format!("dirty-start:{repo_path}:{day}")),
                        })?
                        .is_some()
                    {
                        out.evidence_added += 1;
                    }
                    Some(now)
                }
            }
        } else {
            None
        };

        if let Some(t) = snap.last_commit_at {
            store.touch_project_activity(&project.id, t)?;
        }

        let new_state = RepoState {
            project_id: project.id.clone(),
            repo_path: repo_path.clone(),
            branch: snap.branch.clone(),
            head_commit: snap.head.clone(),
            dirty_files: snap.dirty_files,
            dirty_since,
            ahead: snap.ahead,
            last_commit_at: snap.last_commit_at,
            last_scan_at: now,
        };
        store.upsert_repo_state(&new_state)?;

        for mut d in [detect::uncommitted(now, &new_state), detect::unpushed(now, &new_state)]
            .into_iter()
            .flatten()
        {
            d.evidence_ids = commit_evidence_ids.clone();
            active_keys.insert(d.dedupe_key.clone());
            store.upsert_detected(d)?;
        }
    }

    // Sinyali kaybolan git tespitlerini kapat.
    store.resolve_missing_detected(
        DetectedKind::UncommittedChanges,
        Some(&project.id),
        &active_keys,
    )?;
    store.resolve_missing_detected(
        DetectedKind::UnpushedCommits,
        Some(&project.id),
        &active_keys,
    )?;

    refresh_health(store, &project.id, now)?;

    if out.evidence_added > 0 {
        store.append_audit(NewAudit {
            actor: Actor::Daemon,
            origin: Origin::Daemon,
            action: "OBSERVER_OBSERVATION".into(),
            target: Some(format!("project:{}", project.id)),
            risk_level: RiskLevel::R0,
            capability: Some("READ_GIT_METADATA".into()),
            result: AuditResult::Ok,
            metadata: serde_json::json!({ "evidence": out.evidence_added }),
        })?;
    }
    Ok(out)
}

/// Görev sayıları + aktivite yaşından deterministik health hesabı.
pub fn refresh_health(store: &Store, project_id: &str, now: DateTime<Utc>) -> Result<(), OpsError> {
    let project = store.get_project(project_id)?;
    let tasks = store.list_tasks(&TaskFilter {
        project_id: Some(project_id.to_string()),
        limit: Some(2000),
        ..Default::default()
    })?;
    let open: Vec<_> = tasks.iter().filter(|t| t.status.is_open()).collect();
    let inputs = health::HealthInputs {
        state: project.state,
        last_activity_at: project.last_activity_at.unwrap_or(project.created_at),
        stale_threshold_days: project.stale_threshold_days,
        open_total: open.len() as i64,
        in_progress: open.iter().filter(|t| t.status == TaskStatus::InProgress).count() as i64,
        next_or_planned: open
            .iter()
            .filter(|t| matches!(t.status, TaskStatus::Next | TaskStatus::Planned))
            .count() as i64,
        waiting: open.iter().filter(|t| t.status == TaskStatus::Waiting).count() as i64,
        blocked: open.iter().filter(|t| t.status == TaskStatus::Blocked).count() as i64,
        overdue: open.iter().filter(|t| t.due_at.map(|d| d < now).unwrap_or(false)).count() as i64,
    };
    store.set_project_health(project_id, health::compute(now, &inputs))?;
    Ok(())
}

/// IN_PROGRESS görevlerde durgunluk tespiti (tüm projeler, tek geçiş).
pub fn scan_stale_tasks(store: &Store, now: DateTime<Utc>) -> Result<usize, OpsError> {
    let thresholds: std::collections::HashMap<String, i64> = store
        .list_projects(false)?
        .into_iter()
        .map(|p| (p.project.id, p.project.stale_threshold_days))
        .collect();
    let tasks = store.list_tasks(&TaskFilter {
        statuses: Some(vec![TaskStatus::InProgress]),
        limit: Some(2000),
        ..Default::default()
    })?;
    let mut keys: HashSet<String> = HashSet::new();
    let mut opened = 0usize;
    for t in &tasks {
        let threshold =
            t.project_id.as_ref().and_then(|id| thresholds.get(id).copied()).unwrap_or(4);
        if let Some(d) = detect::stale_task(now, t, threshold) {
            keys.insert(d.dedupe_key.clone());
            if store.upsert_detected(d)? {
                opened += 1;
            }
        }
    }
    store.resolve_missing_detected(DetectedKind::StaleTask, None, &keys)?;
    Ok(opened)
}
