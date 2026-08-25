/// Çekirdek hata tipi. IPC katmanı `code()` ile stabil hata kodu üretir.
#[derive(Debug, thiserror::Error)]
pub enum OpsError {
    #[error("doğrulama hatası: {0}")]
    Validation(String),

    #[error("bulunamadı: {0}")]
    NotFound(String),

    #[error("çakışma: {0}")]
    Conflict(String),

    #[error("bilinmeyen metod: {0}")]
    UnknownMethod(String),

    #[error("güvenlik engeli: {0}")]
    Security(String),

    #[error("veritabanı hatası: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("serileştirme hatası: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("g/ç hatası: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Internal(String),
}

impl OpsError {
    pub fn code(&self) -> &'static str {
        match self {
            OpsError::Validation(_) => "VALIDATION",
            OpsError::NotFound(_) => "NOT_FOUND",
            OpsError::Conflict(_) => "CONFLICT",
            OpsError::UnknownMethod(_) => "UNKNOWN_METHOD",
            OpsError::Security(_) => "SECURITY",
            OpsError::Db(_) => "DB",
            OpsError::Serde(_) => "BAD_REQUEST",
            OpsError::Io(_) => "IO",
            OpsError::Internal(_) => "INTERNAL",
        }
    }
}

pub type Result<T> = std::result::Result<T, OpsError>;
