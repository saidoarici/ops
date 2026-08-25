//! Today ekranının deterministik beyni: odak seçimi, dikkat listesi, timeline.
//! Burada AI yoktur — due date, bekleme süresi, stale süresi gibi hesaplar
//! LLM gerektirmez (docs/architecture.md, "Deterministic core").

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Duration, FixedOffset, TimeZone, Utc};
use serde::Serialize;

use crate::models::{
    DetectedKind, DetectedWork, ReminderFilter, ReminderStatus, Task, TaskFilter, TaskStatus,
};
use crate::store::Store;
use crate::Result;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TodayView {
    pub generated_at: DateTime<Utc>,
    pub day_start: DateTime<Utc>,
    pub day_end: DateTime<Utc>,
    pub focus: Vec<FocusItem>,
    pub needs_attention: Vec<AttentionItem>,
    /// "Dünden beri tespit edilenler": observer'ın git-türevi bulguları
    /// (stale-task tespitleri zaten "needs attention" içinde temsil edilir).
    pub detected: Vec<DetectedWork>,
    pub timeline: Vec<TimelineItem>,
    pub stats: TodayStats,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FocusItem {
    pub task: Task,
    pub why_now: String,
    pub score: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AttentionKind {
    Overdue,
    WaitingLong,
    Blocked,
    Stale,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttentionItem {
    pub task: Task,
    pub kind: AttentionKind,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TimelineKind {
    Reminder,
    Due,
    Scheduled,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineItem {
    pub at: DateTime<Utc>,
    pub kind: TimelineKind,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reminder_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TodayStats {
    pub open_tasks: i64,
    pub inbox: i64,
    pub waiting: i64,
    pub due_today: i64,
    pub overdue: i64,
    pub done_today: i64,
}

struct ProjectMeta {
    priority: i64,
    stale_threshold_days: i64,
    name: String,
}

pub fn build(store: &Store, now: DateTime<Utc>, offset: FixedOffset) -> Result<TodayView> {
    let local_now = now.with_timezone(&offset);
    let day_start_naive = local_now.date_naive().and_hms_opt(0, 0, 0).expect("gün başı");
    let day_start = offset
        .from_local_datetime(&day_start_naive)
        .single()
        .expect("sabit offset tekildir")
        .with_timezone(&Utc);
    let day_end = day_start + Duration::days(1);

    let projects: HashMap<String, ProjectMeta> = store
        .list_projects(false)?
        .into_iter()
        .map(|p| {
            (
                p.project.id.clone(),
                ProjectMeta {
                    priority: p.project.priority,
                    stale_threshold_days: p.project.stale_threshold_days,
                    name: p.project.name,
                },
            )
        })
        .collect();

    // Arşivsiz tüm görevler; bölümleme bellek içinde (tek kullanıcı ölçeği).
    let all = store.list_tasks(&TaskFilter { limit: Some(2000), ..Default::default() })?;
    let open: Vec<&Task> = all.iter().filter(|t| t.status.is_open()).collect();

    let days_between = |a: DateTime<Utc>, b: DateTime<Utc>| -> i64 {
        (a.with_timezone(&offset).date_naive() - b.with_timezone(&offset).date_naive()).num_days()
    };

    // ---------------------------------------------------------------- Focus
    let mut scored: Vec<FocusItem> = Vec::new();
    for t in &open {
        let candidate = match t.status {
            TaskStatus::InProgress | TaskStatus::Next => true,
            TaskStatus::Planned => t.scheduled_at.map(|s| s < day_end).unwrap_or(true),
            // Inbox/Waiting/Blocked/Someday odakta yer almaz; gecikmişse
            // "needs attention" bölümünde görünür.
            _ => false,
        };
        if !candidate {
            continue;
        }

        let mut score = t.importance * 2 + t.urgency * 2 + t.priority;
        let mut reasons: Vec<(i64, String)> = Vec::new(); // (ağırlık, metin)

        let pmeta = t.project_id.as_ref().and_then(|id| projects.get(id));
        if let Some(p) = pmeta {
            score += p.priority;
            if p.priority >= 4 {
                reasons.push((2, format!("öncelikli proje: {}", p.name)));
            }
        }
        if let Some(due) = t.due_at {
            let d = days_between(due, now);
            if d < 0 {
                let late = -d;
                score += 8 + late.min(7);
                reasons.push((10 + late.min(7), format!("{late} gün gecikti")));
            } else if d == 0 {
                score += 6;
                reasons.push((9, "bugün son gün".into()));
            } else if d == 1 {
                score += 3;
                reasons.push((6, "yarın son gün".into()));
            } else if d <= 3 {
                score += 1;
                reasons.push((3, format!("{d} gün içinde bitmeli")));
            }
        }
        if let Some(s) = t.scheduled_at {
            if s >= day_start && s < day_end {
                score += 2;
                reasons.push((5, "bugüne planlandı".into()));
            }
        }
        if t.status == TaskStatus::InProgress {
            score += 2;
            let idle = days_between(now, t.updated_at);
            if idle >= 2 {
                reasons.push((4, format!("{idle} gündür dokunulmadı")));
            } else {
                reasons.push((2, "devam eden iş".into()));
            }
        }
        if t.importance >= 4 {
            reasons.push((1, "yüksek önem".into()));
        }
        if reasons.is_empty() {
            reasons.push((0, "sıradaki en mantıklı iş".into()));
        }
        reasons.sort_by_key(|(weight, _)| std::cmp::Reverse(*weight));
        let why_now = reasons.into_iter().take(2).map(|(_, s)| s).collect::<Vec<_>>().join(" · ");
        scored.push(FocusItem { task: (*t).clone(), why_now, score });
    }
    scored.sort_by(|a, b| b.score.cmp(&a.score).then(a.task.created_at.cmp(&b.task.created_at)));
    let focus: Vec<FocusItem> = scored.into_iter().take(3).collect();
    let focus_ids: HashSet<&str> = focus.iter().map(|f| f.task.id.as_str()).collect();

    // ------------------------------------------------------- Needs attention
    let mut attention: Vec<(i64, AttentionItem)> = Vec::new();
    for t in &open {
        if focus_ids.contains(t.id.as_str()) {
            continue;
        }
        if let Some(due) = t.due_at {
            let late = -days_between(due, now);
            if late > 0 {
                attention.push((
                    100 + late,
                    AttentionItem {
                        task: (*t).clone(),
                        kind: AttentionKind::Overdue,
                        detail: format!("{late} gün gecikti"),
                    },
                ));
                continue;
            }
        }
        match t.status {
            TaskStatus::Waiting => {
                let since = t.waiting_since.unwrap_or(t.created_at);
                let days = days_between(now, since).max(0);
                let followup_due = t.followup_at.map(|f| f <= now).unwrap_or(false);
                if days >= 5 || followup_due {
                    let mut detail = match &t.waiting_for {
                        Some(w) => format!("{days} gündür bekleniyor · {w}"),
                        None => format!("{days} gündür bekleniyor"),
                    };
                    if followup_due {
                        detail.push_str(" · takip zamanı geldi");
                    }
                    attention.push((
                        50 + days,
                        AttentionItem {
                            task: (*t).clone(),
                            kind: AttentionKind::WaitingLong,
                            detail,
                        },
                    ));
                }
            }
            TaskStatus::Blocked => {
                let detail = t.blocked_by.clone().unwrap_or_else(|| "bloklu".into());
                attention.push((
                    40,
                    AttentionItem { task: (*t).clone(), kind: AttentionKind::Blocked, detail },
                ));
            }
            TaskStatus::InProgress => {
                let threshold = t
                    .project_id
                    .as_ref()
                    .and_then(|id| projects.get(id))
                    .map(|p| p.stale_threshold_days)
                    .unwrap_or(4);
                let idle = days_between(now, t.updated_at);
                if idle >= threshold {
                    attention.push((
                        30 + idle,
                        AttentionItem {
                            task: (*t).clone(),
                            kind: AttentionKind::Stale,
                            detail: format!("{idle} gündür aktivite yok"),
                        },
                    ));
                }
            }
            _ => {}
        }
    }
    attention.sort_by_key(|(weight, _)| std::cmp::Reverse(*weight));
    let needs_attention: Vec<AttentionItem> =
        attention.into_iter().map(|(_, i)| i).take(10).collect();

    // ------------------------------------------------------ Detected work
    let detected: Vec<DetectedWork> = store
        .list_detected(false)?
        .into_iter()
        .filter(|d| d.kind != DetectedKind::StaleTask)
        .filter(|d| now - d.last_seen_at < Duration::hours(48))
        .take(6)
        .collect();

    // --------------------------------------------------------------- Timeline
    let mut timeline: Vec<TimelineItem> = Vec::new();
    let reminders = store.list_reminders(&ReminderFilter {
        from: Some(day_start),
        to: Some(day_end),
        statuses: Some(vec![
            ReminderStatus::Scheduled,
            ReminderStatus::Fired,
            ReminderStatus::Missed,
        ]),
        limit: Some(100),
    })?;
    for r in reminders {
        timeline.push(TimelineItem {
            at: r.remind_at,
            kind: TimelineKind::Reminder,
            title: r.title.clone(),
            task_id: r.task_id.clone(),
            reminder_id: Some(r.id.clone()),
            status: Some(r.status.as_ref().to_string()),
        });
    }
    for t in &open {
        let due_today = t.due_at.map(|d| d >= day_start && d < day_end).unwrap_or(false);
        if due_today {
            timeline.push(TimelineItem {
                at: t.due_at.unwrap(),
                kind: TimelineKind::Due,
                title: t.title.clone(),
                task_id: Some(t.id.clone()),
                reminder_id: None,
                status: Some(t.status.as_ref().to_string()),
            });
        } else if let Some(s) = t.scheduled_at {
            if s >= day_start && s < day_end {
                timeline.push(TimelineItem {
                    at: s,
                    kind: TimelineKind::Scheduled,
                    title: t.title.clone(),
                    task_id: Some(t.id.clone()),
                    reminder_id: None,
                    status: Some(t.status.as_ref().to_string()),
                });
            }
        }
    }
    timeline.sort_by_key(|item| item.at);

    // ------------------------------------------------------------------ Stats
    let stats = TodayStats {
        open_tasks: open.len() as i64,
        inbox: open.iter().filter(|t| t.status == TaskStatus::Inbox).count() as i64,
        waiting: open.iter().filter(|t| t.status == TaskStatus::Waiting).count() as i64,
        due_today: open
            .iter()
            .filter(|t| t.due_at.map(|d| d >= day_start && d < day_end).unwrap_or(false))
            .count() as i64,
        overdue: open.iter().filter(|t| t.due_at.map(|d| d < now).unwrap_or(false)).count() as i64,
        done_today: all
            .iter()
            .filter(|t| {
                t.status == TaskStatus::Done
                    && t.completed_at.map(|c| c >= day_start && c < day_end).unwrap_or(false)
            })
            .count() as i64,
    };

    Ok(TodayView {
        generated_at: now,
        day_start,
        day_end,
        focus,
        needs_attention,
        detected,
        timeline,
        stats,
    })
}
