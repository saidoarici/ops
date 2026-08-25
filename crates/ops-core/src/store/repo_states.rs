use rusqlite::{params, Row};

use crate::models::RepoState;
use crate::store::{dt, dt_opt, Store};
use crate::{time, Result};

fn from_row(row: &Row<'_>) -> rusqlite::Result<RepoState> {
    Ok(RepoState {
        project_id: row.get("project_id")?,
        repo_path: row.get("repo_path")?,
        branch: row.get("branch")?,
        head_commit: row.get("head_commit")?,
        dirty_files: row.get("dirty_files")?,
        dirty_since: dt_opt(row.get("dirty_since")?)?,
        ahead: row.get("ahead")?,
        last_commit_at: dt_opt(row.get("last_commit_at")?)?,
        last_scan_at: dt(row.get("last_scan_at")?)?,
    })
}

impl Store {
    pub fn get_repo_state(&self, project_id: &str, repo_path: &str) -> Result<Option<RepoState>> {
        let conn = self.db.conn();
        let mut stmt =
            conn.prepare("SELECT * FROM repo_states WHERE project_id = ?1 AND repo_path = ?2")?;
        let mut rows = stmt.query_map(params![project_id, repo_path], from_row)?;
        Ok(rows.next().transpose()?)
    }

    pub fn list_repo_states(&self, project_id: &str) -> Result<Vec<RepoState>> {
        let conn = self.db.conn();
        let mut stmt =
            conn.prepare("SELECT * FROM repo_states WHERE project_id = ?1 ORDER BY repo_path")?;
        let rows = stmt.query_map([project_id], from_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn upsert_repo_state(&self, state: &RepoState) -> Result<()> {
        let conn = self.db.conn();
        conn.execute(
            "INSERT INTO repo_states(project_id, repo_path, branch, head_commit, dirty_files,
                dirty_since, ahead, last_commit_at, last_scan_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)
             ON CONFLICT(project_id, repo_path) DO UPDATE SET
                branch = excluded.branch,
                head_commit = excluded.head_commit,
                dirty_files = excluded.dirty_files,
                dirty_since = excluded.dirty_since,
                ahead = excluded.ahead,
                last_commit_at = excluded.last_commit_at,
                last_scan_at = excluded.last_scan_at",
            params![
                state.project_id,
                state.repo_path,
                state.branch,
                state.head_commit,
                state.dirty_files,
                time::opt_to_db(&state.dirty_since),
                state.ahead,
                time::opt_to_db(&state.last_commit_at),
                time::to_db(&state.last_scan_at),
            ],
        )?;
        Ok(())
    }
}
