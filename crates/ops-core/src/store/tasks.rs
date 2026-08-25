use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, Row};

use crate::models::{
    AuditResult, Ctx, NewAudit, RiskLevel, Task, TaskCreate, TaskFilter, TaskPatch, TaskSource,
    TaskStatus,
};
use crate::store::{
    audit, check_scale, check_text, check_text_allow_empty, dt, dt_opt, json_list, parse_enum,
    parse_enum_opt, Store,
};
use crate::{time, OpsError, Result};

fn from_row(row: &Row<'_>) -> rusqlite::Result<Task> {
    Ok(Task {
        id: row.get("id")?,
        title: row.get("title")?,
        description: row.get("description")?,
        project_id: row.get("project_id")?,
        status: parse_enum(&row.get::<_, String>("status")?)?,
        priority: row.get("priority")?,
        importance: row.get("importance")?,
        urgency: row.get("urgency")?,
        due_at: dt_opt(row.get("due_at")?)?,
        scheduled_at: dt_opt(row.get("scheduled_at")?)?,
        created_at: dt(row.get("created_at")?)?,
        updated_at: dt(row.get("updated_at")?)?,
        completed_at: dt_opt(row.get("completed_at")?)?,
        parent_task_id: row.get("parent_task_id")?,
        tags: json_list(row.get("tags")?)?,
        source: parse_enum(&row.get::<_, String>("source")?)?,
        waiting_for: row.get("waiting_for")?,
        waiting_since: dt_opt(row.get("waiting_since")?)?,
        followup_at: dt_opt(row.get("followup_at")?)?,
        blocked_by: row.get("blocked_by")?,
        estimated_minutes: row.get("estimated_minutes")?,
        energy_level: parse_enum_opt(row.get("energy_level")?)?,
        archived: row.get("archived")?,
        project_name: row.get("project_name").ok(),
    })
}

const SELECT: &str =
    "SELECT t.*, p.name AS project_name FROM tasks t LEFT JOIN projects p ON p.id = t.project_id";

fn get_in(conn: &Connection, id: &str) -> Result<Task> {
    let sql = format!("{SELECT} WHERE t.id = ?1");
    conn.query_row(&sql, [id], from_row).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => OpsError::NotFound(format!("görev: {id}")),
        other => other.into(),
    })
}

/// Görev aktivitesi proje aktivitesidir: last_activity_at'i ileri alır.
fn touch_project(conn: &Connection, project_id: &Option<String>, now: DateTime<Utc>) -> Result<()> {
    if let Some(pid) = project_id {
        conn.execute(
            "UPDATE projects SET last_activity_at = ?2
             WHERE id = ?1 AND (last_activity_at IS NULL OR last_activity_at < ?2)",
            params![pid, time::to_db(&now)],
        )?;
    }
    Ok(())
}

fn project_exists(conn: &Connection, id: &str) -> Result<()> {
    let ok: Option<i64> = conn
        .query_row("SELECT 1 FROM projects WHERE id = ?1", [id], |r| r.get(0))
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })?;
    if ok.is_none() {
        return Err(OpsError::NotFound(format!("proje: {id}")));
    }
    Ok(())
}

/// Statü geçişlerinin deterministik yan etkileri (docs/data-model.md, tasks).
fn apply_status_transition(task: &mut Task, new_status: TaskStatus, now: DateTime<Utc>) {
    let old = task.status;
    if old == new_status {
        return;
    }
    task.status = new_status;
    match new_status {
        TaskStatus::Done => task.completed_at = Some(now),
        TaskStatus::Waiting if task.waiting_since.is_none() => task.waiting_since = Some(now),
        _ => {}
    }
    if old == TaskStatus::Done && new_status != TaskStatus::Done {
        task.completed_at = None;
    }
    if old == TaskStatus::Waiting && new_status != TaskStatus::Waiting {
        task.waiting_since = None;
    }
}

fn write_update(conn: &Connection, t: &Task) -> Result<()> {
    conn.execute(
        "UPDATE tasks SET
            title=?2, description=?3, project_id=?4, status=?5, priority=?6, importance=?7,
            urgency=?8, due_at=?9, scheduled_at=?10, updated_at=?11, completed_at=?12,
            parent_task_id=?13, tags=?14, source=?15, waiting_for=?16, waiting_since=?17,
            followup_at=?18, blocked_by=?19, estimated_minutes=?20, energy_level=?21,
            archived=?22
         WHERE id=?1",
        params![
            t.id,
            t.title,
            t.description,
            t.project_id,
            t.status.as_ref(),
            t.priority,
            t.importance,
            t.urgency,
            time::opt_to_db(&t.due_at),
            time::opt_to_db(&t.scheduled_at),
            time::to_db(&t.updated_at),
            time::opt_to_db(&t.completed_at),
            t.parent_task_id,
            serde_json::to_string(&t.tags)?,
            t.source.as_ref(),
            t.waiting_for,
            time::opt_to_db(&t.waiting_since),
            time::opt_to_db(&t.followup_at),
            t.blocked_by,
            t.estimated_minutes,
            t.energy_level.as_ref().map(|e| e.as_ref()),
            t.archived,
        ],
    )?;
    Ok(())
}

impl Store {
    pub fn create_task(&self, ctx: &Ctx, input: TaskCreate) -> Result<Task> {
        // Başlık ve açıklama her zaman veri olarak saklanır; hiçbir yerde
        // yorumlanmaz ya da çalıştırılmaz (docs/threat-model.md T8).
        let title = check_text("başlık", &input.title, 500)?;
        let description = match &input.description {
            Some(d) => check_text_allow_empty(d, 10_000)?,
            None => String::new(),
        };
        let now = time::now();
        let status = input.status.unwrap_or(TaskStatus::Inbox);

        let mut task = Task {
            id: uuid::Uuid::new_v4().to_string(),
            title,
            description,
            project_id: input.project_id.clone(),
            status: TaskStatus::Inbox,
            priority: check_scale("priority", input.priority.unwrap_or(3))?,
            importance: check_scale("importance", input.importance.unwrap_or(3))?,
            urgency: check_scale("urgency", input.urgency.unwrap_or(3))?,
            due_at: input.due_at,
            scheduled_at: input.scheduled_at,
            created_at: now,
            updated_at: now,
            completed_at: None,
            parent_task_id: input.parent_task_id.clone(),
            tags: input.tags.unwrap_or_default(),
            source: input.source.unwrap_or(TaskSource::LocalUi),
            waiting_for: input.waiting_for.clone(),
            waiting_since: None,
            followup_at: input.followup_at,
            blocked_by: input.blocked_by.clone(),
            estimated_minutes: input.estimated_minutes,
            energy_level: input.energy_level,
            archived: false,
            project_name: None,
        };
        apply_status_transition(&mut task, status, now);

        let mut conn = self.db.conn();
        let tx = conn.transaction()?;
        if let Some(pid) = &task.project_id {
            project_exists(&tx, pid)?;
        }
        tx.execute(
            "INSERT INTO tasks(id, title, description, project_id, status, priority, importance,
                urgency, due_at, scheduled_at, created_at, updated_at, completed_at,
                parent_task_id, tags, source, waiting_for, waiting_since, followup_at, blocked_by,
                estimated_minutes, energy_level, archived)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,
                     ?21,?22,?23)",
            params![
                task.id,
                task.title,
                task.description,
                task.project_id,
                task.status.as_ref(),
                task.priority,
                task.importance,
                task.urgency,
                time::opt_to_db(&task.due_at),
                time::opt_to_db(&task.scheduled_at),
                time::to_db(&task.created_at),
                time::to_db(&task.updated_at),
                time::opt_to_db(&task.completed_at),
                task.parent_task_id,
                serde_json::to_string(&task.tags)?,
                task.source.as_ref(),
                task.waiting_for,
                time::opt_to_db(&task.waiting_since),
                time::opt_to_db(&task.followup_at),
                task.blocked_by,
                task.estimated_minutes,
                task.energy_level.as_ref().map(|e| e.as_ref()),
                task.archived,
            ],
        )?;
        audit::append_tx(
            &tx,
            &NewAudit {
                actor: ctx.actor,
                origin: ctx.origin,
                action: "TASK_CREATE".into(),
                target: Some(format!("task:{}", task.id)),
                risk_level: RiskLevel::R0,
                capability: Some("CREATE_TASK".into()),
                result: AuditResult::Ok,
                metadata: serde_json::json!({
                    "source": task.source.as_ref(),
                    "status": task.status.as_ref(),
                }),
            },
        )?;
        touch_project(&tx, &task.project_id, now)?;
        let created = get_in(&tx, &task.id)?;
        tx.commit()?;
        Ok(created)
    }

    pub fn get_task(&self, id: &str) -> Result<Task> {
        let conn = self.db.conn();
        get_in(&conn, id)
    }

    pub fn list_tasks(&self, filter: &TaskFilter) -> Result<Vec<Task>> {
        let mut sql = format!("{SELECT} WHERE 1=1");
        let mut values: Vec<rusqlite::types::Value> = Vec::new();

        if !filter.include_archived {
            sql.push_str(" AND t.archived = 0");
        }
        if let Some(statuses) = &filter.statuses {
            if !statuses.is_empty() {
                let marks: Vec<String> = statuses
                    .iter()
                    .map(|s| {
                        values.push(rusqlite::types::Value::Text(s.as_ref().to_string()));
                        format!("?{}", values.len())
                    })
                    .collect();
                sql.push_str(&format!(" AND t.status IN ({})", marks.join(",")));
            }
        }
        if let Some(pid) = &filter.project_id {
            values.push(rusqlite::types::Value::Text(pid.clone()));
            sql.push_str(&format!(" AND t.project_id = ?{}", values.len()));
        }
        if let Some(search) = &filter.search {
            let term = search.trim();
            if !term.is_empty() {
                values.push(rusqlite::types::Value::Text(format!("%{term}%")));
                let idx = values.len();
                sql.push_str(&format!(" AND (t.title LIKE ?{idx} OR t.description LIKE ?{idx})"));
            }
        }
        sql.push_str(
            " ORDER BY CASE t.status
                WHEN 'IN_PROGRESS' THEN 0 WHEN 'NEXT' THEN 1 WHEN 'PLANNED' THEN 2
                WHEN 'INBOX' THEN 3 WHEN 'WAITING' THEN 4 WHEN 'BLOCKED' THEN 5
                WHEN 'SOMEDAY' THEN 6 WHEN 'DONE' THEN 7 ELSE 8 END,
              t.priority DESC, (t.due_at IS NULL), t.due_at ASC, t.updated_at DESC",
        );
        let limit = filter.limit.unwrap_or(500).clamp(1, 2000);
        sql.push_str(&format!(" LIMIT {limit}"));

        let conn = self.db.conn();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(values.iter()), from_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn update_task(&self, ctx: &Ctx, id: &str, patch: TaskPatch) -> Result<Task> {
        let now = time::now();
        let mut conn = self.db.conn();
        let tx = conn.transaction()?;
        let mut task = get_in(&tx, id)?;
        let mut changed: Vec<&'static str> = Vec::new();

        if let Some(v) = patch.title {
            task.title = check_text("başlık", &v, 500)?;
            changed.push("title");
        }
        if let Some(v) = patch.description {
            task.description = check_text_allow_empty(&v, 10_000)?;
            changed.push("description");
        }
        if let Some(v) = patch.priority {
            task.priority = check_scale("priority", v)?;
            changed.push("priority");
        }
        if let Some(v) = patch.importance {
            task.importance = check_scale("importance", v)?;
            changed.push("importance");
        }
        if let Some(v) = patch.urgency {
            task.urgency = check_scale("urgency", v)?;
            changed.push("urgency");
        }
        if let Some(v) = patch.tags {
            task.tags = v;
            changed.push("tags");
        }
        if let Some(v) = patch.project_id {
            if let Some(pid) = &v {
                project_exists(&tx, pid)?;
            }
            task.project_id = v;
            changed.push("projectId");
        }
        if let Some(v) = patch.due_at {
            task.due_at = v;
            changed.push("dueAt");
        }
        if let Some(v) = patch.scheduled_at {
            task.scheduled_at = v;
            changed.push("scheduledAt");
        }
        if let Some(v) = patch.parent_task_id {
            task.parent_task_id = v;
            changed.push("parentTaskId");
        }
        if let Some(v) = patch.waiting_for {
            task.waiting_for = v;
            changed.push("waitingFor");
        }
        if let Some(v) = patch.followup_at {
            task.followup_at = v;
            changed.push("followupAt");
        }
        if let Some(v) = patch.blocked_by {
            task.blocked_by = v;
            changed.push("blockedBy");
        }
        if let Some(v) = patch.estimated_minutes {
            task.estimated_minutes = v;
            changed.push("estimatedMinutes");
        }
        if let Some(v) = patch.energy_level {
            task.energy_level = v;
            changed.push("energyLevel");
        }
        if let Some(v) = patch.status {
            apply_status_transition(&mut task, v, now);
            changed.push("status");
        }

        if changed.is_empty() {
            tx.commit()?;
            return Ok(task);
        }
        task.updated_at = now;
        write_update(&tx, &task)?;
        audit::append_tx(
            &tx,
            &NewAudit {
                actor: ctx.actor,
                origin: ctx.origin,
                action: "TASK_UPDATE".into(),
                target: Some(format!("task:{id}")),
                risk_level: RiskLevel::R0,
                capability: Some("UPDATE_TASK".into()),
                result: AuditResult::Ok,
                // Yalnızca alan adları; içerik dökümü audit'e yazılmaz.
                metadata: serde_json::json!({ "changed": changed }),
            },
        )?;
        touch_project(&tx, &task.project_id, now)?;
        let updated = get_in(&tx, id)?;
        tx.commit()?;
        Ok(updated)
    }

    pub fn complete_task(&self, ctx: &Ctx, id: &str) -> Result<Task> {
        self.update_task(
            ctx,
            id,
            TaskPatch { status: Some(TaskStatus::Done), ..Default::default() },
        )
    }

    /// Soft delete: kayıt silinmez, arşivlenir (audit bütünlüğü + geri alma).
    pub fn archive_task(&self, ctx: &Ctx, id: &str) -> Result<Task> {
        let mut conn = self.db.conn();
        let tx = conn.transaction()?;
        let mut task = get_in(&tx, id)?;
        if !task.archived {
            task.archived = true;
            task.updated_at = time::now();
            write_update(&tx, &task)?;
            audit::append_tx(
                &tx,
                &NewAudit {
                    actor: ctx.actor,
                    origin: ctx.origin,
                    action: "TASK_ARCHIVE".into(),
                    target: Some(format!("task:{id}")),
                    risk_level: RiskLevel::R0,
                    capability: Some("UPDATE_TASK".into()),
                    result: AuditResult::Ok,
                    metadata: serde_json::json!({}),
                },
            )?;
        }
        tx.commit()?;
        Ok(task)
    }
}
