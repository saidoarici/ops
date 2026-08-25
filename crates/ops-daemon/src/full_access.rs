//! Parola korumalı, yalnızca yerel Tam Erişim yetkisi.
//!
//! Keychain'de parola değil Argon2 türevi saklanır. Düz metin parola yalnızca
//! ilgili UDS isteğinin belleğinde bulunur ve provider prompt'una aktarılmaz.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use argon2::Argon2;
use ops_core::OpsError;
use subtle::ConstantTimeEq;

const ACCOUNT: &str = "agent_full_access_hash";
const FORMAT_VERSION: &str = "v1";
const MAX_FAILURES: u8 = 5;
const LOCKOUT: Duration = Duration::from_secs(60);

#[derive(Default)]
struct Attempts {
    failures: u8,
    locked_until: Option<Instant>,
}

#[derive(Default)]
pub struct FullAccessAuth {
    attempts: Mutex<Attempts>,
}

impl FullAccessAuth {
    pub async fn configured(&self) -> Result<bool, OpsError> {
        Ok(ops_keychain::get_secret(ACCOUNT).await?.is_some())
    }

    pub async fn configure(
        &self,
        new_password: String,
        current_password: Option<String>,
    ) -> Result<(), OpsError> {
        validate_password(&new_password)?;
        if self.configured().await? {
            let current = current_password
                .as_deref()
                .ok_or_else(|| OpsError::Security("mevcut Tam Erişim parolası gerekli".into()))?;
            self.verify(current).await?;
        }
        let encoded =
            tokio::task::spawn_blocking(move || encode_password(&new_password))
                .await
                .map_err(|e| OpsError::Internal(format!("parola işlemi tamamlanamadı: {e}")))??;
        ops_keychain::set_secret(ACCOUNT, &encoded).await?;
        self.reset_attempts();
        Ok(())
    }

    pub async fn verify(&self, password: &str) -> Result<(), OpsError> {
        self.check_lockout()?;
        let encoded = ops_keychain::get_secret(ACCOUNT)
            .await?
            .ok_or_else(|| OpsError::Security("Tam Erişim parolası henüz ayarlanmamış".into()))?;
        let candidate = password.to_string();
        let ok = tokio::task::spawn_blocking(move || verify_password(&candidate, &encoded))
            .await
            .map_err(|e| OpsError::Internal(format!("parola doğrulanamadı: {e}")))??;
        if ok {
            self.reset_attempts();
            Ok(())
        } else {
            self.record_failure();
            Err(OpsError::Security("Tam Erişim parolası yanlış".into()))
        }
    }

    fn check_lockout(&self) -> Result<(), OpsError> {
        let mut attempts = self.attempts.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(until) = attempts.locked_until {
            if until > Instant::now() {
                return Err(OpsError::Security(
                    "çok fazla hatalı deneme; 60 saniye sonra tekrar dene".into(),
                ));
            }
            *attempts = Attempts::default();
        }
        Ok(())
    }

    fn record_failure(&self) {
        let mut attempts = self.attempts.lock().unwrap_or_else(|e| e.into_inner());
        attempts.failures = attempts.failures.saturating_add(1);
        if attempts.failures >= MAX_FAILURES {
            attempts.locked_until = Some(Instant::now() + LOCKOUT);
        }
    }

    fn reset_attempts(&self) {
        *self.attempts.lock().unwrap_or_else(|e| e.into_inner()) = Attempts::default();
    }
}

fn validate_password(password: &str) -> Result<(), OpsError> {
    let len = password.chars().count();
    if !(10..=128).contains(&len) {
        return Err(OpsError::Validation("Tam Erişim parolası 10–128 karakter olmalı".into()));
    }
    if password.chars().any(char::is_control) {
        return Err(OpsError::Validation("parola kontrol karakteri içeremez".into()));
    }
    Ok(())
}

fn encode_password(password: &str) -> Result<String, OpsError> {
    let salt = uuid::Uuid::new_v4().simple().to_string();
    let mut output = [0_u8; 32];
    Argon2::default()
        .hash_password_into(password.as_bytes(), salt.as_bytes(), &mut output)
        .map_err(|e| OpsError::Internal(format!("Argon2 hatası: {e}")))?;
    Ok(format!("{FORMAT_VERSION}.{salt}.{}", hex::encode(output)))
}

fn verify_password(password: &str, encoded: &str) -> Result<bool, OpsError> {
    let mut parts = encoded.split('.');
    let (Some(version), Some(salt), Some(expected), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(OpsError::Internal("Keychain parola kaydı bozuk".into()));
    };
    if version != FORMAT_VERSION || salt.len() != 32 {
        return Err(OpsError::Internal("Keychain parola kaydı desteklenmiyor".into()));
    }
    let expected = hex::decode(expected)
        .map_err(|_| OpsError::Internal("Keychain parola hash'i bozuk".into()))?;
    let mut output = [0_u8; 32];
    Argon2::default()
        .hash_password_into(password.as_bytes(), salt.as_bytes(), &mut output)
        .map_err(|e| OpsError::Internal(format!("Argon2 hatası: {e}")))?;
    Ok(output.as_slice().ct_eq(expected.as_slice()).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_roundtrip_and_wrong_password() {
        let encoded = encode_password("uzun-ve-guclu-parola").unwrap();
        assert!(!encoded.contains("uzun-ve-guclu-parola"));
        assert!(verify_password("uzun-ve-guclu-parola", &encoded).unwrap());
        assert!(!verify_password("yanlis-parola", &encoded).unwrap());
    }

    #[test]
    fn password_policy_is_bounded() {
        assert!(validate_password("kisa").is_err());
        assert!(validate_password("gecerli-parola").is_ok());
        assert!(validate_password(&"x".repeat(129)).is_err());
    }
}
