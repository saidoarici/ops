//! macOS Keychain erişimi — `/usr/bin/security` üzerinden.
//!
//! Neden CLI? `security` binary'sinin keychain ACL'si sabittir; daemon binary'si
//! her derlemede değişse de erişim istikrarlı kalır. Secret'lar:
//! - yazarken STDIN'den verilir (ps çıktısında görünmez),
//! - okunurken yalnızca stdout'tan alınır,
//! - asla loglanmaz, DB'ye yazılmaz, export edilmez (docs/threat-model.md T13/T14).
//!
//! Hesap adı ve değer önce katı biçim doğrulamasından geçer; `security`'nin
//! etkileşimli komut satırına yalnızca doğrulanmış karakter kümesi girer.

use std::process::Stdio;

use ops_core::OpsError;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

const SERVICE: &str = "com.personalops.daemon";
const SECURITY: &str = "/usr/bin/security";

/// Keychain hesap adları: yalnızca `[a-z_]`.
fn check_account(account: &str) -> Result<(), OpsError> {
    if !account.is_empty() && account.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
        Ok(())
    } else {
        Err(OpsError::Internal("geçersiz keychain hesabı".into()))
    }
}

/// `security -i` satırına güvenle gömülebilen değer karakterleri: boşluk ve
/// tırnak içermeyen, URL/base64/token biçimlerinde görülen ASCII alt kümesi.
fn check_value(value: &str) -> Result<(), OpsError> {
    let ok = !value.is_empty()
        && value.len() <= 512
        && value.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, ':' | '_' | '-' | '.' | '+' | '/' | '=')
        });
    if ok {
        Ok(())
    } else {
        Err(OpsError::Validation("secret beklenmeyen karakter içeriyor".into()))
    }
}

/// Telegram bot token biçimi: `<botId>:<hash>` — botId rakam, hash
/// `[A-Za-z0-9_-]`. Hem hata yakalar hem de secret'ı güvenli karakter
/// kümesine sabitler.
pub fn validate_telegram_token(token: &str) -> Result<(), OpsError> {
    let Some((id, hash)) = token.split_once(':') else {
        return Err(OpsError::Validation("token biçimi geçersiz (botId:hash bekleniyor)".into()));
    };
    let id_ok = !id.is_empty() && id.len() <= 15 && id.chars().all(|c| c.is_ascii_digit());
    let hash_ok = hash.len() >= 20
        && hash.len() <= 80
        && hash.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if id_ok && hash_ok {
        Ok(())
    } else {
        Err(OpsError::Validation("token biçimi geçersiz".into()))
    }
}

pub async fn set_secret(account: &str, value: &str) -> Result<(), OpsError> {
    check_account(account)?;
    check_value(value)?;
    // `security -i`: komutlar stdin'den okunur; secret argv'ye/ps'e düşmez.
    let mut child = Command::new(SECURITY)
        .arg("-i")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| OpsError::Internal(format!("security başlatılamadı: {e}")))?;
    if let Some(mut stdin) = child.stdin.take() {
        let script = format!("add-generic-password -U -s {SERVICE} -a {account} -w {value}\n");
        stdin.write_all(script.as_bytes()).await.map_err(|e| OpsError::Internal(e.to_string()))?;
        stdin.shutdown().await.ok();
    }
    let status = child.wait().await.map_err(|e| OpsError::Internal(e.to_string()))?;
    if status.success() {
        Ok(())
    } else {
        Err(OpsError::Internal("Keychain'e yazılamadı".into()))
    }
}

/// Kayıt yoksa `Ok(None)`.
pub async fn get_secret(account: &str) -> Result<Option<String>, OpsError> {
    check_account(account)?;
    let out = Command::new(SECURITY)
        .args(["find-generic-password", "-s", SERVICE, "-a", account, "-w"])
        .stdin(Stdio::null())
        .output()
        .await
        .map_err(|e| OpsError::Internal(format!("security çalıştırılamadı: {e}")))?;
    if !out.status.success() {
        return Ok(None);
    }
    let value = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Ok((!value.is_empty()).then_some(value))
}

/// Kayıt yoksa sessizce başarılı sayılır.
pub async fn delete_secret(account: &str) -> Result<(), OpsError> {
    check_account(account)?;
    let _ = Command::new(SECURITY)
        .args(["delete-generic-password", "-s", SERVICE, "-a", account])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_validation_is_strict() {
        assert!(validate_telegram_token("123456789:AAHdqTcvbXYZ_abc-DEF1234567890ghij").is_ok());
        assert!(validate_telegram_token("kötü token").is_err());
        assert!(validate_telegram_token("123:kısa").is_err());
        assert!(validate_telegram_token("abc:AAHdqTcvbXYZ_abc-DEF1234567890ghij").is_err());
        // enjeksiyon denemeleri biçim doğrulamasını geçemez
        assert!(validate_telegram_token("1:x\" -s hack; rm -rf ~; \"aaaaaaaaaaaaaaa").is_err());
    }

    #[test]
    fn account_and_value_charsets_are_bounded() {
        assert!(check_account("telegram_bot_token").is_ok());
        assert!(check_account("Telegram").is_err());
        assert!(check_account("a b").is_err());
        assert!(check_account("").is_err());

        assert!(check_value("v1.abc.def+ghi/jk=").is_ok());
        assert!(check_value("has space").is_err());
        assert!(check_value("quote\"inside").is_err());
        assert!(check_value("newline\ninside").is_err());
        assert!(check_value("").is_err());
    }
}
