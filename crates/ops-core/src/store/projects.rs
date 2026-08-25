use rusqlite::{params, Connection, Row};

use crate::models::{
    AuditResult, Ctx, NewAudit, Project, ProjectCreate, ProjectPatch, ProjectState,
    ProjectWithStats, RiskLevel,
};
use crate::store::{audit, check_scale, check_text, dt, dt_opt, json_list, parse_enum, Store};
use crate::{time, OpsError, Result};

fn from_row(row: &Row<'_>) -> rusqlite::Result<Project> {
    Ok(Project {
        id: row.get("id")?,
        name: row.get("name")?,
        description: row.get("description")?,
        state: parse_enum(&row.get::<_, String>("state")?)?,
        health: parse_enum(&row.get::<_, String>("health")?)?,
        priority: row.get("priority")?,
        local_paths: json_list(row.get("local_paths")?)?,
        git_repositories: json_list(row.get("git_repositories")?)?,
        keywords: json_list(row.get("keywords")?)?,
        related_contacts: json_list(row.get("related_contacts")?)?,
        last_activity_at: dt_opt(row.get("last_activity_at")?)?,
        stale_threshold_days: row.get("stale_threshold_days")?,
        created_at: dt(row.get("created_at")?)?,
        updated_at: dt(row.get("updated_at")?)?,
    })
}

fn get_in(conn: &Connection, id: &str) -> Result<Project> {
    conn.query_row("SELECT * FROM projects WHERE id = ?1", [id], from_row).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => OpsError::NotFound(format!("proje: {id}")),
        other => other.into(),
    })
}

fn write_update(conn: &Connection, p: &Project) -> Result<()> {
    conn.execute(
        "UPDATE projects SET
            name=?2, description=?3, state=?4, health=?5, priority=?6, local_paths=?7,
            git_repositories=?8, keywords=?9, related_contacts=?10, last_activity_at=?11,
            stale_threshold_days=?12, updated_at=?13
         WHERE id=?1",
        params![
            p.id,
            p.name,
            p.description,
            p.state.as_ref(),
            p.health.as_ref(),
            p.priority,
            serde_json::to_string(&p.local_paths)?,
            serde_json::to_string(&p.git_repositories)?,
            serde_json::to_string(&p.keywords)?,
            serde_json::to_string(&p.related_contacts)?,
            time::opt_to_db(&p.last_activity_at),
            p.stale_threshold_days,
            time::to_db(&p.updated_at),
        ],
    )?;
    Ok(())
}

impl Store {
    pub fn create_project(&self, ctx: &Ctx, input: ProjectCreate) -> Result<Project> {
        let name = check_text("proje adı", &input.name, 200)?;
        let now = time::now();
        let project = Project {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            description: input.description.unwrap_or_default(),
            state: ProjectState::Active,
            health: crate::models::ProjectHealth::Active,
            priority: check_scale("priority", input.priority.unwrap_or(3))?,
            local_paths: input.local_paths.unwrap_or_default(),
            git_repositories: input.git_repositories.unwrap_or_default(),
            keywords: input.keywords.unwrap_or_default(),
            related_contacts: input.related_contacts.unwrap_or_default(),
            last_activity_at: None,
            stale_threshold_days: input.stale_threshold_days.unwrap_or(4).clamp(1, 90),
            created_at: now,
            updated_at: now,
        };

        let mut conn = self.db.conn();
        let tx = conn.transaction()?;
        // Aynı isimde aktif proje varsa reddet (kullanıcı karışıklığını önler).
        let dup: i64 = tx.query_row(
            "SELECT COUNT(*) FROM projects WHERE lower(name) = lower(?1) AND state != 'ARCHIVED'",
            [&project.name],
            |r| r.get(0),
        )?;
        if dup > 0 {
            return Err(OpsError::Conflict(format!(
                "'{}' adında bir proje zaten var",
                project.name
            )));
        }
        tx.execute(
            "INSERT INTO projects(id, name, description, state, health, priority, local_paths,
                git_repositories, keywords, related_contacts, last_activity_at,
                stale_threshold_days, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
            params![
                project.id,
                project.name,
                project.description,
                project.state.as_ref(),
                project.health.as_ref(),
                project.priority,
                serde_json::to_string(&project.local_paths)?,
                serde_json::to_string(&project.git_repositories)?,
                serde_json::to_string(&project.keywords)?,
                serde_json::to_string(&project.related_contacts)?,
                time::opt_to_db(&project.last_activity_at),
                project.stale_threshold_days,
                time::to_db(&project.created_at),
                time::to_db(&project.updated_at),
            ],
        )?;
        audit::append_tx(
            &tx,
            &NewAudit {
                actor: ctx.actor,
                origin: ctx.origin,
                action: "PROJECT_CREATE".into(),
                target: Some(format!("project:{}", project.id)),
                risk_level: RiskLevel::R0,
                capability: None,
                result: AuditResult::Ok,
                metadata: serde_json::json!({}),
            },
        )?;
        tx.commit()?;
        Ok(project)
    }

    pub fn get_project(&self, id: &str) -> Result<Project> {
        let conn = self.db.conn();
        get_in(&conn, id)
    }

    pub fn list_projects(&self, include_archived: bool) -> Result<Vec<ProjectWithStats>> {
        let where_clause = if include_archived { "1=1" } else { "p.state != 'ARCHIVED'" };
        let sql = format!(
            "SELECT p.*,
                (SELECT COUNT(*) FROM tasks t WHERE t.project_id = p.id AND t.archived = 0
                    AND t.status NOT IN ('DONE','CANCELLED')) AS open_tasks,
                (SELECT COUNT(*) FROM tasks t WHERE t.project_id = p.id AND t.archived = 0
                    AND t.status = 'WAITING') AS waiting_tasks,
                (SELECT COUNT(*) FROM tasks t WHERE t.project_id = p.id AND t.archived = 0
                    AND t.status = 'INBOX') AS inbox_tasks,
                (SELECT MAX(t.updated_at) FROM tasks t WHERE t.project_id = p.id)
                    AS last_task_activity
             FROM projects p WHERE {where_clause}
             ORDER BY p.priority DESC, lower(p.name) ASC"
        );
        let conn = self.db.conn();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], |row| {
            Ok(ProjectWithStats {
                project: from_row(row)?,
                open_tasks: row.get("open_tasks")?,
                waiting_tasks: row.get("waiting_tasks")?,
                inbox_tasks: row.get("inbox_tasks")?,
                last_task_activity: dt_opt(row.get("last_task_activity")?)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn update_project(&self, ctx: &Ctx, id: &str, patch: ProjectPatch) -> Result<Project> {
        let now = time::now();
        let mut conn = self.db.conn();
        let tx = conn.transaction()?;
        let mut p = get_in(&tx, id)?;
        let mut changed: Vec<&'static str> = Vec::new();

        if let Some(v) = patch.name {
            p.name = check_text("proje adı", &v, 200)?;
            changed.push("name");
        }
        if let Some(v) = patch.description {
            p.description = v;
            changed.push("description");
        }
        if let Some(v) = patch.state {
            p.state = v;
            changed.push("state");
        }
        if let Some(v) = patch.health {
            p.health = v;
            changed.push("health");
        }
        if let Some(v) = patch.priority {
            p.priority = check_scale("priority", v)?;
            changed.push("priority");
        }
        if let Some(v) = patch.local_paths {
            p.local_paths = v;
            changed.push("localPaths");
        }
        if let Some(v) = patch.git_repositories {
            p.git_repositories = v;
            changed.push("gitRepositories");
        }
        if let Some(v) = patch.keywords {
            p.keywords = v;
            changed.push("keywords");
        }
        if let Some(v) = patch.related_contacts {
            p.related_contacts = v;
            changed.push("relatedContacts");
        }
        if let Some(v) = patch.stale_threshold_days {
            p.stale_threshold_days = v.clamp(1, 90);
            changed.push("staleThresholdDays");
        }
        if let Some(v) = patch.last_activity_at {
            p.last_activity_at = v;
            changed.push("lastActivityAt");
        }

        if changed.is_empty() {
            tx.commit()?;
            return Ok(p);
        }
        p.updated_at = now;
        write_update(&tx, &p)?;
        audit::append_tx(
            &tx,
            &NewAudit {
                actor: ctx.actor,
                origin: ctx.origin,
                action: "PROJECT_UPDATE".into(),
                target: Some(format!("project:{id}")),
                risk_level: RiskLevel::R0,
                capability: None,
                result: AuditResult::Ok,
                metadata: serde_json::json!({ "changed": changed }),
            },
        )?;
        tx.commit()?;
        Ok(p)
    }

    pub fn archive_project(&self, ctx: &Ctx, id: &str) -> Result<Project> {
        self.update_project(
            ctx,
            id,
            ProjectPatch { state: Some(ProjectState::Archived), ..Default::default() },
        )
    }

    /// Observer: proje aktivite damgasını ileri alır (yalnızca daha yeniyse).
    pub fn touch_project_activity(
        &self,
        id: &str,
        at: chrono::DateTime<chrono::Utc>,
    ) -> Result<()> {
        let conn = self.db.conn();
        conn.execute(
            "UPDATE projects SET last_activity_at = ?2
             WHERE id = ?1 AND (last_activity_at IS NULL OR last_activity_at < ?2)",
            rusqlite::params![id, time::to_db(&at)],
        )?;
        Ok(())
    }

    /// Observer: deterministik health hesabını yazar; değişmediyse sessizdir
    /// (audit'i şişirmemek için yalnızca gerçek değişim audit'lenir).
    pub fn set_project_health(
        &self,
        id: &str,
        health: crate::models::ProjectHealth,
    ) -> Result<bool> {
        let mut conn = self.db.conn();
        let tx = conn.transaction()?;
        let current: String =
            tx.query_row("SELECT health FROM projects WHERE id = ?1", [id], |r| r.get(0))?;
        if current == health.as_ref() {
            tx.commit()?;
            return Ok(false);
        }
        tx.execute(
            "UPDATE projects SET health = ?2 WHERE id = ?1",
            rusqlite::params![id, health.as_ref()],
        )?;
        audit::append_tx(
            &tx,
            &NewAudit {
                actor: crate::models::Actor::Daemon,
                origin: crate::models::Origin::Daemon,
                action: "PROJECT_HEALTH_UPDATE".into(),
                target: Some(format!("project:{id}")),
                risk_level: RiskLevel::R0,
                capability: Some("READ_GIT_METADATA".into()),
                result: AuditResult::Ok,
                metadata: serde_json::json!({ "from": current, "to": health.as_ref() }),
            },
        )?;
        tx.commit()?;
        Ok(true)
    }
}
