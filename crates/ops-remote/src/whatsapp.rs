//! WhatsApp — kullanıcının kendi barındırdığı bir WhatsApp bot API'sine
//! bağlanan, yalnızca giden yönlü (outbound-only) adapter.
//!
//! Beklenen sözleşme (self-hosted bot):
//! - Gönderim: POST `{baseUrl}{sendPath}` gövde `{"phoneNumber","message"}`;
//!   kimlik `x-api-key` başlığı, değeri yalnızca Keychain'deki
//!   `whatsapp_api_token` hesabından okunur.
//! - Oturum keşfi: GET `{baseUrl}/api/sessions` — çoklu oturumlu botlarda
//!   `x-custom-id` başlığı için READY oturum kimliği.
//! - Doğrulama: GET `{baseUrl}/api/get-groups` — erişilebilirlik + anahtar testi.
//!
//! Gelen yön yoktur: bot gelen mesajları yalnızca webhook push ile iletir ve
//! Personal Ops inbound port açmaz (docs/threat-model.md T2/T4). Uzaktan görev
//! girişi Telegram üzerinden yapılır.

use ops_core::OpsError;
use serde::{Deserialize, Serialize};

pub const DEFAULT_SEND_PATH: &str = "/api/send-to-user";
const API_KEY_HEADER: &str = "x-api-key";
const CUSTOM_ID_HEADER: &str = "x-custom-id";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WhatsAppConfig {
    /// ör. `https://bot.example.com`
    pub base_url: String,
    /// Bildirimlerin gideceği kayıtlı numara — ülke kodlu, `+`siz (ör. `9053…`).
    pub phone_number: String,
    #[serde(default = "default_send_path")]
    pub send_path: String,
    /// Botun çoklu-oturum kimliği (`x-custom-id` başlığı). Bot, oturum
    /// belirtilmeyen isteği reddeder; configure sırasında `/api/sessions`
    /// üzerinden otomatik keşfedilir.
    #[serde(default)]
    pub custom_id: Option<String>,
}

fn default_send_path() -> String {
    DEFAULT_SEND_PATH.to_string()
}

/// Bot adresini doğrular ve normalize eder (sondaki `/` atılır). API anahtarı
/// her istekte başlıkta gittiği için düz `http://` yalnızca loopback için kabul
/// edilir; uzak sunucular `https://` olmak zorundadır.
pub fn validate_base_url(raw: &str) -> Result<String, OpsError> {
    let base = raw.trim().trim_end_matches('/');
    let invalid = |why: &str| OpsError::Validation(format!("bot adresi geçersiz: {why}"));
    if base.len() > 200 {
        return Err(invalid("çok uzun"));
    }
    let url = reqwest::Url::parse(base).map_err(|_| invalid("https://… bekleniyor"))?;
    let host = url.host_str().ok_or_else(|| invalid("sunucu adı yok"))?;
    if url.query().is_some() || url.fragment().is_some() {
        return Err(invalid("sorgu/parça içeremez"));
    }
    let loopback = matches!(host, "localhost" | "127.0.0.1" | "[::1]");
    match url.scheme() {
        "https" => Ok(base.to_string()),
        "http" if loopback => Ok(base.to_string()),
        "http" => Err(invalid("uzak sunucu için https gerekli")),
        _ => Err(invalid("https://… bekleniyor")),
    }
}

pub struct WhatsAppProvider {
    cfg: WhatsAppConfig,
    http: reqwest::Client,
}

impl WhatsAppProvider {
    pub fn new(cfg: WhatsAppConfig) -> Self {
        // IPv4'e sabitlenir: bot sunucusundaki IP allowlist'i tek bir çıkış IP'si
        // görsün (dual-stack ağda IPv6'ya kayınca allowlist eşleşmez).
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .local_address(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED))
            .build()
            .expect("http istemcisi");
        Self { cfg, http }
    }

    async fn api_key(&self) -> Result<String, String> {
        ops_keychain::get_secret("whatsapp_api_token")
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "WhatsApp API anahtarı Keychain'de kayıtlı değil".to_string())
    }

    /// GET /api/sessions — READY oturumları döndürür (customId, name).
    pub async fn discover_sessions(&self) -> Result<Vec<(String, String)>, String> {
        let url = format!("{}/api/sessions", self.cfg.base_url.trim_end_matches('/'));
        let key = self.api_key().await?;
        let resp = self
            .http
            .get(&url)
            .header(API_KEY_HEADER, key)
            .send()
            .await
            .map_err(|e| format!("ağ hatası: {e}"))?;
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(format!("HTTP {status}: {}", truncate(&body)));
        }
        #[derive(Deserialize)]
        struct Sess {
            #[serde(default)]
            custom_id: Option<String>,
            #[serde(rename = "customId", default)]
            custom_id_camel: Option<String>,
            #[serde(default)]
            name: Option<String>,
            #[serde(default)]
            status: Option<String>,
        }
        let rows: Vec<Sess> = serde_json::from_str(&body)
            .map_err(|e| format!("oturum listesi çözümlenemedi: {e}"))?;
        Ok(rows
            .into_iter()
            .filter(|s| s.status.as_deref() == Some("READY"))
            .filter_map(|s| {
                let cid = s.custom_id_camel.or(s.custom_id)?;
                Some((cid, s.name.unwrap_or_default()))
            })
            .collect())
    }

    /// GET /api/get-groups — erişilebilirlik + API anahtarı + oturum doğrulaması.
    pub async fn verify(&self) -> Result<String, String> {
        let url = format!("{}/api/get-groups", self.cfg.base_url.trim_end_matches('/'));
        let key = self.api_key().await?;
        let mut req = self.http.get(&url).header(API_KEY_HEADER, key);
        if let Some(cid) = &self.cfg.custom_id {
            req = req.header(CUSTOM_ID_HEADER, cid.as_str());
        }
        let resp = req.send().await.map_err(|e| format!("ağ hatası: {e}"))?;
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(format!("HTTP {status}: {}", truncate(&body)));
        }
        #[derive(Deserialize)]
        struct Groups {
            success: bool,
            #[serde(default)]
            groups: Vec<serde_json::Value>,
        }
        match serde_json::from_str::<Groups>(&body) {
            Ok(g) if g.success => Ok(format!("bot erişilebilir ({} grup görünür)", g.groups.len())),
            _ => Err(format!("beklenmeyen yanıt: {}", truncate(&body))),
        }
    }

    /// POST {sendPath} — `{"phoneNumber","message"}` (botun belgeli sözleşmesi).
    pub async fn send(&self, text: &str) -> Result<(), String> {
        let base = self.cfg.base_url.trim_end_matches('/');
        let path = if self.cfg.send_path.starts_with('/') {
            self.cfg.send_path.clone()
        } else {
            format!("/{}", self.cfg.send_path)
        };
        let key = self.api_key().await?;
        let mut req = self.http.post(format!("{base}{path}")).header(API_KEY_HEADER, key).json(
            &serde_json::json!({
                "phoneNumber": self.cfg.phone_number,
                "message": text,
            }),
        );
        if let Some(cid) = &self.cfg.custom_id {
            req = req.header(CUSTOM_ID_HEADER, cid.as_str());
        }
        let resp = req.send().await.map_err(|e| format!("ağ hatası: {e}"))?;
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(format!("HTTP {status}: {}", truncate(&body)));
        }
        #[derive(Deserialize)]
        struct Sent {
            success: bool,
        }
        match serde_json::from_str::<Sent>(&body) {
            Ok(s) if s.success => Ok(()),
            _ => Err(format!("gönderim onaylanmadı: {}", truncate(&body))),
        }
    }
}

/// Hata gövdelerini log/audit'e sığacak şekilde kısalt (secret sızıntı yüzeyini de daraltır).
fn truncate(s: &str) -> String {
    let t: String = s.chars().take(160).collect();
    if t.len() < s.len() {
        format!("{t}…")
    } else {
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_url_requires_https_except_loopback() {
        assert_eq!(
            validate_base_url("https://bot.example.com/").unwrap(),
            "https://bot.example.com"
        );
        assert_eq!(validate_base_url("http://localhost:3000").unwrap(), "http://localhost:3000");
        assert!(validate_base_url("http://bot.example.com").is_err());
        assert!(validate_base_url("ftp://bot.example.com").is_err());
        assert!(validate_base_url("bot.example.com").is_err());
        assert!(validate_base_url("https://bot.example.com/?x=1").is_err());
        assert!(validate_base_url("https://bot example.com").is_err());
    }

    #[test]
    fn config_camelcase_ve_default_send_path() {
        let cfg: WhatsAppConfig = serde_json::from_value(serde_json::json!({
            "baseUrl": "https://bot.example.com",
            "phoneNumber": "905551234567",
        }))
        .expect("parse");
        assert_eq!(cfg.send_path, DEFAULT_SEND_PATH);
        assert_eq!(cfg.phone_number, "905551234567");
        assert_eq!(cfg.custom_id, None); // eski kayıtlarla geriye uyumlu
    }

    #[test]
    fn config_custom_id_okunur() {
        let cfg: WhatsAppConfig = serde_json::from_value(serde_json::json!({
            "baseUrl": "https://bot.example.com",
            "phoneNumber": "905551234567",
            "customId": "session-1",
        }))
        .expect("parse");
        assert_eq!(cfg.custom_id.as_deref(), Some("session-1"));
    }

    #[test]
    fn config_send_path_ozellestirilebilir() {
        let cfg: WhatsAppConfig = serde_json::from_value(serde_json::json!({
            "baseUrl": "https://bot.example.com/",
            "phoneNumber": "905551234567",
            "sendPath": "/api/reply-to-message",
        }))
        .expect("parse");
        assert_eq!(cfg.send_path, "/api/reply-to-message");
    }

    #[test]
    fn config_serialize_roundtrip() {
        let cfg = WhatsAppConfig {
            base_url: "https://bot.example.com".into(),
            phone_number: "905551234567".into(),
            send_path: DEFAULT_SEND_PATH.into(),
            custom_id: Some("session-1".into()),
        };
        let v = serde_json::to_value(&cfg).expect("ser");
        assert_eq!(v["baseUrl"], "https://bot.example.com");
        let back: WhatsAppConfig = serde_json::from_value(v).expect("de");
        assert_eq!(back.base_url, cfg.base_url);
    }
}
