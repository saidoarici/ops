//! Deterministik proje sağlığı: görev sayıları ve aktivite yaşından hesaplanır; AI yok.

use chrono::{DateTime, Utc};
use ops_core::models::{ProjectHealth, ProjectState};

#[derive(Debug, Clone)]
pub struct HealthInputs {
    pub state: ProjectState,
    /// Aktivite referansı (yoksa proje oluşturulma zamanı).
    pub last_activity_at: DateTime<Utc>,
    pub stale_threshold_days: i64,
    pub open_total: i64,
    pub in_progress: i64,
    pub next_or_planned: i64,
    pub waiting: i64,
    pub blocked: i64,
    pub overdue: i64,
}

/// Kural sırası bilinçli: engel > bekleme > risk > durgunluk > sessizlik.
pub fn compute(now: DateTime<Utc>, i: &HealthInputs) -> ProjectHealth {
    match i.state {
        ProjectState::Completed => return ProjectHealth::Completed,
        ProjectState::Paused => return ProjectHealth::Quiet,
        ProjectState::Archived | ProjectState::Active => {}
    }
    let idle_days = (now - i.last_activity_at).num_days();
    if i.blocked > 0 && i.in_progress == 0 && i.next_or_planned == 0 {
        return ProjectHealth::Blocked;
    }
    if i.open_total > 0 && i.waiting == i.open_total {
        return ProjectHealth::Waiting;
    }
    if i.overdue > 0 {
        return ProjectHealth::AtRisk;
    }
    if idle_days >= i.stale_threshold_days {
        return ProjectHealth::Stale;
    }
    if idle_days >= 2 {
        return ProjectHealth::Quiet;
    }
    ProjectHealth::Active
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn base(now: DateTime<Utc>) -> HealthInputs {
        HealthInputs {
            state: ProjectState::Active,
            last_activity_at: now,
            stale_threshold_days: 4,
            open_total: 3,
            in_progress: 1,
            next_or_planned: 1,
            waiting: 1,
            blocked: 0,
            overdue: 0,
        }
    }

    #[test]
    fn health_rules() {
        let now = Utc::now();
        assert_eq!(compute(now, &base(now)), ProjectHealth::Active);

        let mut i = base(now);
        i.last_activity_at = now - Duration::days(3);
        assert_eq!(compute(now, &i), ProjectHealth::Quiet);
        i.last_activity_at = now - Duration::days(5);
        assert_eq!(compute(now, &i), ProjectHealth::Stale);

        let mut i = base(now);
        i.overdue = 1;
        assert_eq!(compute(now, &i), ProjectHealth::AtRisk);

        let mut i = base(now);
        i.blocked = 1;
        i.in_progress = 0;
        i.next_or_planned = 0;
        assert_eq!(compute(now, &i), ProjectHealth::Blocked);

        let mut i = base(now);
        i.waiting = 3;
        i.in_progress = 0;
        i.next_or_planned = 0;
        assert_eq!(compute(now, &i), ProjectHealth::Waiting);

        let mut i = base(now);
        i.state = ProjectState::Completed;
        assert_eq!(compute(now, &i), ProjectHealth::Completed);
    }
}
