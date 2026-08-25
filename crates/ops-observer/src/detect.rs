//! "Yarım kalan iş" kuralları. Deterministiktir; sistem görev yaratmaz,
//! yalnızca öneri üretir (kullanıcı dönüştürür ya da yoksayar).

use chrono::{DateTime, Duration, Utc};
use ops_core::models::{DetectedKind, NewDetected, RepoState, Task};

fn day_word(days: i64) -> String {
    if days <= 0 {
        "bugün".into()
    } else {
        format!("{days} gündür")
    }
}

fn repo_label(repo_path: &str) -> String {
    std::path::Path::new(repo_path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| repo_path.to_string())
}

/// Commit'lenmemiş değişiklikler 24 saatten eskiyse tespit üret.
pub fn uncommitted(now: DateTime<Utc>, state: &RepoState) -> Option<NewDetected> {
    let since = state.dirty_since?;
    if state.dirty_files == 0 || now - since < Duration::hours(24) {
        return None;
    }
    let days = (now - since).num_days();
    let label = repo_label(&state.repo_path);
    Some(NewDetected {
        project_id: Some(state.project_id.clone()),
        task_id: None,
        kind: DetectedKind::UncommittedChanges,
        title: format!("{label}: commit'lenmemiş değişiklikler"),
        detail: format!(
            "{} dosya {} commit bekliyor ({})",
            state.dirty_files,
            day_word(days),
            state.repo_path
        ),
        evidence_ids: vec![],
        confidence: 0.85,
        suggested_task_title: Some(format!("{label} değişikliklerini gözden geçir ve commit'le")),
        dedupe_key: format!("uncommitted:{}:{}", state.project_id, state.repo_path),
    })
}

/// Push'lanmamış commit'ler 24 saatten eskiyse tespit üret (upstream varsa).
pub fn unpushed(now: DateTime<Utc>, state: &RepoState) -> Option<NewDetected> {
    if state.ahead <= 0 {
        return None;
    }
    let last = state.last_commit_at?;
    if now - last < Duration::hours(24) {
        return None;
    }
    let days = (now - last).num_days();
    let label = repo_label(&state.repo_path);
    Some(NewDetected {
        project_id: Some(state.project_id.clone()),
        task_id: None,
        kind: DetectedKind::UnpushedCommits,
        title: format!("{label}: push'lanmamış commit'ler"),
        detail: format!("{} commit {} lokalde duruyor", state.ahead, day_word(days)),
        evidence_ids: vec![],
        confidence: 0.8,
        suggested_task_title: Some(format!("{label} commit'lerini push'la")),
        dedupe_key: format!("unpushed:{}:{}", state.project_id, state.repo_path),
    })
}

/// IN_PROGRESS görev threshold'dan uzun süredir hareketsizse "muhtemelen yarım".
pub fn stale_task(now: DateTime<Utc>, task: &Task, threshold_days: i64) -> Option<NewDetected> {
    let idle = (now - task.updated_at).num_days();
    if idle < threshold_days {
        return None;
    }
    Some(NewDetected {
        project_id: task.project_id.clone(),
        task_id: Some(task.id.clone()),
        kind: DetectedKind::StaleTask,
        title: task.title.clone(),
        detail: format!(
            "Sürüyor görünüyor ama {idle} gündür aktivite yok — muhtemelen yarım kaldı"
        ),
        evidence_ids: vec![],
        confidence: 0.7,
        suggested_task_title: None,
        dedupe_key: format!("stale-task:{}", task.id),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ops_core::time;

    fn repo_state(dirty: i64, dirty_days_ago: i64, ahead: i64, commit_days_ago: i64) -> RepoState {
        let now = time::now();
        RepoState {
            project_id: "p1".into(),
            repo_path: "/tmp/proj/jontus".into(),
            branch: Some("main".into()),
            head_commit: Some("abc".into()),
            dirty_files: dirty,
            dirty_since: (dirty > 0).then(|| now - Duration::days(dirty_days_ago)),
            ahead,
            last_commit_at: Some(now - Duration::days(commit_days_ago)),
            last_scan_at: now,
        }
    }

    #[test]
    fn uncommitted_needs_24h() {
        let now = time::now();
        assert!(uncommitted(now, &repo_state(3, 0, 0, 0)).is_none());
        let d = uncommitted(now, &repo_state(3, 2, 0, 0)).unwrap();
        assert!(d.detail.contains("3 dosya"));
        assert_eq!(d.dedupe_key, "uncommitted:p1:/tmp/proj/jontus");
    }

    #[test]
    fn unpushed_needs_upstream_and_age() {
        let now = time::now();
        assert!(unpushed(now, &repo_state(0, 0, 0, 0)).is_none());
        assert!(unpushed(now, &repo_state(0, 0, 2, 0)).is_none()); // taze commit
        let d = unpushed(now, &repo_state(0, 0, 2, 3)).unwrap();
        assert!(d.detail.contains("2 commit"));
    }
}
