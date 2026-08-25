//! ops-remote — Remote Intake Gateway.
//!
//! Değiştirilemez sınır: bu crate execution dünyasına bağımlılık almaz
//! (Cargo.toml'da ops-agent yok). Remote mesajların tüm yetkisi
//! `gateway::apply_intent` içindeki typed Store yazımlarından ibarettir;
//! riskli onay ya da mod değişikliği API'si bu yüzeyde tanımsızdır.

pub mod gateway;
pub mod intent;
pub mod telegram;
pub mod whatsapp;

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::Serialize;
use tokio::time::{sleep, Duration};
use tracing::{info, warn};

use ops_core::models::{Ctx, NotificationChannel, RemoteChannel};
use ops_core::store::Store;
use ops_core::{time, OpsError};

use crate::gateway::{GatewayConfig, IncomingMessage};
use crate::telegram::TelegramClient;
use crate::whatsapp::{WhatsAppConfig, WhatsAppProvider, DEFAULT_SEND_PATH};

const TELEGRAM_TOKEN_ACCOUNT: &str = "telegram_bot_token";
const WHATSAPP_KEY_ACCOUNT: &str = "whatsapp_api_token";
const CONFIG_CHECK_SECS: u64 = 30;
const POLL_RETRY_SECS: u64 = 15;
/// Allowlist dışı bir gönderici saatte bu kadar mesajdan sonra kayıt bile almaz.
const REJECT_LIMIT_PER_HOUR: u32 = 10;
/// Rate-limit tablosunun üst sınırı; aşılınca eski girdiler atılır.
const REJECT_TABLE_MAX: usize = 1000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TelegramStatus {
    pub configured: bool,
    pub enabled: bool,
    pub polling: bool,
    pub bot_name: Option<String>,
    pub allowed_user_set: bool,
    pub allowed_chat_set: bool,
    pub last_poll_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WhatsAppStatus {
    pub configured: bool,
    pub base_url: Option<String>,
    /// Maskelenmiş numara (yalnızca son 4 hane).
    pub phone_number: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteStatus {
    pub telegram: TelegramStatus,
    pub whatsapp: WhatsAppStatus,
}

#[derive(Default)]
struct RemoteState {
    polling: bool,
    bot_name: Option<String>,
    last_poll_at: Option<DateTime<Utc>>,
    last_error: Option<String>,
    /// Yetkisiz gönderici başına (sayaç, pencere başlangıcı).
    reject_counts: HashMap<String, (u32, DateTime<Utc>)>,
}

pub struct Remote {
    store: Store,
    state: tokio::sync::Mutex<RemoteState>,
}

impl Remote {
    pub fn new(store: Store) -> Arc<Self> {
        Arc::new(Self { store, state: tokio::sync::Mutex::new(RemoteState::default()) })
    }

    fn setting_str(&self, key: &str) -> Option<String> {
        self.store.get_setting(key).ok().flatten().and_then(|v| {
            v.as_str().map(str::to_string).or_else(|| v.as_i64().map(|n| n.to_string()))
        })
    }

    fn telegram_enabled(&self) -> bool {
        self.store
            .get_setting("telegram_enabled")
            .ok()
            .flatten()
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    /// Etkin ve eksiksiz Telegram yapılandırması: (token, allowlist).
    async fn load_telegram_config(&self) -> Option<(String, GatewayConfig)> {
        if !self.telegram_enabled() {
            return None;
        }
        let token = ops_keychain::get_secret(TELEGRAM_TOKEN_ACCOUNT).await.ok().flatten()?;
        let user = self.setting_str("telegram_allowed_user_id")?;
        let chat = self.setting_str("telegram_allowed_chat_id")?;
        if user.is_empty() || chat.is_empty() {
            return None;
        }
        Some((token, GatewayConfig { allowed_user_id: user, allowed_chat_id: chat }))
    }

    fn load_whatsapp_config(&self) -> Option<WhatsAppConfig> {
        let v = self.store.get_setting("whatsapp_config").ok().flatten()?;
        if v.is_null() {
            return None;
        }
        serde_json::from_value(v).ok()
    }

    pub async fn status(&self) -> RemoteStatus {
        let token_set =
            ops_keychain::get_secret(TELEGRAM_TOKEN_ACCOUNT).await.ok().flatten().is_some();
        let st = self.state.lock().await;
        let wa = self.load_whatsapp_config();
        RemoteStatus {
            telegram: TelegramStatus {
                configured: token_set,
                enabled: self.telegram_enabled(),
                polling: st.polling,
                bot_name: st.bot_name.clone(),
                allowed_user_set: self.setting_str("telegram_allowed_user_id").is_some(),
                allowed_chat_set: self.setting_str("telegram_allowed_chat_id").is_some(),
                last_poll_at: st.last_poll_at,
                last_error: st.last_error.clone(),
            },
            whatsapp: WhatsAppStatus {
                configured: wa.is_some(),
                base_url: wa.as_ref().map(|c| c.base_url.clone()),
                phone_number: wa.as_ref().map(|c| mask_phone(&c.phone_number)),
            },
        }
    }

    /// Yapılandırılmış giden kanallar (rutin brifleri için).
    pub async fn outbound_channels(&self) -> Vec<NotificationChannel> {
        let mut out = Vec::new();
        if self.load_telegram_config().await.is_some() {
            out.push(NotificationChannel::Telegram);
        }
        if self.load_whatsapp_config().is_some() {
            out.push(NotificationChannel::Whatsapp);
        }
        out
    }

    /// Token'ı doğrular (getMe), Keychain'e yazar, allowlist'i ayarlara işler.
    /// Yalnızca lokal UI'dan çağrılabilir (UDS zaten lokal-tek-kullanıcı).
    pub async fn configure_telegram(
        &self,
        token: &str,
        allowed_user_id: &str,
        allowed_chat_id: &str,
    ) -> Result<String, OpsError> {
        ops_keychain::validate_telegram_token(token)?;
        let numeric = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit() || c == '-');
        if !numeric(allowed_user_id) || !numeric(allowed_chat_id) {
            return Err(OpsError::Validation("user id ve chat id sayısal olmalı".into()));
        }
        let client = TelegramClient::new(token.to_string());
        let bot_name = client
            .get_me()
            .await
            .map_err(|e| OpsError::Validation(format!("token doğrulanamadı: {e}")))?;

        ops_keychain::set_secret(TELEGRAM_TOKEN_ACCOUNT, token).await?;
        let ctx = Ctx::LOCAL_USER;
        self.store.set_setting(&ctx, "telegram_enabled", serde_json::json!(true))?;
        self.store.set_setting(
            &ctx,
            "telegram_allowed_user_id",
            serde_json::json!(allowed_user_id),
        )?;
        self.store.set_setting(
            &ctx,
            "telegram_allowed_chat_id",
            serde_json::json!(allowed_chat_id),
        )?;

        let mut st = self.state.lock().await;
        st.bot_name = Some(bot_name.clone());
        st.last_error = None;
        Ok(bot_name)
    }

    pub async fn disable_telegram(&self) -> Result<(), OpsError> {
        ops_keychain::delete_secret(TELEGRAM_TOKEN_ACCOUNT).await?;
        self.store.set_setting(&Ctx::LOCAL_USER, "telegram_enabled", serde_json::json!(false))?;
        let mut st = self.state.lock().await;
        st.polling = false;
        st.bot_name = None;
        Ok(())
    }

    pub async fn test_telegram(&self) -> Result<String, OpsError> {
        let token = ops_keychain::get_secret(TELEGRAM_TOKEN_ACCOUNT)
            .await?
            .ok_or_else(|| OpsError::NotFound("Telegram token'ı kayıtlı değil".into()))?;
        TelegramClient::new(token).get_me().await.map_err(OpsError::Validation)
    }

    /// Giden bildirim: yalnızca allowlist'teki sohbete gönderilir.
    pub async fn send_telegram(&self, text: &str) -> Result<(), OpsError> {
        let Some((token, cfg)) = self.load_telegram_config().await else {
            return Err(OpsError::Validation("Telegram yapılandırılmamış".into()));
        };
        TelegramClient::new(token)
            .send_message(&cfg.allowed_chat_id, text)
            .await
            .map_err(OpsError::Internal)
    }

    /// WhatsApp'ı self-hosted bot API'sine bağlar (yalnızca giden yön).
    /// API anahtarı yalnızca Keychain'e yazılır; ayara URL + numara girer.
    pub async fn configure_whatsapp(
        &self,
        base_url: &str,
        api_key: &str,
        phone_number: &str,
    ) -> Result<String, OpsError> {
        let base = whatsapp::validate_base_url(base_url)?;
        let phone = phone_number.trim();
        if !(8..=15).contains(&phone.len()) || !phone.chars().all(|c| c.is_ascii_digit()) {
            return Err(OpsError::Validation(
                "numara ülke kodlu ve yalnızca rakam olmalı (ör. 90555…)".into(),
            ));
        }
        let key = api_key.trim();
        if key.is_empty() {
            return Err(OpsError::Validation("API anahtarı boş olamaz".into()));
        }
        ops_keychain::set_secret(WHATSAPP_KEY_ACCOUNT, key).await?;
        let mut cfg = WhatsAppConfig {
            base_url: base,
            phone_number: phone.to_string(),
            send_path: DEFAULT_SEND_PATH.to_string(),
            custom_id: None,
        };
        // Çoklu oturumlu botlarda ilk READY oturumun kimliği otomatik keşfedilir.
        let session_note = match WhatsAppProvider::new(cfg.clone()).discover_sessions().await {
            Ok(sessions) => match sessions.first() {
                Some((cid, name)) => {
                    cfg.custom_id = Some(cid.clone());
                    format!("; oturum: {name}")
                }
                None => String::new(),
            },
            Err(e) => format!("; oturum keşfi başarısız: {e}"),
        };
        let value = serde_json::to_value(&cfg)
            .map_err(|e| OpsError::Internal(format!("config serileştirilemedi: {e}")))?;
        self.store.set_setting(&Ctx::LOCAL_USER, "whatsapp_config", value)?;
        match WhatsAppProvider::new(cfg).verify().await {
            Ok(msg) => Ok(format!("kaydedildi; {msg}{session_note}")),
            Err(e) => Ok(format!(
                "kaydedildi; erişim testi başarısız: {e} (sunucu IP allowlist'ine bu Mac'i \
                 eklemen gerekebilir){session_note}"
            )),
        }
    }

    pub async fn disable_whatsapp(&self) -> Result<(), OpsError> {
        ops_keychain::delete_secret(WHATSAPP_KEY_ACCOUNT).await?;
        self.store.set_setting(&Ctx::LOCAL_USER, "whatsapp_config", serde_json::Value::Null)?;
        Ok(())
    }

    pub async fn test_whatsapp(&self) -> Result<String, OpsError> {
        let cfg = self
            .load_whatsapp_config()
            .ok_or_else(|| OpsError::NotFound("WhatsApp yapılandırılmamış".into()))?;
        WhatsAppProvider::new(cfg).verify().await.map_err(OpsError::Validation)
    }

    /// Giden bildirim: yalnızca yapılandırılmış numaraya gönderilir.
    pub async fn send_whatsapp(&self, text: &str) -> Result<(), OpsError> {
        let cfg = self
            .load_whatsapp_config()
            .ok_or_else(|| OpsError::Validation("WhatsApp yapılandırılmamış".into()))?;
        WhatsAppProvider::new(cfg).send(text).await.map_err(OpsError::Internal)
    }

    /// Yetkisiz göndericiler için kaba saatlik limit (docs/threat-model.md T3).
    async fn should_rate_limit(&self, sender: &str) -> bool {
        let now = time::now();
        let mut st = self.state.lock().await;
        if st.reject_counts.len() >= REJECT_TABLE_MAX {
            st.reject_counts.retain(|_, (_, since)| now - *since <= ChronoDuration::hours(1));
        }
        let entry = st.reject_counts.entry(sender.to_string()).or_insert((0, now));
        if now - entry.1 > ChronoDuration::hours(1) {
            *entry = (0, now);
        }
        entry.0 += 1;
        entry.0 > REJECT_LIMIT_PER_HOUR
    }
}

/// Numarayı durum görünümü için maskele (son 4 hane kalır).
fn mask_phone(p: &str) -> String {
    if p.len() > 4 {
        format!("…{}", &p[p.len() - 4..])
    } else {
        "…".into()
    }
}

/// Telegram long-poll döngüsü. Yapılandırma yoksa sessizce bekler ve periyodik
/// olarak yeniden kontrol eder; ağ hatasında kısa bir aradan sonra devam eder.
pub fn spawn(
    remote: Arc<Remote>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            if *shutdown.borrow() {
                break;
            }
            let Some((token, cfg)) = remote.load_telegram_config().await else {
                remote.state.lock().await.polling = false;
                tokio::select! {
                    _ = shutdown.changed() => break,
                    _ = sleep(Duration::from_secs(CONFIG_CHECK_SECS)) => continue,
                }
            };

            let client = TelegramClient::new(token);
            remote.state.lock().await.polling = true;
            let offset = remote
                .store
                .get_setting("telegram_last_update_id")
                .ok()
                .flatten()
                .and_then(|v| v.as_i64())
                .map(|v| v + 1)
                .unwrap_or(0);

            let updates = tokio::select! {
                _ = shutdown.changed() => break,
                res = client.get_updates(offset) => res,
            };
            remote.state.lock().await.last_poll_at = Some(time::now());
            match updates {
                Ok(messages) => {
                    for m in messages {
                        // İmleç işleme öncesi ilerletilir: bir mesaj en fazla bir kez
                        // denenir (at-most-once); replay koruması zaten external id'dedir.
                        let _ = remote.store.set_setting_unaudited(
                            "telegram_last_update_id",
                            serde_json::json!(m.update_id),
                        );
                        if m.text.is_empty() || m.message_id == 0 {
                            continue;
                        }
                        let authorized_sender = m.sender_id == cfg.allowed_user_id;
                        if !authorized_sender && remote.should_rate_limit(&m.sender_id).await {
                            continue;
                        }
                        let incoming = IncomingMessage {
                            channel: RemoteChannel::Telegram,
                            external_id: m.message_id.to_string(),
                            sender_id: m.sender_id,
                            chat_id: m.chat_id,
                            text: m.text,
                        };
                        match gateway::process_incoming(&remote.store, &cfg, &incoming) {
                            Ok(Some(reply)) => {
                                if let Err(e) =
                                    client.send_message(&cfg.allowed_chat_id, &reply).await
                                {
                                    warn!(error = %e, "Telegram yanıtı gönderilemedi");
                                }
                            }
                            Ok(None) => {}
                            Err(e) => warn!(error = %e, "remote mesaj işlenemedi"),
                        }
                    }
                    remote.state.lock().await.last_error = None;
                }
                Err(e) => {
                    remote.state.lock().await.last_error = Some(e.clone());
                    warn!(error = %e, "Telegram poll hatası; {POLL_RETRY_SECS} sn sonra tekrar");
                    tokio::select! {
                        _ = shutdown.changed() => break,
                        _ = sleep(Duration::from_secs(POLL_RETRY_SECS)) => {}
                    }
                }
            }
        }
        info!("remote gateway kapandı");
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unauthorized_senders_are_rate_limited_per_hour() {
        let remote = Remote::new(Store::in_memory().unwrap());
        for _ in 0..REJECT_LIMIT_PER_HOUR {
            assert!(!remote.should_rate_limit("stranger").await);
        }
        assert!(remote.should_rate_limit("stranger").await, "limit aşımı");
        assert!(!remote.should_rate_limit("another").await, "göndericiler bağımsız sayılır");
    }

    #[test]
    fn phone_is_masked_in_status() {
        assert_eq!(mask_phone("905551234567"), "…4567");
        assert_eq!(mask_phone("123"), "…");
    }
}
