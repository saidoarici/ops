//! Remote Intake Gateway — mimarideki "untrusted inbox".
//!
//! Bu modülde process başlatma, dosya sistemi erişimi ve agent çağrısı yoktur.
//! Bir remote mesajın yapabildiği her şey bu dosyadadır ve tamamı Store'a
//! typed veri yazmaktan ibarettir (docs/threat-model.md T1/T3/T5/T19).

use ops_core::models::{
    Actor, AuditResult, Ctx, NewAudit, NewRemoteMessage, RemoteAuthState, RemoteChannel,
    RemoteIntent, RemoteProcessingStatus, RiskLevel, TaskCreate, TaskFilter, TaskStatus,
};
use ops_core::store::Store;
use ops_core::Result;

#[derive(Debug, Clone)]
pub struct IncomingMessage {
    pub channel: RemoteChannel,
    pub external_id: String,
    pub sender_id: String,
    pub chat_id: String,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct GatewayConfig {
    pub allowed_user_id: String,
    pub allowed_chat_id: String,
}

const MAX_STORED_TEXT_CHARS: usize = 4000;

/// Mesajı işler ve (yetkiliyse) gönderilecek yanıt metnini döner.
/// Yetkisiz gönderici: içerik saklanmaz, işlenmez, yanıtlanmaz — yalnızca
/// güvenli metadata + audit.
pub fn process_incoming(
    store: &Store,
    cfg: &GatewayConfig,
    msg: &IncomingMessage,
) -> Result<Option<String>> {
    let ctx = Ctx { actor: Actor::Remote, origin: msg.channel.origin() };

    let authorized = !cfg.allowed_user_id.is_empty()
        && msg.sender_id == cfg.allowed_user_id
        && msg.chat_id == cfg.allowed_chat_id;

    if !authorized {
        let recorded = store.record_remote_message(NewRemoteMessage {
            channel: msg.channel,
            external_message_id: msg.external_id.clone(),
            sender_id: msg.sender_id.clone(),
            raw_text: String::new(),
            authentication_state: RemoteAuthState::RejectedSender,
        })?;
        if let Some(r) = recorded {
            store.finalize_remote_message(&r.id, None, None, RemoteProcessingStatus::Rejected)?;
        }
        store.append_audit(NewAudit {
            actor: ctx.actor,
            origin: ctx.origin,
            action: "REMOTE_MESSAGE_REJECTED".into(),
            target: None,
            risk_level: RiskLevel::R0,
            capability: None,
            result: AuditResult::Denied,
            metadata: serde_json::json!({ "reason": "sender allowlist dışında" }),
        })?;
        return Ok(None);
    }

    // Replay koruması: aynı external id ikinci kez işlenmez.
    let Some(record) = store.record_remote_message(NewRemoteMessage {
        channel: msg.channel,
        external_message_id: msg.external_id.clone(),
        sender_id: msg.sender_id.clone(),
        raw_text: msg.text.chars().take(MAX_STORED_TEXT_CHARS).collect(),
        authentication_state: RemoteAuthState::Authenticated,
    })?
    else {
        return Ok(None);
    };

    let intent = crate::intent::parse(&msg.text);
    let (reply, item_id) = apply_intent(store, &ctx, msg.channel, &intent)?;
    store.finalize_remote_message(
        &record.id,
        Some(&intent),
        item_id.as_deref(),
        RemoteProcessingStatus::Processed,
    )?;
    store.append_audit(NewAudit {
        actor: ctx.actor,
        origin: ctx.origin,
        action: "REMOTE_MESSAGE_PROCESSED".into(),
        target: item_id.as_ref().map(|id| format!("task:{id}")),
        risk_level: RiskLevel::R0,
        capability: Some("CREATE_TASK".into()),
        result: AuditResult::Ok,
        metadata: serde_json::json!({ "intent": intent.kind() }),
    })?;
    Ok(Some(reply))
}

fn status_word(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::InProgress => "sürüyor",
        TaskStatus::Waiting => "bekliyor",
        TaskStatus::Done => "bitti",
        TaskStatus::Next => "sıradaki",
        TaskStatus::Planned => "planlı",
        TaskStatus::Inbox => "gelen",
        TaskStatus::Blocked => "bloklu",
        TaskStatus::Someday => "bir gün",
        TaskStatus::Cancelled => "iptal",
    }
}

/// İzinli intent'lerin uygulaması: hepsi Store'a typed yazımdır.
fn apply_intent(
    store: &Store,
    ctx: &Ctx,
    channel: RemoteChannel,
    intent: &RemoteIntent,
) -> Result<(String, Option<String>)> {
    let source = channel.task_source();
    match intent {
        RemoteIntent::CreateTask { title, description } => {
            let task = store.create_task(
                ctx,
                TaskCreate {
                    title: title.clone(),
                    description: description.clone(),
                    source: Some(source),
                    status: Some(TaskStatus::Inbox),
                    ..Default::default()
                },
            )?;
            Ok((format!("✓ Görev eklendi: {}", task.title), Some(task.id)))
        }
        RemoteIntent::CreateReminderProposal { text, requested_time } => {
            // Öneri olarak Inbox'a düşer; gerçek zamanlama lokal UI'da onaylanır.
            let mut description =
                "Uzaktan gelen hatırlatma önerisi — zamanı uygulamadan onayla.".to_string();
            if let Some(t) = requested_time {
                description.push_str(&format!(" İstenen zaman: {t}"));
            }
            let task = store.create_task(
                ctx,
                TaskCreate {
                    title: format!("⏰ {}", text.chars().take(190).collect::<String>()),
                    description: Some(description),
                    source: Some(source),
                    status: Some(TaskStatus::Inbox),
                    tags: Some(vec!["hatırlatma-önerisi".into()]),
                    ..Default::default()
                },
            )?;
            let when = requested_time.as_ref().map(|t| format!(" ({t})")).unwrap_or_default();
            Ok((
                format!("✓ Hatırlatma önerisi kaydedildi{when} — uygulamadan onayla."),
                Some(task.id),
            ))
        }
        RemoteIntent::QueryTask { query } => {
            let tasks = store.list_tasks(&TaskFilter {
                search: Some(query.clone()),
                limit: Some(5),
                ..Default::default()
            })?;
            if tasks.is_empty() {
                return Ok((format!("'{query}' ile eşleşen görev yok."), None));
            }
            let lines: Vec<String> = tasks
                .iter()
                .map(|t| format!("• {} — {}", t.title, status_word(t.status)))
                .collect();
            Ok((lines.join("\n"), None))
        }
        RemoteIntent::AddNote { text } => {
            let title: String = text.lines().next().unwrap_or(text).chars().take(120).collect();
            let task = store.create_task(
                ctx,
                TaskCreate {
                    title: format!("Not: {title}"),
                    description: Some(text.clone()),
                    source: Some(source),
                    status: Some(TaskStatus::Inbox),
                    tags: Some(vec!["not".into()]),
                    ..Default::default()
                },
            )?;
            Ok(("✓ Not kaydedildi.".into(), Some(task.id)))
        }
    }
}
