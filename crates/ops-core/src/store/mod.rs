mod agent;
mod audit;
mod detected;
mod evidence;
mod maintenance;
mod projects;
mod reminders;
mod remote;
mod repo_states;
mod routines;
mod settings;
mod tasks;

use std::str::FromStr;

use crate::db::Db;
use crate::{paths, Result};

pub use audit::verify_chain;

/// Tüm veri erişimi bu tip üzerinden. Her mutasyon `Ctx` (kim/nereden) ister ve
/// aynı transaction içinde audit kaydı üretir.
#[derive(Clone)]
pub struct Store {
    pub(crate) db: Db,
}

impl Store {
    pub fn open_default() -> Result<Self> {
        paths::ensure_data_dirs()?;
        Ok(Self { db: Db::open(&paths::db_path())? })
    }

    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub fn in_memory() -> Result<Self> {
        Ok(Self { db: Db::open_in_memory()? })
    }

    /// Bağlantı sağlığı (health.check için).
    pub fn ping(&self) -> Result<()> {
        self.db.conn().query_row("SELECT 1", [], |_r| Ok(()))?;
        Ok(())
    }
}

// Satır dönüşüm yardımcıları (query_map closure'ları için).

/// Sütun dönüşüm hatasını rusqlite hatasına sarar.
pub(crate) fn conv_err<E>(e: E) -> rusqlite::Error
where
    E: std::error::Error + Send + Sync + 'static,
{
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
}

pub(crate) fn parse_enum<T>(s: &str) -> rusqlite::Result<T>
where
    T: FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    T::from_str(s).map_err(conv_err)
}

pub(crate) fn parse_enum_opt<T>(s: Option<String>) -> rusqlite::Result<Option<T>>
where
    T: FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    s.map(|v| parse_enum(&v)).transpose()
}

pub(crate) fn dt(s: String) -> rusqlite::Result<chrono::DateTime<chrono::Utc>> {
    crate::time::from_db(&s).map_err(conv_err)
}

pub(crate) fn dt_opt(s: Option<String>) -> rusqlite::Result<Option<chrono::DateTime<chrono::Utc>>> {
    s.map(dt).transpose()
}

pub(crate) fn json_list<T: serde::de::DeserializeOwned>(s: String) -> rusqlite::Result<Vec<T>> {
    serde_json::from_str(&s).map_err(conv_err)
}

// Girdi doğrulama yardımcıları.

/// 1–5 aralık doğrulaması (priority/importance/urgency).
pub(crate) fn check_scale(name: &str, v: i64) -> Result<i64> {
    if (1..=5).contains(&v) {
        Ok(v)
    } else {
        Err(crate::OpsError::Validation(format!("{name} 1–5 aralığında olmalı, gelen: {v}")))
    }
}

/// Zorunlu metin: boş olamaz, `max` karakteri aşamaz; kırpılmış hali döner.
pub(crate) fn check_text(name: &str, v: &str, max: usize) -> Result<String> {
    let t = v.trim();
    if t.is_empty() {
        return Err(crate::OpsError::Validation(format!("{name} boş olamaz")));
    }
    if t.chars().count() > max {
        return Err(crate::OpsError::Validation(format!(
            "{name} en fazla {max} karakter olabilir"
        )));
    }
    Ok(t.to_string())
}

/// Opsiyonel metin: boş olabilir, yalnızca üst sınır uygulanır.
pub(crate) fn check_text_allow_empty(v: &str, max: usize) -> Result<String> {
    if v.chars().count() > max {
        return Err(crate::OpsError::Validation(format!("metin en fazla {max} karakter olabilir")));
    }
    Ok(v.to_string())
}
