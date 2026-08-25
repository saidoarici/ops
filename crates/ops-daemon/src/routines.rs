//! Rutin motoru: Sabah Brifingi / Akşam Değerlendirmesi / Haftalık Gözden
//! Geçirme. İçerik tamamen deterministiktir (today engine + evidence + sayaçlar);
//! AI provider yoksa da aynen çalışır. Tek yan etkisi bildirimdir (R0).

use chrono::{DateTime, Duration, Utc};
use tracing::{info, warn};

use ops_core::models::{
    Actor, AuditResult, EvidenceFilter, EvidenceType, NewAudit, NewEvidence, NotificationChannel,
    Origin, RiskLevel, Routine, RoutineAction, RoutineSchedule, TaskFilter, TaskStatus,
};
use ops_core::{time, today};

use crate::{notify, AppState};

/// Kaçırılan rutin bu süreden eskiyse koşturulmaz, ileri kurulur (offline telafisi).
const MISSED_RUN_GRACE: Duration = Duration::hours(3);

fn local_offset() -> chrono::FixedOffset {
    *chrono::Local::now().offset()
}

/// `schedule` metni için `now`'dan sonraki ilk çalışma anı; geçersizse `None`.
pub fn compute_next_run(schedule: &str, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    RoutineSchedule::parse(schedule).ok()?.next_after(now, local_offset())
}

fn cut(s: &str, n: usize) -> String {
    let t: String = s.chars().take(n).collect();
    if t.len() < s.len() {
        format!("{t}…")
    } else {
        t
    }
}

/// Deterministik brief metni: kısa, gerçek, aksiyona dönük; motivasyon metni yok.
fn build_text(state: &AppState, action: RoutineAction, now: DateTime<Utc>) -> String {
    let view = match today::build(&state.store, now, local_offset()) {
        Ok(v) => v,
        Err(e) => return format!("Brifing üretilemedi: {e}"),
    };
    let evidence = state
        .store
        .list_evidence(&EvidenceFilter { limit: Some(200), ..Default::default() })
        .unwrap_or_default();
    let since = now - Duration::hours(24);
    let mut per_project: std::collections::BTreeMap<String, usize> = Default::default();
    for e in evidence.iter().filter(|e| e.timestamp >= since) {
        if let Some(name) = &e.project_name {
            *per_project.entry(name.clone()).or_default() += 1;
        }
    }
    let waiting = state
        .store
        .list_tasks(&TaskFilter { statuses: Some(vec![TaskStatus::Waiting]), ..Default::default() })
        .unwrap_or_default();
    let waiting_days =
        |t: &ops_core::models::Task| (now - t.waiting_since.unwrap_or(t.created_at)).num_days();

    let mut lines: Vec<String> = Vec::new();
    match action {
        RoutineAction::MorningBrief => {
            lines.push("☀️ Sabah brifingi".into());
            if per_project.is_empty() {
                lines.push("Dünden beri: yeni gözlem yok.".into());
            } else {
                let items: Vec<String> =
                    per_project.iter().map(|(name, n)| format!("{name} +{n}")).collect();
                lines.push(format!("Dünden beri: {}", items.join(" · ")));
            }
            if let Some(oldest) = waiting.iter().max_by_key(|t| waiting_days(t)) {
                lines.push(format!(
                    "Bekleyen: {} iş (en eskisi: {} — {} gün)",
                    waiting.len(),
                    cut(&oldest.title, 40),
                    waiting_days(oldest)
                ));
            }
            if view.stats.overdue > 0 {
                lines.push(format!("⚠️ {} geciken iş var.", view.stats.overdue));
            }
            if view.focus.is_empty() {
                lines.push("Bugün için odak önerisi yok.".into());
            } else {
                lines.push("Bugün:".into());
                for (i, f) in view.focus.iter().enumerate() {
                    lines.push(format!("{}. {} — {}", i + 1, cut(&f.task.title, 60), f.why_now));
                }
            }
        }
        RoutineAction::EveningReview => {
            lines.push("🌙 Akşam değerlendirmesi".into());
            lines.push(format!("Bugün biten: {} görev.", view.stats.done_today));
            let in_progress = state
                .store
                .list_tasks(&TaskFilter {
                    statuses: Some(vec![TaskStatus::InProgress]),
                    ..Default::default()
                })
                .map(|v| v.len())
                .unwrap_or(0);
            if in_progress > 0 {
                lines.push(format!("Hâlâ süren: {in_progress} iş."));
            }
            let detected = state.store.list_detected(false).map(|d| d.len()).unwrap_or(0);
            if detected > 0 {
                lines.push(format!("Yarım görünen: {detected} tespit — uygulamadan bak."));
            }
            if view.stats.inbox > 0 {
                lines.push(format!("Gelen kutusunda {} öğe triage bekliyor.", view.stats.inbox));
            }
            if !waiting.is_empty() {
                lines.push(format!("Cevap bekleyen: {} iş.", waiting.len()));
            }
        }
        RoutineAction::WeeklyReview => {
            lines.push("📆 Haftalık görünüm".into());
            let projects = state.store.list_projects(false).unwrap_or_default();
            if !projects.is_empty() {
                let items: Vec<String> = projects
                    .iter()
                    .take(6)
                    .map(|p| format!("{} {}", p.project.name, p.project.health.as_ref()))
                    .collect();
                lines.push(format!("Projeler: {}", items.join(" · ")));
            }
            let week_ago = now - Duration::days(7);
            let done_week = state
                .store
                .list_tasks(&TaskFilter {
                    statuses: Some(vec![TaskStatus::Done]),
                    ..Default::default()
                })
                .map(|v| v.iter().filter(|t| t.completed_at.is_some_and(|c| c >= week_ago)).count())
                .unwrap_or(0);
            lines.push(format!("Son 7 günde biten: {done_week} görev."));
            for w in waiting.iter().take(3) {
                lines.push(format!("• Bekliyor: {} — {} gün", cut(&w.title, 50), waiting_days(w)));
            }
        }
    }
    lines.join("\n")
}

/// Brifi macOS'a ve yapılandırılmış tüm uzak kanallara teslim eder.
async fn deliver_brief(
    state: &AppState,
    routine: &Routine,
    text: &str,
) -> Vec<NotificationChannel> {
    let mut channels = vec![NotificationChannel::Macos];
    channels.extend(state.remote.outbound_channels().await);
    notify::deliver(state, &channels, &routine.name, text, &format!("routine:{}", routine.id)).await
}

async fn run(
    state: &AppState,
    routine: &Routine,
    now: DateTime<Utc>,
) -> Result<String, ops_core::OpsError> {
    let text = build_text(state, routine.action_type, now);
    let channels = deliver_brief(state, routine, &text).await;
    let channel_names: Vec<&str> = channels.iter().map(|c| c.as_ref()).collect();
    let next = compute_next_run(&routine.schedule, now).unwrap_or(now + Duration::days(1));
    state.store.mark_routine_run(
        &routine.id,
        &serde_json::json!({ "summary": cut(&text, 300), "channels": channel_names }),
        next,
    )?;
    let _ = state.store.add_evidence(NewEvidence {
        task_id: None,
        project_id: None,
        kind: EvidenceType::RoutineResult,
        source: "routine".into(),
        timestamp: now,
        summary: format!("{}: {}", routine.name, cut(text.lines().nth(1).unwrap_or(&text), 120)),
        confidence: None,
        source_reference: Some(format!("routine:{}", routine.id)),
        content_hash: Some(format!("routine:{}:{}", routine.id, now.format("%Y-%m-%d-%H%M"))),
    });
    state.store.append_audit(NewAudit {
        actor: Actor::Scheduler,
        origin: Origin::Daemon,
        action: "ROUTINE_RUN".into(),
        target: Some(format!("routine:{}", routine.id)),
        risk_level: RiskLevel::R0,
        capability: Some("SEND_NOTIFICATION".into()),
        result: AuditResult::Ok,
        metadata: serde_json::json!({ "channels": channel_names }),
    })?;
    Ok(text)
}

/// Kullanıcının "şimdi çalıştır" isteği: zamanlamadan bağımsız koşar, metni döner.
pub async fn run_routine_now(state: &AppState, id: &str) -> Result<String, ops_core::OpsError> {
    let routine = state.store.get_routine(id)?;
    run(state, &routine, time::now()).await
}

/// Scheduler tick'inden çağrılır: vadesi gelen rutinleri koşturur.
pub async fn run_due_routines(state: &AppState) {
    let now = time::now();
    let routines = match state.store.list_routines() {
        Ok(r) => r,
        Err(e) => {
            warn!(error = %e, "rutinler listelenemedi");
            return;
        }
    };
    for routine in routines.into_iter().filter(|r| r.enabled) {
        match routine.next_run_at {
            None => {
                if let Some(next) = compute_next_run(&routine.schedule, now) {
                    let _ = state.store.set_routine_next_run(&routine.id, next);
                }
            }
            Some(next) if now - next > MISSED_RUN_GRACE => {
                if let Some(n) = compute_next_run(&routine.schedule, now) {
                    let _ = state.store.set_routine_next_run(&routine.id, n);
                }
            }
            Some(next) if next <= now => {
                info!(routine = %routine.id, "rutin koşuyor");
                if let Err(e) = run(state, &routine, now).await {
                    warn!(error = %e, "rutin sonucu yazılamadı");
                }
            }
            Some(_) => {}
        }
    }
}
