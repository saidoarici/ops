use rusqlite::{params, Row};

use crate::models::{Evidence, EvidenceFilter, NewEvidence};
use crate::store::{dt, parse_enum, Store};
use crate::{time, Result};

fn from_row(row: &Row<'_>) -> rusqlite::Result<Evidence> {
    Ok(Evidence {
        id: row.get("id")?,
        task_id: row.get("task_id")?,
        project_id: row.get("project_id")?,
        kind: parse_enum(&row.get::<_, String>("type")?)?,
        source: row.get("source")?,
        timestamp: dt(row.get("timestamp")?)?,
        summary: row.get("summary")?,
        confidence: row.get("confidence")?,
        source_reference: row.get("source_reference")?,
        content_hash: row.get("content_hash")?,
        created_at: dt(row.get("created_at")?)?,
        project_name: row.get("project_name").ok(),
    })
}

impl Store {
    /// Evidence ekler. `content_hash` doluysa ve aynı hash zaten varsa kayıt
    /// sessizce atlanır ve `None` döner (idempotent gözlem).
    pub fn add_evidence(&self, input: NewEvidence) -> Result<Option<Evidence>> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = time::now();
        let conn = self.db.conn();
        let inserted = conn.execute(
            "INSERT OR IGNORE INTO evidence(id, task_id, project_id, type, source, timestamp,
                summary, confidence, source_reference, content_hash, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![
                id,
                input.task_id,
                input.project_id,
                input.kind.as_ref(),
                input.source,
                time::to_db(&input.timestamp),
                input.summary,
                input.confidence,
                input.source_reference,
                input.content_hash,
                time::to_db(&now),
            ],
        )?;
        if inserted == 0 {
            return Ok(None);
        }
        Ok(Some(Evidence {
            id,
            task_id: input.task_id,
            project_id: input.project_id,
            kind: input.kind,
            source: input.source,
            timestamp: input.timestamp,
            summary: input.summary,
            confidence: input.confidence,
            source_reference: input.source_reference,
            content_hash: input.content_hash,
            created_at: now,
            project_name: None,
        }))
    }

    pub fn list_evidence(&self, filter: &EvidenceFilter) -> Result<Vec<Evidence>> {
        let mut sql = String::from(
            "SELECT e.*, p.name AS project_name FROM evidence e
             LEFT JOIN projects p ON p.id = e.project_id WHERE 1=1",
        );
        let mut values: Vec<rusqlite::types::Value> = Vec::new();
        if let Some(pid) = &filter.project_id {
            values.push(rusqlite::types::Value::Text(pid.clone()));
            sql.push_str(&format!(" AND e.project_id = ?{}", values.len()));
        }
        if let Some(tid) = &filter.task_id {
            values.push(rusqlite::types::Value::Text(tid.clone()));
            sql.push_str(&format!(" AND e.task_id = ?{}", values.len()));
        }
        sql.push_str(" ORDER BY e.timestamp DESC");
        let limit = filter.limit.unwrap_or(100).clamp(1, 500);
        sql.push_str(&format!(" LIMIT {limit}"));

        let conn = self.db.conn();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(values.iter()), from_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}
