//! Telegram Bot API istemcisi — yalnızca long polling (getUpdates) ve
//! sendMessage/getMe. Inbound port açılmaz; token hata metinlerinde
//! maskelenir ve asla loglanmaz (docs/threat-model.md T13).

use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub struct TgMessage {
    pub update_id: i64,
    pub message_id: i64,
    pub sender_id: String,
    pub chat_id: String,
    pub text: String,
}

pub struct TelegramClient {
    http: reqwest::Client,
    token: String,
}

impl TelegramClient {
    pub fn new(token: String) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(70))
            .build()
            .expect("http istemcisi");
        Self { http, token }
    }

    fn url(&self, method: &str) -> String {
        format!("https://api.telegram.org/bot{}/{method}", self.token)
    }

    async fn call(&self, method: &str, body: Value) -> Result<Value, String> {
        let resp = self
            .http
            .post(self.url(method))
            .json(&body)
            .send()
            .await
            .map_err(|e| redact(&self.token, &format!("ağ hatası: {e}")))?;
        let status = resp.status();
        let v: Value = resp
            .json()
            .await
            .map_err(|e| redact(&self.token, &format!("yanıt çözümlenemedi: {e}")))?;
        if v.get("ok").and_then(|o| o.as_bool()) == Some(true) {
            Ok(v.get("result").cloned().unwrap_or(Value::Null))
        } else {
            let desc = v.get("description").and_then(|d| d.as_str()).unwrap_or("bilinmeyen hata");
            Err(format!("Telegram API {status}: {desc}"))
        }
    }

    pub async fn get_me(&self) -> Result<String, String> {
        let result = self.call("getMe", json!({})).await?;
        Ok(result.get("username").and_then(|u| u.as_str()).unwrap_or("bot").to_string())
    }

    /// Long poll: 50 sn bekler; yalnızca `message` update'leri.
    pub async fn get_updates(&self, offset: i64) -> Result<Vec<TgMessage>, String> {
        let result = self
            .call(
                "getUpdates",
                json!({ "timeout": 50, "offset": offset, "allowed_updates": ["message"] }),
            )
            .await?;
        let mut out = Vec::new();
        for update in result.as_array().cloned().unwrap_or_default() {
            let Some(update_id) = update.get("update_id").and_then(|u| u.as_i64()) else {
                continue;
            };
            let Some(message) = update.get("message") else {
                // message dışındaki update türleri istenmedi; yine de offset ilerlesin
                out.push(TgMessage {
                    update_id,
                    message_id: 0,
                    sender_id: String::new(),
                    chat_id: String::new(),
                    text: String::new(),
                });
                continue;
            };
            out.push(TgMessage {
                update_id,
                message_id: message.get("message_id").and_then(|m| m.as_i64()).unwrap_or(0),
                sender_id: message
                    .pointer("/from/id")
                    .and_then(|v| v.as_i64())
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
                chat_id: message
                    .pointer("/chat/id")
                    .and_then(|v| v.as_i64())
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
                text: message.get("text").and_then(|t| t.as_str()).unwrap_or("").to_string(),
            });
        }
        Ok(out)
    }

    pub async fn send_message(&self, chat_id: &str, text: &str) -> Result<(), String> {
        let trimmed: String = text.chars().take(3800).collect();
        self.call("sendMessage", json!({ "chat_id": chat_id, "text": trimmed })).await?;
        Ok(())
    }
}

/// Hata metinlerinde token asla görünmez (reqwest hataları URL'yi içerebilir).
fn redact(token: &str, message: &str) -> String {
    message.replace(token, "***")
}
