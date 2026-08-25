use rusqlite::{params, Connection, Row};

use crate::models::{
    AgentMessage, AgentMessageRole, AgentMode, AgentProviderKind, AgentSession, AgentSessionStatus,
    AuditResult, Ctx, EvidenceType, NewAudit, NewEvidence,
};
use crate::store::{audit, dt, dt_opt, json_list, parse_enum, Store};
use crate::{time, OpsError, Result};

fn from_row(row: &Row<'_>) -> rusqlite::Result<AgentSession> {
    Ok(AgentSession {
        id: row.get("id")?,
        provider: parse_enum(&row.get::<_, String>("provider")?)?,
        project_id: row.get("project_id")?,
        started_at: dt(row.get("started_at")?)?,
        ended_at: dt_opt(row.get("ended_at")?)?,
        mode: parse_enum(&row.get::<_, String>("mode")?)?,
        working_directory: row.get("working_directory")?,
        status: parse_enum(&row.get::<_, String>("status")?)?,
        summary: row.get("summary")?,
        evidence_ids: json_list(row.get("evidence_ids")?)?,
        created_at: dt(row.get("created_at")?)?,
        provider_session_id: row.get("provider_session_id")?,
        last_activity_at: dt_opt(row.get("last_activity_at")?)?,
        title: row.get("title")?,
        project_name: row.get("project_name").ok(),
    })
}

const SELECT: &str = "SELECT s.*, p.name AS project_name FROM agent_sessions s
    LEFT JOIN projects p ON p.id = s.project_id";

fn get_in(conn: &Connection, id: &str) -> Result<AgentSession> {
    let sql = format!("{SELECT} WHERE s.id = ?1");
    conn.query_row(&sql, [id], from_row).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => OpsError::NotFound(format!("oturum: {id}")),
        other => other.into(),
    })
}

impl Store {
    pub fn create_agent_session(
        &self,
        ctx: &Ctx,
        provider: AgentProviderKind,
        project_id: Option<String>,
        mode: AgentMode,
        working_directory: Option<String>,
        title: &str,
    ) -> Result<AgentSession> {
        let now = time::now();
        let session = AgentSession {
            id: uuid::Uuid::new_v4().to_string(),
            provider,
            project_id: project_id.clone(),
            started_at: now,
            ended_at: None,
            mode,
            working_directory,
            status: AgentSessionStatus::Running,
            summary: None,
            evidence_ids: Vec::new(),
            created_at: now,
            provider_session_id: None,
            last_activity_at: Some(now),
            title: Some(title.chars().take(120).collect()),
            project_name: None,
        };
        let (capability, risk) = mode.capability();
        let mut conn = self.db.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO agent_sessions(id, provider, project_id, started_at, ended_at, mode,
                working_directory, status, summary, evidence_ids, created_at,
                provider_session_id, last_activity_at, title)
             VALUES (?1,?2,?3,?4,NULL,?5,?6,?7,NULL,'[]',?4,NULL,?4,?8)",
            params![
                session.id,
                session.provider.as_ref(),
                session.project_id,
                time::to_db(&now),
                session.mode.as_ref(),
                session.working_directory,
                session.status.as_ref(),
                session.title,
            ],
        )?;
        audit::append_tx(
            &tx,
            &NewAudit {
                actor: ctx.actor,
                origin: ctx.origin,
                action: "AGENT_SESSION_START".into(),
                target: Some(format!("session:{}", session.id)),
                risk_level: risk,
                capability: capability.map(str::to_string),
                result: AuditResult::Ok,
                metadata: serde_json::json!({
                    "provider": session.provider.as_ref(),
                    "mode": session.mode.as_ref(),
                }),
            },
        )?;
        tx.commit()?;
        // Db mutex'i reentrant değildir: ikinci bağlantı almadan önce ilkini bırak.
        drop(conn);
        let conn = self.db.conn();
        get_in(&conn, &session.id)
    }

    pub fn get_agent_session(&self, id: &str) -> Result<AgentSession> {
        let conn = self.db.conn();
        get_in(&conn, id)
    }

    pub fn list_agent_sessions(&self, limit: i64) -> Result<Vec<AgentSession>> {
        let sql =
            format!("{SELECT} ORDER BY s.last_activity_at DESC LIMIT {}", limit.clamp(1, 100));
        let conn = self.db.conn();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], from_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn set_agent_provider_session(&self, id: &str, provider_session_id: &str) -> Result<()> {
        let conn = self.db.conn();
        conn.execute(
            "UPDATE agent_sessions SET provider_session_id = ?2 WHERE id = ?1",
            params![id, provider_session_id],
        )?;
        Ok(())
    }

    pub fn mark_agent_session_running(&self, id: &str) -> Result<()> {
        let conn = self.db.conn();
        conn.execute(
            "UPDATE agent_sessions SET status='RUNNING', ended_at=NULL, last_activity_at=?2
             WHERE id=?1",
            params![id, time::to_db(&time::now())],
        )?;
        Ok(())
    }

    /// Oturumu kapatır; özetten AI_SESSION evidence üretir ve audit'ler.
    pub fn finish_agent_session(
        &self,
        id: &str,
        status: AgentSessionStatus,
        summary: Option<&str>,
    ) -> Result<AgentSession> {
        let now = time::now();
        let session = self.get_agent_session(id)?;
        let mut evidence_ids = session.evidence_ids.clone();
        if let (Some(pid), Some(sum)) = (&session.project_id, summary) {
            if status == AgentSessionStatus::Completed && !sum.trim().is_empty() {
                let short: String = sum.trim().chars().take(300).collect();
                if let Some(ev) = self.add_evidence(NewEvidence {
                    task_id: None,
                    project_id: Some(pid.clone()),
                    kind: EvidenceType::AiSession,
                    source: format!("agent:{}", session.provider.as_ref().to_lowercase()),
                    timestamp: now,
                    summary: format!("AI oturumu: {short}"),
                    confidence: None,
                    source_reference: Some(format!("session:{id}")),
                    content_hash: Some(format!("session-end:{id}:{}", time::to_db(&now))),
                })? {
                    evidence_ids.push(ev.id);
                }
                self.touch_project_activity(pid, now)?;
            }
        }
        let mut conn = self.db.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE agent_sessions SET status=?2, ended_at=?3, summary=?4, evidence_ids=?5,
                last_activity_at=?3 WHERE id=?1",
            params![
                id,
                status.as_ref(),
                time::to_db(&now),
                summary.map(|s| s.chars().take(2000).collect::<String>()),
                serde_json::to_string(&evidence_ids)?,
            ],
        )?;
        audit::append_tx(
            &tx,
            &NewAudit {
                actor: Ctx::DAEMON.actor,
                origin: Ctx::DAEMON.origin,
                action: "AGENT_SESSION_END".into(),
                target: Some(format!("session:{id}")),
                risk_level: crate::models::RiskLevel::R0,
                capability: None,
                result: if status == AgentSessionStatus::Failed {
                    AuditResult::Error
                } else {
                    AuditResult::Ok
                },
                metadata: serde_json::json!({ "status": status.as_ref() }),
            },
        )?;
        tx.commit()?;
        drop(conn);
        self.get_agent_session(id)
    }

    pub fn append_agent_message(
        &self,
        session_id: &str,
        role: AgentMessageRole,
        content: &str,
        payload: Option<&str>,
    ) -> Result<AgentMessage> {
        let now = time::now();
        let conn = self.db.conn();
        let seq: i64 = conn.query_row(
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM agent_messages WHERE session_id = ?1",
            [session_id],
            |r| r.get(0),
        )?;
        let msg = AgentMessage {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            seq,
            role,
            content: content.chars().take(60_000).collect(),
            created_at: now,
        };
        conn.execute(
            "INSERT INTO agent_messages(id, session_id, seq, role, content, payload, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![
                msg.id,
                msg.session_id,
                msg.seq,
                msg.role.as_ref(),
                msg.content,
                payload.map(|p| p.chars().take(60_000).collect::<String>()),
                time::to_db(&now),
            ],
        )?;
        conn.execute(
            "UPDATE agent_sessions SET last_activity_at = ?2 WHERE id = ?1",
            params![session_id, time::to_db(&now)],
        )?;
        Ok(msg)
    }

    pub fn list_agent_messages(
        &self,
        session_id: &str,
        after_seq: Option<i64>,
    ) -> Result<Vec<AgentMessage>> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT id, session_id, seq, role, content, created_at FROM agent_messages
             WHERE session_id = ?1 AND seq > ?2 ORDER BY seq ASC LIMIT 500",
        )?;
        let rows = stmt.query_map(params![session_id, after_seq.unwrap_or(0)], |row| {
            Ok(AgentMessage {
                id: row.get("id")?,
                session_id: row.get("session_id")?,
                seq: row.get("seq")?,
                role: parse_enum(&row.get::<_, String>("role")?)?,
                content: row.get("content")?,
                created_at: dt(row.get("created_at")?)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}
