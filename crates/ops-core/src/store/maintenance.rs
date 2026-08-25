use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;

use chrono::{DateTime, Utc};
use rusqlite::backup::Backup;
use rusqlite::Connection;

use crate::models::{AuditResult, BackupInfo, Ctx, NewAudit, RiskLevel};
use crate::store::Store;
use crate::{time, Result};

const KEEP_BACKUPS: usize = 10;

impl Store {
    /// SQLite online backup: canlı DB'den tutarlı kopya. Şemada secret alanı
    /// olmadığı için yedek de secret içermez.
    pub fn backup_to(&self, ctx: &Ctx, dir: &Path) -> Result<BackupInfo> {
        fs::create_dir_all(dir)?;
        let stamp = time::now().format("%Y%m%d-%H%M%S");
        let file_name = format!("personalops-{stamp}.db");
        let dest = dir.join(&file_name);
        {
            let conn = self.db.conn();
            let mut dst = Connection::open(&dest)?;
            let bk = Backup::new(&conn, &mut dst)?;
            bk.run_to_completion(64, Duration::from_millis(25), None)?;
        }
        let _ = fs::set_permissions(&dest, fs::Permissions::from_mode(0o600));
        prune(dir)?;
        self.append_audit(NewAudit {
            actor: ctx.actor,
            origin: ctx.origin,
            action: "BACKUP_CREATE".into(),
            target: Some(file_name.clone()),
            risk_level: RiskLevel::R0,
            capability: None,
            result: AuditResult::Ok,
            metadata: serde_json::json!({}),
        })?;
        let meta = fs::metadata(&dest)?;
        Ok(BackupInfo {
            file_name,
            path: dest.display().to_string(),
            size_bytes: meta.len(),
            created_at: meta.modified().ok().map(DateTime::<Utc>::from),
        })
    }

    pub fn list_backups(&self, dir: &Path) -> Result<Vec<BackupInfo>> {
        let mut out = Vec::new();
        if !dir.exists() {
            return Ok(out);
        }
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with("personalops-") || !name.ends_with(".db") {
                continue;
            }
            let meta = entry.metadata()?;
            out.push(BackupInfo {
                file_name: name,
                path: entry.path().display().to_string(),
                size_bytes: meta.len(),
                created_at: meta.modified().ok().map(DateTime::<Utc>::from),
            });
        }
        out.sort_by(|a, b| b.file_name.cmp(&a.file_name));
        Ok(out)
    }
}

fn prune(dir: &Path) -> Result<()> {
    let mut names: Vec<String> = fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.starts_with("personalops-") && n.ends_with(".db"))
        .collect();
    names.sort();
    while names.len() > KEEP_BACKUPS {
        let oldest = names.remove(0);
        let _ = fs::remove_file(dir.join(oldest));
    }
    Ok(())
}
