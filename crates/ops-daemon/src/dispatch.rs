//! UDS isteklerinin metod yönlendiricisi. Lokal socket (0600) üzerinden gelen
//! her istek lokal kullanıcı bağlamıyla (`Ctx::LOCAL_USER`) çalışır.
//! Her metod typed bir parametre şemasından geçer; ham string yürütme yoktur.

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};

use ops_core::ipc::{
    AgentDetectParams, AgentMessagesParams, AuditListParams, DetectedListParams,
    FullAccessConfigureParams, IdParams, LimitParams, ProjectListParams, ProjectUpdateParams,
    ReminderUpdateParams, RoutineUpdateParams, SettingsSetParams, TaskUpdateParams,
    TelegramConfigureParams, TodayParams, WhatsAppConfigureParams,
};
use ops_core::models::{
    AgentChatRequest, AgentMode, AuditResult, Ctx, EvidenceFilter, NewAudit, ProjectCreate,
    ReminderCreate, ReminderFilter, RiskLevel, TaskCreate, TaskFilter,
};
use ops_core::{paths, time, today, OpsError};

use crate::AppState;

type Result<T> = std::result::Result<T, OpsError>;

pub async fn dispatch(state: &AppState, method: &str, params: Value) -> Result<Value> {
    let ctx = Ctx::LOCAL_USER;
    match method {
        "health.check" => {
            state.store.ping()?;
            Ok(json!({
                "ok": true,
                "version": env!("CARGO_PKG_VERSION"),
                "uptimeSecs": state.started_at.elapsed().as_secs(),
                "dataDir": paths::data_dir().display().to_string(),
                "socketPath": paths::socket_path().display().to_string(),
                "time": time::to_db(&time::now()),
            }))
        }

        "today.view" => {
            let p: TodayParams = parse_opt(params)?;
            let offset = match p.utc_offset_minutes {
                Some(m) => chrono::FixedOffset::east_opt(m * 60).ok_or_else(|| {
                    OpsError::Validation(format!("geçersiz utcOffsetMinutes: {m}"))
                })?,
                None => *chrono::Local::now().offset(),
            };
            to_value(today::build(&state.store, time::now(), offset)?)
        }

        // tasks
        "task.create" => {
            let p: TaskCreate = parse(params)?;
            to_value(state.store.create_task(&ctx, p)?)
        }
        "task.get" => {
            let p: IdParams = parse(params)?;
            to_value(state.store.get_task(&p.id)?)
        }
        "task.list" => {
            let p: TaskFilter = parse_opt(params)?;
            to_value(state.store.list_tasks(&p)?)
        }
        "task.update" => {
            let p: TaskUpdateParams = parse(params)?;
            to_value(state.store.update_task(&ctx, &p.id, p.patch)?)
        }
        "task.complete" => {
            let p: IdParams = parse(params)?;
            to_value(state.store.complete_task(&ctx, &p.id)?)
        }
        "task.archive" => {
            let p: IdParams = parse(params)?;
            to_value(state.store.archive_task(&ctx, &p.id)?)
        }

        // projects — yol değişince observer watcher'ları hemen tazelenir
        "project.create" => {
            let p: ProjectCreate = parse(params)?;
            let created = state.store.create_project(&ctx, p)?;
            state.observer.refresh();
            to_value(created)
        }
        "project.get" => {
            let p: IdParams = parse(params)?;
            to_value(state.store.get_project(&p.id)?)
        }
        "project.list" => {
            let p: ProjectListParams = parse_opt(params)?;
            to_value(state.store.list_projects(p.include_archived)?)
        }
        "project.update" => {
            let p: ProjectUpdateParams = parse(params)?;
            let updated = state.store.update_project(&ctx, &p.id, p.patch)?;
            state.observer.refresh();
            to_value(updated)
        }
        "project.archive" => {
            let p: IdParams = parse(params)?;
            let archived = state.store.archive_project(&ctx, &p.id)?;
            state.observer.refresh();
            to_value(archived)
        }
        "project.overview" => {
            let p: IdParams = parse(params)?;
            let project = state.store.get_project(&p.id)?;
            let repo_states = state.store.list_repo_states(&p.id)?;
            let evidence = state.store.list_evidence(&EvidenceFilter {
                project_id: Some(p.id.clone()),
                limit: Some(30),
                ..Default::default()
            })?;
            let detected = state.store.list_detected_for_project(&p.id)?;
            Ok(json!({
                "project": project,
                "repoStates": repo_states,
                "evidence": evidence,
                "detected": detected,
            }))
        }

        // reminders
        "reminder.create" => {
            let p: ReminderCreate = parse(params)?;
            to_value(state.store.create_reminder(&ctx, p)?)
        }
        "reminder.list" => {
            let p: ReminderFilter = parse_opt(params)?;
            to_value(state.store.list_reminders(&p)?)
        }
        "reminder.update" => {
            let p: ReminderUpdateParams = parse(params)?;
            to_value(state.store.update_reminder(&ctx, &p.id, p.patch)?)
        }
        "reminder.dismiss" => {
            let p: IdParams = parse(params)?;
            to_value(state.store.dismiss_reminder(&ctx, &p.id)?)
        }

        // observer
        "evidence.list" => {
            let p: EvidenceFilter = parse_opt(params)?;
            to_value(state.store.list_evidence(&p)?)
        }
        "detected.list" => {
            let p: DetectedListParams = parse_opt(params)?;
            to_value(state.store.list_detected(p.include_closed)?)
        }
        "detected.dismiss" => {
            let p: IdParams = parse(params)?;
            to_value(state.store.dismiss_detected(&ctx, &p.id)?)
        }
        "detected.convert" => {
            let p: IdParams = parse(params)?;
            to_value(state.store.convert_detected(&ctx, &p.id)?)
        }
        "observer.status" => to_value(state.observer.status()),
        "observer.scan" => {
            state.observer.refresh();
            let summary = state.observer.scan_all();
            state.store.append_audit(NewAudit {
                actor: ctx.actor,
                origin: ctx.origin,
                action: "OBSERVER_SCAN".into(),
                target: None,
                risk_level: RiskLevel::R0,
                capability: Some("READ_GIT_METADATA".into()),
                result: AuditResult::Ok,
                metadata: json!({
                    "projects": summary.projects,
                    "evidence": summary.evidence_added,
                }),
            })?;
            to_value(summary)
        }

        // agent
        "agent.detect" => {
            let p: AgentDetectParams = parse_opt(params)?;
            to_value(state.agent.detect(p.force).await)
        }
        "agent.chat" => {
            let mut p: AgentChatRequest = parse(params)?;
            // Parola isteğin geri kalanından ayrılır; yalnızca doğrulamada kullanılır.
            let password = p.full_access_password.take();
            let full_session = match &p.session_id {
                Some(id) => state.store.get_agent_session(id)?.mode == AgentMode::Full,
                None => p.mode == Some(AgentMode::Full),
            };
            let authorized =
                if full_session && !state.agent.full_access_is_unlocked(p.session_id.as_deref()) {
                    let password = password.as_deref().ok_or_else(|| {
                        OpsError::Security(
                            "Tam Erişim oturumu kilitli; yerel parolanı yeniden gir".into(),
                        )
                    })?;
                    state.full_access.verify(password).await?;
                    true
                } else {
                    false
                };
            to_value(state.agent.chat(p, authorized).await?)
        }
        "agent.sessions" => {
            let p: LimitParams = parse_opt(params)?;
            to_value(state.store.list_agent_sessions(p.limit.unwrap_or(30))?)
        }
        "agent.session" => {
            let p: IdParams = parse(params)?;
            to_value(state.store.get_agent_session(&p.id)?)
        }
        "agent.messages" => {
            let p: AgentMessagesParams = parse(params)?;
            to_value(state.store.list_agent_messages(&p.session_id, p.after_seq)?)
        }
        "agent.cancel" => {
            let p: IdParams = parse(params)?;
            Ok(json!({ "cancelled": state.agent.cancel(&p.id) }))
        }
        "agent.fullAccess.status" => Ok(json!({
            "configured": state.full_access.configured().await?,
            "unlockMinutes": ops_agent::FULL_ACCESS_IDLE_MINUTES,
        })),
        "agent.fullAccess.configure" => {
            let p: FullAccessConfigureParams = parse(params)?;
            state.full_access.configure(p.new_password, p.current_password).await?;
            state.store.append_audit(NewAudit {
                actor: ctx.actor,
                origin: ctx.origin,
                action: "FULL_ACCESS_PASSWORD_SET".into(),
                target: None,
                risk_level: RiskLevel::R4,
                capability: Some("FULL_LOCAL_ACCESS".into()),
                result: AuditResult::Ok,
                metadata: json!({ "stored": "argon2-keychain" }),
            })?;
            Ok(json!({
                "configured": true,
                "unlockMinutes": ops_agent::FULL_ACCESS_IDLE_MINUTES,
            }))
        }
        "agent.fullAccess.lock" => {
            let p: IdParams = parse(params)?;
            state.agent.lock_full_access(&p.id);
            state.store.append_audit(NewAudit {
                actor: ctx.actor,
                origin: ctx.origin,
                action: "FULL_ACCESS_SESSION_LOCK".into(),
                target: Some(format!("session:{}", p.id)),
                risk_level: RiskLevel::R4,
                capability: Some("FULL_LOCAL_ACCESS".into()),
                result: AuditResult::Ok,
                metadata: json!({}),
            })?;
            Ok(json!({ "locked": true }))
        }

        // routines
        "routine.list" => to_value(state.store.list_routines()?),
        "routine.update" => {
            let p: RoutineUpdateParams = parse(params)?;
            to_value(state.store.update_routine(&ctx, &p.id, p.patch)?)
        }
        "routine.run" => {
            let p: IdParams = parse(params)?;
            let text = crate::routines::run_routine_now(state, &p.id).await?;
            Ok(json!({ "text": text }))
        }

        // remote channels — yapılandırma yalnızca bu lokal yüzeyden değişir
        "remote.status" => to_value(state.remote.status().await),
        "remote.telegram.configure" => {
            let p: TelegramConfigureParams = parse(params)?;
            let bot = state
                .remote
                .configure_telegram(&p.token, &p.allowed_user_id, &p.allowed_chat_id)
                .await?;
            Ok(json!({ "botName": bot }))
        }
        "remote.telegram.disable" => {
            state.remote.disable_telegram().await?;
            Ok(json!({ "ok": true }))
        }
        "remote.telegram.test" => {
            let bot = state.remote.test_telegram().await?;
            Ok(json!({ "botName": bot }))
        }
        "remote.whatsapp.configure" => {
            let p: WhatsAppConfigureParams = parse(params)?;
            let status =
                state.remote.configure_whatsapp(&p.base_url, &p.api_key, &p.phone_number).await?;
            Ok(json!({ "status": status }))
        }
        "remote.whatsapp.disable" => {
            state.remote.disable_whatsapp().await?;
            Ok(json!({ "ok": true }))
        }
        "remote.whatsapp.test" => {
            let status = state.remote.test_whatsapp().await?;
            Ok(json!({ "status": status }))
        }
        "remote.messages" => {
            let p: LimitParams = parse_opt(params)?;
            to_value(state.store.list_remote_messages(p.limit.unwrap_or(50))?)
        }

        // audit
        "audit.list" => {
            let p: AuditListParams = parse_opt(params)?;
            to_value(state.store.list_audit(p.limit.unwrap_or(100), p.before_seq)?)
        }
        "audit.verify" => to_value(state.store.verify_audit()?),

        // settings
        "settings.get" => to_value(state.store.get_settings()?),
        "settings.set" => {
            let p: SettingsSetParams = parse(params)?;
            state.store.set_setting(&ctx, &p.key, p.value)?;
            to_value(state.store.get_settings()?)
        }

        // data
        "data.backup" => to_value(state.store.backup_to(&ctx, &paths::backups_dir())?),
        "data.backups" => to_value(state.store.list_backups(&paths::backups_dir())?),

        other => Err(OpsError::UnknownMethod(other.to_string())),
    }
}

fn parse<T: DeserializeOwned>(v: Value) -> Result<T> {
    serde_json::from_value(v).map_err(|e| OpsError::Validation(format!("parametre hatası: {e}")))
}

/// Parametresi opsiyonel metodlar: `params` yok/null ise default kullanılır.
fn parse_opt<T: DeserializeOwned + Default>(v: Value) -> Result<T> {
    if v.is_null() {
        Ok(T::default())
    } else {
        parse(v)
    }
}

fn to_value<T: Serialize>(t: T) -> Result<Value> {
    serde_json::to_value(t).map_err(OpsError::from)
}
