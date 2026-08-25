use chrono::{DateTime, SecondsFormat, Utc};

use crate::{OpsError, Result};

pub fn now() -> DateTime<Utc> {
    Utc::now()
}

/// DB temsili: UTC, RFC 3339, saniye hassasiyeti (`2026-08-23T20:15:00Z`).
pub fn to_db(dt: &DateTime<Utc>) -> String {
    dt.to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub fn from_db(s: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| OpsError::Internal(format!("geçersiz zaman damgası '{s}': {e}")))
}

pub fn opt_to_db(dt: &Option<DateTime<Utc>>) -> Option<String> {
    dt.as_ref().map(to_db)
}

pub fn opt_from_db(s: Option<String>) -> Result<Option<DateTime<Utc>>> {
    match s {
        Some(v) if !v.is_empty() => Ok(Some(from_db(&v)?)),
        _ => Ok(None),
    }
}

/// Kullanıcıdan gelen zaman: RFC 3339 bekler, doğrulama hatası döner.
pub fn parse_user(s: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|_| OpsError::Validation(format!("zaman RFC 3339 olmalı, gelen: '{s}'")))
}
