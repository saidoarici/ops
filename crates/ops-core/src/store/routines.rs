use chrono::{DateTime, Utc};
use rusqlite::{params, Row};

use crate::models::{
    AuditResult, Ctx, NewAudit, RiskLevel, Routine, RoutineAction, RoutinePatch, RoutineSchedule,
};
use crate::store::{audit, dt, dt_opt, parse_enum, Store};
use crate::{time, OpsError, Result};

fn from_row(row: &Row<'_>) -> rusqlite::Result<Routine> {
    let last_result: Option<String> = row.get("last_result")?;
    Ok(Routine {
        id: row.get("id")?,
        name: row.get("name")?,
        enabled: row.get("enabled")?,
        schedule: row.get("schedule")?,
        action_type: parse_enum(&row.get::<_, String>("action_type")?)?,
        last_run_at: dt_opt(row.get("last_run_at")?)?,
        next_run_at: dt_opt(row.get("next_run_at")?)?,
        last_result: last_result.and_then(|s| serde_json::from_str(&s).ok()),
        created_at: dt(row.get("created_at")?)?,
        updated_at: dt(row.get("updated_at")?)?,
    })
}

impl Store {
    /// Yerleşik rutinleri (yoksa) oluşturur; daemon açılışında çağrılır.
    /// Rutinlerin tek yan etkisi bildirimdir (R0).
    pub fn ensure_builtin_routines(&self) -> Result<()> {
        let builtins: &[(&str, &str, RoutineAction, &str)] = &[
            ("morning_brief", "Sabah Brifingi", RoutineAction::MorningBrief, "09:00"),
            ("evening_review", "Akşam Değerlendirmesi", RoutineAction::EveningReview, "21:30"),
            ("weekly_review", "Haftalık Gözden Geçirme", RoutineAction::WeeklyReview, "MON 09:30"),
        ];
        let conn = self.db.conn();
        for (id, name, action, schedule) in builtins {
            conn.execute(
                "INSERT OR IGNORE INTO routines(id, name, enabled, schedule, action_type,
                    created_at, updated_at)
                 VALUES (?1, ?2, 1, ?3, ?4, ?5, ?5)",
                params![id, name, schedule, action.as_ref(), time::to_db(&time::now())],
            )?;
        }
        Ok(())
    }

    pub fn list_routines(&self) -> Result<Vec<Routine>> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare("SELECT * FROM routines ORDER BY name")?;
        let rows = stmt.query_map([], from_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn get_routine(&self, id: &str) -> Result<Routine> {
        let conn = self.db.conn();
        conn.query_row("SELECT * FROM routines WHERE id = ?1", [id], from_row).map_err(
            |e| match e {
                rusqlite::Error::QueryReturnedNoRows => OpsError::NotFound(format!("rutin: {id}")),
                other => other.into(),
            },
        )
    }

    pub fn update_routine(&self, ctx: &Ctx, id: &str, patch: RoutinePatch) -> Result<Routine> {
        if let Some(s) = &patch.schedule {
            RoutineSchedule::parse(s)?;
        }
        let now = time::to_db(&time::now());
        let mut conn = self.db.conn();
        let tx = conn.transaction()?;
        let mut changed: Vec<&str> = Vec::new();
        if let Some(enabled) = patch.enabled {
            tx.execute(
                "UPDATE routines SET enabled=?2, updated_at=?3 WHERE id=?1",
                params![id, enabled, now],
            )?;
            changed.push("enabled");
        }
        if let Some(schedule) = &patch.schedule {
            // Zamanlama değişince sıradaki koşu scheduler tarafından yeniden hesaplanır.
            tx.execute(
                "UPDATE routines SET schedule=?2, next_run_at=NULL, updated_at=?3 WHERE id=?1",
                params![id, schedule, now],
            )?;
            changed.push("schedule");
        }
        if !changed.is_empty() {
            audit::append_tx(
                &tx,
                &NewAudit {
                    actor: ctx.actor,
                    origin: ctx.origin,
                    action: "ROUTINE_UPDATE".into(),
                    target: Some(format!("routine:{id}")),
                    risk_level: RiskLevel::R0,
                    capability: None,
                    result: AuditResult::Ok,
                    metadata: serde_json::json!({ "changed": changed }),
                },
            )?;
        }
        tx.commit()?;
        drop(conn);
        self.get_routine(id)
    }

    pub fn set_routine_next_run(&self, id: &str, next: DateTime<Utc>) -> Result<()> {
        let conn = self.db.conn();
        conn.execute(
            "UPDATE routines SET next_run_at=?2 WHERE id=?1",
            params![id, time::to_db(&next)],
        )?;
        Ok(())
    }

    pub fn mark_routine_run(
        &self,
        id: &str,
        result: &serde_json::Value,
        next: DateTime<Utc>,
    ) -> Result<()> {
        let now = time::now();
        let conn = self.db.conn();
        conn.execute(
            "UPDATE routines SET last_run_at=?2, next_run_at=?3, last_result=?4, updated_at=?2
             WHERE id=?1",
            params![id, time::to_db(&now), time::to_db(&next), result.to_string()],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_routines_seed_once() {
        let s = Store::in_memory().unwrap();
        s.ensure_builtin_routines().unwrap();
        s.ensure_builtin_routines().unwrap();
        let routines = s.list_routines().unwrap();
        assert_eq!(routines.len(), 3);
        assert!(routines.iter().any(|r| r.action_type == RoutineAction::MorningBrief));
    }

    #[test]
    fn update_rejects_bad_schedule_and_resets_next_run() {
        let s = Store::in_memory().unwrap();
        s.ensure_builtin_routines().unwrap();
        s.set_routine_next_run("morning_brief", time::now()).unwrap();
        let bad = s.update_routine(
            &Ctx::LOCAL_USER,
            "morning_brief",
            RoutinePatch { schedule: Some("25:99".into()), ..Default::default() },
        );
        assert!(matches!(bad, Err(OpsError::Validation(_))));

        let ok = s
            .update_routine(
                &Ctx::LOCAL_USER,
                "morning_brief",
                RoutinePatch { schedule: Some("08:15".into()), enabled: Some(false) },
            )
            .unwrap();
        assert_eq!(ok.schedule, "08:15");
        assert!(!ok.enabled);
        assert!(ok.next_run_at.is_none(), "zamanlama değişince next_run sıfırlanır");
    }
}
