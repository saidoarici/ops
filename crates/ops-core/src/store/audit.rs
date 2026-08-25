//! Tamper-evident audit: her kayıt bir öncekinin hash'ine zincirlenir.
//! Formül (docs/data-model.md, "Audit hash chain"):
//! `hash = hex(SHA-256(previous_hash + "\n" + canonical))`
//! `canonical` = hash alanı hariç kayıt; alan sırası sabit struct sırası;
//! `metadata` DB'de saklanan **ham JSON metni** olarak dahil edilir (yeniden
//! serileştirme yapılmaz ki doğrulama her zaman bit-bit aynı kalsın).

use rusqlite::Connection;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::models::{AuditEvent, AuditVerifyReport, NewAudit};
use crate::store::{conv_err, dt, parse_enum, Store};
use crate::{time, Result};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Canonical<'a> {
    id: &'a str,
    seq: i64,
    timestamp: &'a str,
    actor: &'a str,
    origin: &'a str,
    action: &'a str,
    target: &'a Option<String>,
    risk_level: &'a str,
    capability: &'a Option<String>,
    result: &'a str,
    metadata: &'a str,
    previous_hash: &'a str,
}

fn compute_hash(previous_hash: &str, canonical: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(previous_hash.as_bytes());
    hasher.update(b"\n");
    hasher.update(canonical.as_bytes());
    hex::encode(hasher.finalize())
}

/// Bir transaction (veya düz bağlantı) içinde audit kaydı ekler.
/// Store mutasyonları bunu kendi tx'lerinin içinden çağırır: iş + izi atomiktir.
pub(crate) fn append_tx(conn: &Connection, new: &NewAudit) -> Result<AuditEvent> {
    let seq: i64 =
        conn.query_row("SELECT COALESCE(MAX(seq), 0) + 1 FROM audit_events", [], |r| r.get(0))?;
    let previous_hash: String = if seq == 1 {
        "GENESIS".to_string()
    } else {
        conn.query_row("SELECT hash FROM audit_events WHERE seq = ?1", [seq - 1], |r| r.get(0))?
    };

    let id = uuid::Uuid::new_v4().to_string();
    let now = time::now();
    let timestamp = time::to_db(&now);
    let metadata_str = new.metadata.to_string();

    let canonical = serde_json::to_string(&Canonical {
        id: &id,
        seq,
        timestamp: &timestamp,
        actor: new.actor.as_ref(),
        origin: new.origin.as_ref(),
        action: &new.action,
        target: &new.target,
        risk_level: new.risk_level.as_ref(),
        capability: &new.capability,
        result: new.result.as_ref(),
        metadata: &metadata_str,
        previous_hash: &previous_hash,
    })?;
    let hash = compute_hash(&previous_hash, &canonical);

    conn.execute(
        "INSERT INTO audit_events(id, seq, timestamp, actor, origin, action, target, risk_level,
                                  capability, result, metadata, previous_hash, hash)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
        rusqlite::params![
            id,
            seq,
            timestamp,
            new.actor.as_ref(),
            new.origin.as_ref(),
            new.action,
            new.target,
            new.risk_level.as_ref(),
            new.capability,
            new.result.as_ref(),
            metadata_str,
            previous_hash,
            hash,
        ],
    )?;

    Ok(AuditEvent {
        id,
        seq,
        timestamp: now,
        actor: new.actor,
        origin: new.origin,
        action: new.action.clone(),
        target: new.target.clone(),
        risk_level: new.risk_level,
        capability: new.capability.clone(),
        result: new.result,
        metadata: new.metadata.clone(),
        previous_hash,
        hash,
    })
}

fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AuditEvent> {
    let metadata_str: String = row.get("metadata")?;
    Ok(AuditEvent {
        id: row.get("id")?,
        seq: row.get("seq")?,
        timestamp: dt(row.get("timestamp")?)?,
        actor: parse_enum(&row.get::<_, String>("actor")?)?,
        origin: parse_enum(&row.get::<_, String>("origin")?)?,
        action: row.get("action")?,
        target: row.get("target")?,
        risk_level: parse_enum(&row.get::<_, String>("risk_level")?)?,
        capability: row.get("capability")?,
        result: parse_enum(&row.get::<_, String>("result")?)?,
        metadata: serde_json::from_str(&metadata_str).map_err(conv_err)?,
        previous_hash: row.get("previous_hash")?,
        hash: row.get("hash")?,
    })
}

impl Store {
    /// Tekil audit kaydı (tx dışından; ör. daemon'ın bildirim gönderimi).
    pub fn append_audit(&self, new: NewAudit) -> Result<AuditEvent> {
        let conn = self.db.conn();
        append_tx(&conn, &new)
    }

    /// Son kayıtlar, seq azalan sırada.
    pub fn list_audit(&self, limit: i64, before_seq: Option<i64>) -> Result<Vec<AuditEvent>> {
        let limit = limit.clamp(1, 500);
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT * FROM audit_events WHERE (?1 IS NULL OR seq < ?1)
             ORDER BY seq DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![before_seq, limit], from_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Zinciri baştan sona yeniden hesaplayarak doğrular (docs/threat-model.md T18).
    pub fn verify_audit(&self) -> Result<AuditVerifyReport> {
        let conn = self.db.conn();
        verify_chain(&conn)
    }
}

pub fn verify_chain(conn: &Connection) -> Result<AuditVerifyReport> {
    struct Raw {
        id: String,
        seq: i64,
        timestamp: String,
        actor: String,
        origin: String,
        action: String,
        target: Option<String>,
        risk_level: String,
        capability: Option<String>,
        result: String,
        metadata: String,
        previous_hash: String,
        hash: String,
    }

    let mut stmt = conn.prepare("SELECT * FROM audit_events ORDER BY seq ASC")?;
    let rows = stmt.query_map([], |row| {
        Ok(Raw {
            id: row.get("id")?,
            seq: row.get("seq")?,
            timestamp: row.get("timestamp")?,
            actor: row.get("actor")?,
            origin: row.get("origin")?,
            action: row.get("action")?,
            target: row.get("target")?,
            risk_level: row.get("risk_level")?,
            capability: row.get("capability")?,
            result: row.get("result")?,
            metadata: row.get("metadata")?,
            previous_hash: row.get("previous_hash")?,
            hash: row.get("hash")?,
        })
    })?;

    let mut expected_prev = "GENESIS".to_string();
    let mut checked: i64 = 0;
    for row in rows {
        let r = row?;
        checked += 1;
        if r.seq != checked {
            return Ok(broken(checked, r.seq, "seq sırası kopuk"));
        }
        if r.previous_hash != expected_prev {
            return Ok(broken(checked, r.seq, "previous_hash zincire uymuyor"));
        }
        let canonical = serde_json::to_string(&Canonical {
            id: &r.id,
            seq: r.seq,
            timestamp: &r.timestamp,
            actor: &r.actor,
            origin: &r.origin,
            action: &r.action,
            target: &r.target,
            risk_level: &r.risk_level,
            capability: &r.capability,
            result: &r.result,
            metadata: &r.metadata,
            previous_hash: &r.previous_hash,
        })?;
        if compute_hash(&r.previous_hash, &canonical) != r.hash {
            return Ok(broken(checked, r.seq, "kayıt içeriği hash ile uyuşmuyor"));
        }
        expected_prev = r.hash;
    }

    Ok(AuditVerifyReport {
        ok: true,
        checked,
        broken_at_seq: None,
        message: format!("{checked} kayıt doğrulandı, zincir sağlam"),
    })
}

fn broken(checked: i64, seq: i64, why: &str) -> AuditVerifyReport {
    AuditVerifyReport {
        ok: false,
        checked,
        broken_at_seq: Some(seq),
        message: format!("zincir seq {seq} kaydında kırık: {why}"),
    }
}
