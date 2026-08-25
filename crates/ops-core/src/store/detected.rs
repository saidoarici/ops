use std::collections::HashSet;

use rusqlite::{params, Connection, Row};

use crate::models::{
    AuditResult, Ctx, DetectedKind, DetectedStatus, DetectedWork, NewAudit, NewDetected, RiskLevel,
    Task, TaskCreate, TaskSource,
};
use crate::store::{audit, dt, dt_opt, json_list, parse_enum, Store};
use crate::{time, OpsError, Result};

fn from_row(row: &Row<'_>) -> rusqlite::Result<DetectedWork> {
    Ok(DetectedWork {
        id: row.get("id")?,
        project_id: row.get("project_id")?,
        task_id: row.get("task_id")?,
        kind: parse_enum(&row.get::<_, String>("kind")?)?,
        title: row.get("title")?,
        detail: row.get("detail")?,
        evidence_ids: json_list(row.get("evidence_ids")?)?,
        confidence: row.get("confidence")?,
        status: parse_enum(&row.get::<_, String>("status")?)?,
        suggested_task_title: row.get("suggested_task_title")?,
        dedupe_key: row.get("dedupe_key")?,
        first_detected_at: dt(row.get("first_detected_at")?)?,
        last_seen_at: dt(row.get("last_seen_at")?)?,
        resolved_at: dt_opt(row.get("resolved_at")?)?,
        created_at: dt(row.get("created_at")?)?,
        project_name: row.get("project_name").ok(),
    })
}

const SELECT: &str = "SELECT d.*, p.name AS project_name FROM detected_work d
    LEFT JOIN projects p ON p.id = d.project_id";

fn get_in(conn: &Connection, id: &str) -> Result<DetectedWork> {
    let sql = format!("{SELECT} WHERE d.id = ?1");
    conn.query_row(&sql, [id], from_row).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => OpsError::NotFound(format!("tespit: {id}")),
        other => other.into(),
    })
}

impl Store {
    /// Tespit kaydını dedupe_key üzerinden günceller/açar.
    ///
    /// Durum makinesi (deterministik):
    /// - kayıt yok            → OPEN olarak oluştur (+ audit DETECTED_WORK_CREATE)
    /// - OPEN                 → detail/last_seen tazele
    /// - RESOLVED             → sinyal geri geldi: yeniden OPEN (+ audit)
    /// - DISMISSED/CONVERTED  → kullanıcı kararına saygı: dokunma
    ///
    /// Yeni açıldı/yeniden açıldıysa true döner.
    pub fn upsert_detected(&self, input: NewDetected) -> Result<bool> {
        let now = time::now();
        let mut conn = self.db.conn();
        let tx = conn.transaction()?;
        let existing: Option<DetectedWork> = {
            let sql = format!("{SELECT} WHERE d.dedupe_key = ?1");
            let mut stmt = tx.prepare(&sql)?;
            let mut rows = stmt.query_map([&input.dedupe_key], from_row)?;
            rows.next().transpose()?
        };

        let opened = match existing {
            None => {
                let id = uuid::Uuid::new_v4().to_string();
                tx.execute(
                    "INSERT INTO detected_work(id, project_id, task_id, kind, title, detail,
                        evidence_ids, confidence, status, suggested_task_title, dedupe_key,
                        first_detected_at, last_seen_at, resolved_at, created_at)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'OPEN',?9,?10,?11,?11,NULL,?11)",
                    params![
                        id,
                        input.project_id,
                        input.task_id,
                        input.kind.as_ref(),
                        input.title,
                        input.detail,
                        serde_json::to_string(&input.evidence_ids)?,
                        input.confidence,
                        input.suggested_task_title,
                        input.dedupe_key,
                        time::to_db(&now),
                    ],
                )?;
                audit::append_tx(
                    &tx,
                    &NewAudit {
                        actor: Ctx::DAEMON.actor,
                        origin: Ctx::DAEMON.origin,
                        action: "DETECTED_WORK_CREATE".into(),
                        target: Some(format!("detected:{id}")),
                        risk_level: RiskLevel::R0,
                        capability: None,
                        result: AuditResult::Ok,
                        metadata: serde_json::json!({ "kind": input.kind.as_ref() }),
                    },
                )?;
                true
            }
            Some(d) => match d.status {
                DetectedStatus::Open => {
                    tx.execute(
                        "UPDATE detected_work SET title=?2, detail=?3, evidence_ids=?4,
                            confidence=?5, last_seen_at=?6 WHERE id=?1",
                        params![
                            d.id,
                            input.title,
                            input.detail,
                            serde_json::to_string(&input.evidence_ids)?,
                            input.confidence,
                            time::to_db(&now),
                        ],
                    )?;
                    false
                }
                DetectedStatus::Resolved => {
                    tx.execute(
                        "UPDATE detected_work SET title=?2, detail=?3, evidence_ids=?4,
                            confidence=?5, status='OPEN', resolved_at=NULL,
                            first_detected_at=?6, last_seen_at=?6 WHERE id=?1",
                        params![
                            d.id,
                            input.title,
                            input.detail,
                            serde_json::to_string(&input.evidence_ids)?,
                            input.confidence,
                            time::to_db(&now),
                        ],
                    )?;
                    audit::append_tx(
                        &tx,
                        &NewAudit {
                            actor: Ctx::DAEMON.actor,
                            origin: Ctx::DAEMON.origin,
                            action: "DETECTED_WORK_REOPEN".into(),
                            target: Some(format!("detected:{}", d.id)),
                            risk_level: RiskLevel::R0,
                            capability: None,
                            result: AuditResult::Ok,
                            metadata: serde_json::json!({ "kind": input.kind.as_ref() }),
                        },
                    )?;
                    true
                }
                // Kullanıcı yoksaydı ya da göreve çevirdi — sistem üstüne yazmaz.
                DetectedStatus::Dismissed | DetectedStatus::Converted => false,
            },
        };
        tx.commit()?;
        Ok(opened)
    }

    pub fn list_detected(&self, include_closed: bool) -> Result<Vec<DetectedWork>> {
        let where_clause = if include_closed { "1=1" } else { "d.status = 'OPEN'" };
        let sql = format!("{SELECT} WHERE {where_clause} ORDER BY d.last_seen_at DESC LIMIT 200");
        let conn = self.db.conn();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], from_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn list_detected_for_project(&self, project_id: &str) -> Result<Vec<DetectedWork>> {
        let sql = format!(
            "{SELECT} WHERE d.project_id = ?1 AND d.status = 'OPEN'
             ORDER BY d.last_seen_at DESC LIMIT 50"
        );
        let conn = self.db.conn();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([project_id], from_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn dismiss_detected(&self, ctx: &Ctx, id: &str) -> Result<DetectedWork> {
        let mut conn = self.db.conn();
        let tx = conn.transaction()?;
        let d = get_in(&tx, id)?;
        if d.status == DetectedStatus::Open {
            tx.execute(
                "UPDATE detected_work SET status='DISMISSED', last_seen_at=?2 WHERE id=?1",
                params![id, time::to_db(&time::now())],
            )?;
            audit::append_tx(
                &tx,
                &NewAudit {
                    actor: ctx.actor,
                    origin: ctx.origin,
                    action: "DETECTED_WORK_DISMISS".into(),
                    target: Some(format!("detected:{id}")),
                    risk_level: RiskLevel::R0,
                    capability: None,
                    result: AuditResult::Ok,
                    metadata: serde_json::json!({}),
                },
            )?;
        }
        let updated = get_in(&tx, id)?;
        tx.commit()?;
        Ok(updated)
    }

    /// Tespiti kullanıcı onayıyla göreve çevirir.
    pub fn convert_detected(&self, ctx: &Ctx, id: &str) -> Result<Task> {
        let d = {
            let conn = self.db.conn();
            get_in(&conn, id)?
        };
        if d.status != DetectedStatus::Open {
            return Err(OpsError::Conflict("tespit zaten kapatılmış".into()));
        }
        let title = d.suggested_task_title.clone().unwrap_or_else(|| d.title.clone());
        let task = self.create_task(
            ctx,
            TaskCreate {
                title,
                description: Some(d.detail.clone()),
                project_id: d.project_id.clone(),
                source: Some(TaskSource::AiDetected),
                status: Some(crate::models::TaskStatus::Next),
                ..Default::default()
            },
        )?;
        let mut conn = self.db.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE detected_work SET status='CONVERTED', task_id=?2, last_seen_at=?3 WHERE id=?1",
            params![id, task.id, time::to_db(&time::now())],
        )?;
        audit::append_tx(
            &tx,
            &NewAudit {
                actor: ctx.actor,
                origin: ctx.origin,
                action: "DETECTED_WORK_CONVERT".into(),
                target: Some(format!("detected:{id}")),
                risk_level: RiskLevel::R0,
                capability: Some("CREATE_TASK".into()),
                result: AuditResult::Ok,
                metadata: serde_json::json!({ "taskId": task.id }),
            },
        )?;
        tx.commit()?;
        Ok(task)
    }

    /// Sinyali kaybolan OPEN tespitleri RESOLVED yapar.
    /// `kind` verilirse yalnızca o tür; `project_id` verilirse yalnızca o proje.
    /// `active_keys` = bu scan'de hâlâ geçerli dedupe_key kümesi.
    pub fn resolve_missing_detected(
        &self,
        kind: DetectedKind,
        project_id: Option<&str>,
        active_keys: &HashSet<String>,
    ) -> Result<i64> {
        let open: Vec<DetectedWork> = {
            let sql = format!(
                "{SELECT} WHERE d.status = 'OPEN' AND d.kind = ?1
                   AND (?2 IS NULL OR d.project_id = ?2)"
            );
            let conn = self.db.conn();
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(params![kind.as_ref(), project_id], from_row)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let mut resolved = 0i64;
        for d in open {
            if !active_keys.contains(&d.dedupe_key) {
                let now = time::to_db(&time::now());
                let conn = self.db.conn();
                conn.execute(
                    "UPDATE detected_work SET status='RESOLVED', resolved_at=?2, last_seen_at=?2
                     WHERE id=?1",
                    params![d.id, now],
                )?;
                resolved += 1;
            }
        }
        Ok(resolved)
    }
}
