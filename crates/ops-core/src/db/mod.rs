use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use rusqlite::Connection;

use crate::{time, Result};

/// Gömülü migration'lar; sıra = sürüm numarası (1'den başlar).
const MIGRATIONS: &[(&str, &str)] = &[
    ("0001_init", include_str!("migrations/0001_init.sql")),
    ("0002_observer", include_str!("migrations/0002_observer.sql")),
    ("0003_agent", include_str!("migrations/0003_agent.sql")),
    ("0004_schema_cleanup", include_str!("migrations/0004_schema_cleanup.sql")),
];

/// SQLite bağlantısı. Tek kullanıcılı lokal uygulama için kısa kritik
/// bölgelerle `Mutex<Connection>` yeterlidir; store katmanındaki tüm işlemler
/// milisaniye altıdır. Mutex reentrant değildir: aynı thread ikinci kez
/// `conn()` almadan önce ilk guard'ı bırakmalıdır.
#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut conn = Connection::open(path)?;
        configure(&conn)?;
        migrate(&mut conn)?;
        // DB dosyası yalnızca kullanıcıya açık.
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
        Ok(Self { conn: Arc::new(Mutex::new(conn)) })
    }

    pub fn open_in_memory() -> Result<Self> {
        let mut conn = Connection::open_in_memory()?;
        configure(&conn)?;
        migrate(&mut conn)?;
        Ok(Self { conn: Arc::new(Mutex::new(conn)) })
    }

    pub fn conn(&self) -> MutexGuard<'_, Connection> {
        // Zehirlenmiş lock'ta bile devam edilir: tutarlılık SQLite'ın transaction
        // sınırlarıyla korunur, guard'ı düşüren panic bağlantıyı bozmaz.
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }
}

fn configure(conn: &Connection) -> Result<()> {
    // `PRAGMA journal_mode` bir satır döndürür; execute yerine query_row gerekir.
    conn.query_row("PRAGMA journal_mode=WAL", [], |_r| Ok(()))?;
    conn.execute_batch("PRAGMA foreign_keys=ON; PRAGMA synchronous=NORMAL;")?;
    conn.busy_timeout(Duration::from_millis(5000))?;
    Ok(())
}

fn migrate(conn: &mut Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations(
            version    INTEGER PRIMARY KEY,
            name       TEXT NOT NULL,
            applied_at TEXT NOT NULL
        );",
    )?;
    let applied: i64 =
        conn.query_row("SELECT COALESCE(MAX(version), 0) FROM schema_migrations", [], |r| {
            r.get(0)
        })?;
    for (idx, (name, sql)) in MIGRATIONS.iter().enumerate() {
        let version = (idx + 1) as i64;
        if version > applied {
            let tx = conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.execute(
                "INSERT INTO schema_migrations(version, name, applied_at) VALUES (?1, ?2, ?3)",
                rusqlite::params![version, name, time::to_db(&time::now())],
            )?;
            tx.commit()?;
            tracing::info!(version, name, "migration uygulandı");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Eski şemayla (0001–0003) oluşturulmuş ve veri içeren bir DB, 0004 temizlik
    /// migration'ından veri kaybı olmadan geçer.
    #[test]
    fn schema_cleanup_migration_preserves_legacy_data() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("legacy.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY, name TEXT NOT NULL, applied_at TEXT NOT NULL);",
            )
            .unwrap();
            for (idx, (name, sql)) in MIGRATIONS.iter().take(3).enumerate() {
                conn.execute_batch(sql).unwrap();
                conn.execute(
                    "INSERT INTO schema_migrations(version, name, applied_at) VALUES (?1, ?2, 'x')",
                    rusqlite::params![idx as i64 + 1, name],
                )
                .unwrap();
            }
            conn.execute_batch(
                "INSERT INTO tasks(id, title, created_at, updated_at, confidence, inferred_status, user_confirmed_status)
                     VALUES ('t1', 'eski görev', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 0.5, 'DONE', 1);
                 INSERT INTO routines(id, name, schedule, timezone, action_type, parameters, created_at, updated_at)
                     VALUES ('morning_brief', 'Sabah', '09:00', 'Europe/Istanbul', 'MORNING_BRIEF', '{}', 'x', 'x');
                 INSERT INTO settings(key, value, updated_at) VALUES ('timezone', '\"Europe/Istanbul\"', 'x'),
                     ('display_name', '\"Demo\"', 'x');",
            )
            .unwrap();
        }

        let db = Db::open(&path).unwrap();
        let conn = db.conn();
        let columns = |table: &str| -> Vec<String> {
            let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})")).unwrap();
            stmt.query_map([], |r| r.get::<_, String>(1)).unwrap().map(|c| c.unwrap()).collect()
        };
        let task_cols = columns("tasks");
        for dropped in
            ["confidence", "inferred_status", "last_evidence_at", "user_confirmed_status"]
        {
            assert!(!task_cols.contains(&dropped.to_string()), "{dropped} kaldırılmalı");
        }
        assert!(!columns("routines").contains(&"timezone".to_string()));
        assert!(!columns("remote_messages").contains(&"attachment_meta".to_string()));

        let title: String =
            conn.query_row("SELECT title FROM tasks WHERE id='t1'", [], |r| r.get(0)).unwrap();
        assert_eq!(title, "eski görev");
        let routine: String = conn
            .query_row("SELECT schedule FROM routines WHERE id='morning_brief'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(routine, "09:00");
        let keys: Vec<String> = {
            let mut stmt = conn.prepare("SELECT key FROM settings ORDER BY key").unwrap();
            stmt.query_map([], |r| r.get(0)).unwrap().map(|k| k.unwrap()).collect()
        };
        assert_eq!(keys, vec!["display_name".to_string()]);
        let version: i64 =
            conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r.get(0)).unwrap();
        assert_eq!(version, MIGRATIONS.len() as i64);
    }

    #[test]
    fn migrations_are_idempotent_on_reopen() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("t.db");
        {
            let db = Db::open(&path).unwrap();
            db.conn()
                .execute(
                    "INSERT INTO projects(id, name, created_at, updated_at) VALUES ('p1','X','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
                    [],
                )
                .unwrap();
        }
        // ikinci açılış migration'ı tekrar uygulamamalı, veri durmalı
        let db = Db::open(&path).unwrap();
        let n: i64 =
            db.conn().query_row("SELECT COUNT(*) FROM projects", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
        let v: i64 = db
            .conn()
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, MIGRATIONS.len() as i64);
    }
}
