//! Dispatch düzeyinde yetki sınırları: riskli agent modları lokal onay/parola
//! olmadan açılamaz; secret'lar ayarlara yazılamaz; hatalı girdiler ağa ya da
//! Keychain'e ulaşmadan reddedilir. Bu testler ağ ve Keychain'e dokunmaz.

use ops_core::store::Store;
use ops_core::OpsError;
use ops_daemon::dispatch::dispatch;
use ops_daemon::AppState;
use serde_json::json;

fn state() -> std::sync::Arc<AppState> {
    AppState::new(Store::in_memory().unwrap())
}

#[tokio::test]
async fn act_mode_requires_explicit_local_confirmation() {
    let st = state();
    let denied = dispatch(
        &st,
        "agent.chat",
        json!({ "provider": "CLAUDE", "mode": "ACT", "prompt": "cargo test çalıştır" }),
    )
    .await;
    assert!(matches!(denied, Err(OpsError::Security(_))), "onaysız ACT: {denied:?}");
    // Oturum bile açılmamış olmalı
    let sessions = dispatch(&st, "agent.sessions", json!({})).await.unwrap();
    assert_eq!(sessions.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn full_mode_requires_password_before_anything_else() {
    let st = state();
    let denied = dispatch(
        &st,
        "agent.chat",
        json!({ "provider": "CODEX", "mode": "FULL", "prompt": "diski tara" }),
    )
    .await;
    assert!(matches!(denied, Err(OpsError::Security(_))), "parolasız FULL: {denied:?}");
    let sessions = dispatch(&st, "agent.sessions", json!({})).await.unwrap();
    assert_eq!(sessions.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn chat_request_rejects_unknown_fields() {
    let st = state();
    let err = dispatch(
        &st,
        "agent.chat",
        json!({ "provider": "CLAUDE", "prompt": "selam", "shell": "rm -rf /" }),
    )
    .await;
    assert!(matches!(err, Err(OpsError::Validation(_))));
}

#[tokio::test]
async fn secrets_never_land_in_settings() {
    let st = state();
    for key in ["telegram_token", "whatsapp_api_key", "full_access_password"] {
        let err = dispatch(&st, "settings.set", json!({ "key": key, "value": "x" })).await;
        assert!(matches!(err, Err(OpsError::Security(_))), "{key}: {err:?}");
    }
    let err = dispatch(&st, "settings.set", json!({ "key": "shell_hook", "value": "x" })).await;
    assert!(matches!(err, Err(OpsError::Validation(_))));
    let ok = dispatch(&st, "settings.set", json!({ "key": "display_name", "value": "Demo" }))
        .await
        .unwrap();
    assert_eq!(ok["display_name"], "Demo");
}

#[tokio::test]
async fn malformed_remote_configuration_is_rejected_offline() {
    let st = state();
    // Token biçimi bozuk → ağ isteği yapılmadan reddedilir.
    let tg = dispatch(
        &st,
        "remote.telegram.configure",
        json!({ "token": "not-a-token", "allowedUserId": "1", "allowedChatId": "2" }),
    )
    .await;
    assert!(matches!(tg, Err(OpsError::Validation(_))), "{tg:?}");
    // http:// uzak sunucu → API anahtarı düz metin gitmez; Keychain'e yazılmadan reddedilir.
    let wa = dispatch(
        &st,
        "remote.whatsapp.configure",
        json!({ "baseUrl": "http://bot.example.com", "apiKey": "k", "phoneNumber": "905551234567" }),
    )
    .await;
    assert!(matches!(wa, Err(OpsError::Validation(_))), "{wa:?}");
    let status = dispatch(&st, "remote.status", json!({})).await.unwrap();
    assert_eq!(status["whatsapp"]["configured"], false);
}

#[tokio::test]
async fn unknown_methods_and_bad_params_are_typed_errors() {
    let st = state();
    assert!(matches!(
        dispatch(&st, "shell.exec", json!({ "cmd": "id" })).await,
        Err(OpsError::UnknownMethod(_))
    ));
    assert!(matches!(
        dispatch(&st, "task.create", json!({ "title": "" })).await,
        Err(OpsError::Validation(_))
    ));
    assert!(matches!(
        dispatch(&st, "task.get", json!({ "id": "yok" })).await,
        Err(OpsError::NotFound(_))
    ));
}

#[tokio::test]
async fn fresh_state_serves_health_today_and_routines() {
    let st = state();
    st.store.ensure_builtin_routines().unwrap();
    let health = dispatch(&st, "health.check", json!({})).await.unwrap();
    assert_eq!(health["ok"], true);
    let today = dispatch(&st, "today.view", json!({ "utcOffsetMinutes": 180 })).await.unwrap();
    assert_eq!(today["stats"]["openTasks"], 0);
    let routines = dispatch(&st, "routine.list", json!({})).await.unwrap();
    assert_eq!(routines.as_array().unwrap().len(), 3);
    let observer = dispatch(&st, "observer.status", json!({})).await.unwrap();
    assert_eq!(observer["running"], false, "arka plan görevi başlatılmadı");
}
