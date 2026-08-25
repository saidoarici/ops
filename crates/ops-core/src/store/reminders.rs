use chrono::{DateTime, Datelike, Duration, Utc, Weekday};
use rusqlite::{params, Connection, Row};

use crate::models::{
    AuditResult, Ctx, NewAudit, NotificationChannel, Reminder, ReminderCreate, ReminderFilter,
    ReminderPatch, ReminderStatus, RepeatRule, RiskLevel,
};
use crate::store::{audit, check_text, dt, dt_opt, json_list, parse_enum, Store};
use crate::{time, OpsError, Result};

fn from_row(row: &Row<'_>) -> rusqlite::Result<Reminder> {
    Ok(Reminder {
        id: row.get("id")?,
        task_id: row.get("task_id")?,
        title: row.get("title")?,
        notes: row.get("notes")?,
        remind_at: dt(row.get("remind_at")?)?,
        repeat_rule: parse_enum(&row.get::<_, String>("repeat_rule")?)?,
        channels: json_list(row.get("channels")?)?,
        status: parse_enum(&row.get::<_, String>("status")?)?,
        fired_at: dt_opt(row.get("fired_at")?)?,
        created_at: dt(row.get("created_at")?)?,
        updated_at: dt(row.get("updated_at")?)?,
    })
}

fn get_in(conn: &Connection, id: &str) -> Result<Reminder> {
    conn.query_row("SELECT * FROM reminders WHERE id = ?1", [id], from_row).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => OpsError::NotFound(format!("hatırlatma: {id}")),
        other => other.into(),
    })
}

/// Tekrar kuralına göre bir sonraki tetikleme zamanı; `now`'dan sonraya
/// düşene kadar ilerletir (daemon uzun süre kapalı kalmış olabilir).
pub fn next_occurrence(rule: RepeatRule, from: DateTime<Utc>, now: DateTime<Utc>) -> DateTime<Utc> {
    let step = |t: DateTime<Utc>| -> DateTime<Utc> {
        match rule {
            RepeatRule::None => t,
            RepeatRule::Daily => t + Duration::days(1),
            RepeatRule::Weekdays => {
                let mut n = t + Duration::days(1);
                while matches!(n.weekday(), Weekday::Sat | Weekday::Sun) {
                    n += Duration::days(1);
                }
                n
            }
            RepeatRule::Weekly => t + Duration::days(7),
            RepeatRule::Monthly => t
                .checked_add_months(chrono::Months::new(1))
                .unwrap_or_else(|| t + Duration::days(30)),
        }
    };
    let mut next = from;
    for _ in 0..1000 {
        next = step(next);
        if next > now {
            break;
        }
    }
    next
}

fn write_update(conn: &Connection, r: &Reminder) -> Result<()> {
    conn.execute(
        "UPDATE reminders SET task_id=?2, title=?3, notes=?4, remind_at=?5, repeat_rule=?6,
            channels=?7, status=?8, fired_at=?9, updated_at=?10
         WHERE id=?1",
        params![
            r.id,
            r.task_id,
            r.title,
            r.notes,
            time::to_db(&r.remind_at),
            r.repeat_rule.as_ref(),
            serde_json::to_string(&r.channels)?,
            r.status.as_ref(),
            time::opt_to_db(&r.fired_at),
            time::to_db(&r.updated_at),
        ],
    )?;
    Ok(())
}

impl Store {
    pub fn create_reminder(&self, ctx: &Ctx, input: ReminderCreate) -> Result<Reminder> {
        let title = check_text("hatırlatma başlığı", &input.title, 300)?;
        let now = time::now();
        let reminder = Reminder {
            id: uuid::Uuid::new_v4().to_string(),
            task_id: input.task_id.clone(),
            title,
            notes: input.notes.unwrap_or_default(),
            remind_at: input.remind_at,
            repeat_rule: input.repeat_rule.unwrap_or(RepeatRule::None),
            channels: input
                .channels
                .filter(|c| !c.is_empty())
                .unwrap_or_else(|| vec![NotificationChannel::Macos]),
            status: ReminderStatus::Scheduled,
            fired_at: None,
            created_at: now,
            updated_at: now,
        };
        let mut conn = self.db.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO reminders(id, task_id, title, notes, remind_at, repeat_rule, channels,
                status, fired_at, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![
                reminder.id,
                reminder.task_id,
                reminder.title,
                reminder.notes,
                time::to_db(&reminder.remind_at),
                reminder.repeat_rule.as_ref(),
                serde_json::to_string(&reminder.channels)?,
                reminder.status.as_ref(),
                time::opt_to_db(&reminder.fired_at),
                time::to_db(&reminder.created_at),
                time::to_db(&reminder.updated_at),
            ],
        )?;
        audit::append_tx(
            &tx,
            &NewAudit {
                actor: ctx.actor,
                origin: ctx.origin,
                action: "REMINDER_CREATE".into(),
                target: Some(format!("reminder:{}", reminder.id)),
                risk_level: RiskLevel::R0,
                capability: Some("CREATE_REMINDER".into()),
                result: AuditResult::Ok,
                metadata: serde_json::json!({ "repeat": reminder.repeat_rule.as_ref() }),
            },
        )?;
        tx.commit()?;
        Ok(reminder)
    }

    pub fn list_reminders(&self, filter: &ReminderFilter) -> Result<Vec<Reminder>> {
        let mut sql = String::from("SELECT * FROM reminders WHERE 1=1");
        let mut values: Vec<rusqlite::types::Value> = Vec::new();
        if let Some(statuses) = &filter.statuses {
            if !statuses.is_empty() {
                let marks: Vec<String> = statuses
                    .iter()
                    .map(|s| {
                        values.push(rusqlite::types::Value::Text(s.as_ref().to_string()));
                        format!("?{}", values.len())
                    })
                    .collect();
                sql.push_str(&format!(" AND status IN ({})", marks.join(",")));
            }
        }
        if let Some(from) = &filter.from {
            values.push(rusqlite::types::Value::Text(time::to_db(from)));
            sql.push_str(&format!(" AND remind_at >= ?{}", values.len()));
        }
        if let Some(to) = &filter.to {
            values.push(rusqlite::types::Value::Text(time::to_db(to)));
            sql.push_str(&format!(" AND remind_at < ?{}", values.len()));
        }
        sql.push_str(" ORDER BY remind_at ASC");
        let limit = filter.limit.unwrap_or(200).clamp(1, 1000);
        sql.push_str(&format!(" LIMIT {limit}"));

        let conn = self.db.conn();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(values.iter()), from_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn update_reminder(&self, ctx: &Ctx, id: &str, patch: ReminderPatch) -> Result<Reminder> {
        let now = time::now();
        let mut conn = self.db.conn();
        let tx = conn.transaction()?;
        let mut r = get_in(&tx, id)?;
        let mut changed: Vec<&'static str> = Vec::new();

        if let Some(v) = patch.title {
            r.title = check_text("hatırlatma başlığı", &v, 300)?;
            changed.push("title");
        }
        if let Some(v) = patch.notes {
            r.notes = v;
            changed.push("notes");
        }
        if let Some(v) = patch.remind_at {
            r.remind_at = v;
            changed.push("remindAt");
            // Zaman güncellenirse ve açık bir statü verilmemişse yeniden kurulur.
            if patch.status.is_none() {
                r.status = ReminderStatus::Scheduled;
            }
        }
        if let Some(v) = patch.repeat_rule {
            r.repeat_rule = v;
            changed.push("repeatRule");
        }
        if let Some(v) = patch.channels {
            if !v.is_empty() {
                r.channels = v;
                changed.push("channels");
            }
        }
        if let Some(v) = patch.status {
            r.status = v;
            changed.push("status");
        }
        if let Some(v) = patch.task_id {
            r.task_id = v;
            changed.push("taskId");
        }

        if changed.is_empty() {
            tx.commit()?;
            return Ok(r);
        }
        r.updated_at = now;
        write_update(&tx, &r)?;
        audit::append_tx(
            &tx,
            &NewAudit {
                actor: ctx.actor,
                origin: ctx.origin,
                action: "REMINDER_UPDATE".into(),
                target: Some(format!("reminder:{id}")),
                risk_level: RiskLevel::R0,
                capability: Some("CREATE_REMINDER".into()),
                result: AuditResult::Ok,
                metadata: serde_json::json!({ "changed": changed }),
            },
        )?;
        tx.commit()?;
        Ok(r)
    }

    pub fn dismiss_reminder(&self, ctx: &Ctx, id: &str) -> Result<Reminder> {
        self.update_reminder(
            ctx,
            id,
            ReminderPatch { status: Some(ReminderStatus::Dismissed), ..Default::default() },
        )
    }

    /// Vadesi gelen hatırlatmaları tetikler; bildirim gönderimi çağıranın işi.
    /// Dönen listede `remind_at` orijinal (tetiklenen) zamandır.
    pub fn fire_due_reminders(&self, now: DateTime<Utc>) -> Result<Vec<Reminder>> {
        let mut conn = self.db.conn();
        let tx = conn.transaction()?;
        let due: Vec<Reminder> = {
            let mut stmt = tx.prepare(
                "SELECT * FROM reminders WHERE status = 'SCHEDULED' AND remind_at <= ?1
                 ORDER BY remind_at ASC",
            )?;
            let rows = stmt.query_map([time::to_db(&now)], from_row)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        for r in &due {
            let mut updated = r.clone();
            updated.fired_at = Some(now);
            updated.updated_at = now;
            if r.repeat_rule == RepeatRule::None {
                updated.status = ReminderStatus::Fired;
            } else {
                updated.remind_at = next_occurrence(r.repeat_rule, r.remind_at, now);
            }
            write_update(&tx, &updated)?;
            audit::append_tx(
                &tx,
                &NewAudit {
                    actor: Ctx::SCHEDULER.actor,
                    origin: Ctx::SCHEDULER.origin,
                    action: "REMINDER_FIRE".into(),
                    target: Some(format!("reminder:{}", r.id)),
                    risk_level: RiskLevel::R0,
                    capability: Some("CREATE_REMINDER".into()),
                    result: AuditResult::Ok,
                    metadata: serde_json::json!({ "repeat": r.repeat_rule.as_ref() }),
                },
            )?;
        }
        tx.commit()?;
        Ok(due)
    }

    /// Daemon kapalıyken 24 saatten fazla gecikenler MISSED işaretlenir;
    /// daha tazeler ilk tick'te normal tetiklenir (docs/data-model.md, reminders).
    pub fn mark_missed_reminders(&self, now: DateTime<Utc>) -> Result<i64> {
        let cutoff = now - Duration::hours(24);
        let mut conn = self.db.conn();
        let tx = conn.transaction()?;
        let ids: Vec<String> = {
            let mut stmt = tx.prepare(
                "SELECT id FROM reminders WHERE status = 'SCHEDULED' AND remind_at <= ?1",
            )?;
            let rows = stmt.query_map([time::to_db(&cutoff)], |r| r.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        for id in &ids {
            tx.execute(
                "UPDATE reminders SET status='MISSED', updated_at=?2 WHERE id=?1",
                params![id, time::to_db(&now)],
            )?;
            audit::append_tx(
                &tx,
                &NewAudit {
                    actor: Ctx::SCHEDULER.actor,
                    origin: Ctx::SCHEDULER.origin,
                    action: "REMINDER_MISSED".into(),
                    target: Some(format!("reminder:{id}")),
                    risk_level: RiskLevel::R0,
                    capability: None,
                    result: AuditResult::Ok,
                    metadata: serde_json::json!({}),
                },
            )?;
        }
        tx.commit()?;
        Ok(ids.len() as i64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(s: &str) -> DateTime<Utc> {
        crate::time::from_db(s).unwrap()
    }

    #[test]
    fn next_occurrence_rules() {
        let now = utc("2026-08-23T10:00:00Z"); // Pazar
                                               // Günlük: ertesi gün
        assert_eq!(
            next_occurrence(RepeatRule::Daily, utc("2026-08-23T09:00:00Z"), now),
            utc("2026-08-24T09:00:00Z")
        );
        // Hafta içi: cuma → pazartesi
        assert_eq!(
            next_occurrence(RepeatRule::Weekdays, utc("2026-08-21T09:00:00Z"), now),
            utc("2026-08-24T09:00:00Z")
        );
        // Uzun kapalılık: geçmişte kalan günlük hatırlatma now sonrasına ilerler
        let n = next_occurrence(RepeatRule::Daily, utc("2026-08-01T09:00:00Z"), now);
        assert!(n > now);
        assert_eq!(n, utc("2026-08-24T09:00:00Z"));
    }
}
