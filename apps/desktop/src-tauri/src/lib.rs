//! Tauri köprüsü: ince bir UDS proxy'sidir. İş mantığı daemon'dadır;
//! UI process'i yalnızca istek taşır (docs/architecture.md, "Desktop shell").

mod uds;

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ErrPayload {
    pub code: String,
    pub message: String,
}

/// Tek geçit: method + params → daemon → result. Typed istemci TS tarafındadır.
#[tauri::command]
async fn ops_call(method: String, params: Option<Value>) -> Result<Value, ErrPayload> {
    uds::call(&method, params.unwrap_or(Value::Null)).await
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DaemonStatus {
    connected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    health: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    socket_path: String,
}

#[tauri::command]
async fn daemon_status() -> DaemonStatus {
    let socket_path = ops_core::paths::socket_path().display().to_string();
    match uds::call("health.check", Value::Null).await {
        Ok(health) => {
            DaemonStatus { connected: true, health: Some(health), error: None, socket_path }
        }
        Err(e) => {
            DaemonStatus { connected: false, health: None, error: Some(e.message), socket_path }
        }
    }
}

/// Geliştirme kolaylığı: debug build'de, aynı target dizininde derlenmiş
/// personal-opsd varsa başlat. Production'da esas mekanizma launchd'dir.
#[tauri::command]
fn start_daemon_dev() -> Result<bool, ErrPayload> {
    if !cfg!(debug_assertions) {
        return Ok(false);
    }
    let io_err = |e: std::io::Error| ErrPayload { code: "IO".into(), message: e.to_string() };
    let exe = std::env::current_exe().map_err(io_err)?;
    let Some(daemon) = exe.parent().map(|dir| dir.join("personal-opsd")) else {
        return Ok(false);
    };
    if !daemon.exists() {
        return Ok(false);
    }
    std::process::Command::new(daemon)
        .arg("run")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(io_err)?;
    Ok(true)
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = tauri::Manager::get_webview_window(app, "main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .invoke_handler(tauri::generate_handler![ops_call, daemon_status, start_daemon_dev])
        .setup(|app| {
            use tauri::Emitter;

            // ⌥Space — hızlı yakalama: pencereyi öne getir + paleti "capture"
            // modunda aç. Kısayol başka uygulamada kayıtlıysa uygulama yine çalışır.
            {
                use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};
                let handle = app.handle().clone();
                if let Ok(shortcut) = "Alt+Space".parse::<Shortcut>() {
                    let result =
                        app.global_shortcut().on_shortcut(shortcut, move |_app, _s, event| {
                            if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                                show_main_window(&handle);
                                let _ = handle.emit("quick-capture", ());
                            }
                        });
                    if let Err(e) = result {
                        tracing::warn!("global kısayol kaydedilemedi: {e}");
                    }
                }
            }

            // Menü çubuğu simgesi: hızlı erişim + hızlı görev.
            {
                use tauri::menu::{MenuBuilder, MenuItemBuilder};
                use tauri::tray::TrayIconBuilder;

                let open_item = MenuItemBuilder::with_id("open", "Personal Ops'u Aç").build(app)?;
                let capture_item =
                    MenuItemBuilder::with_id("capture", "Hızlı Görev  ⌥Space").build(app)?;
                let quit_item = MenuItemBuilder::with_id("quit", "Çıkış").build(app)?;
                let menu = MenuBuilder::new(app)
                    .item(&open_item)
                    .item(&capture_item)
                    .separator()
                    .item(&quit_item)
                    .build()?;

                let mut tray = TrayIconBuilder::with_id("main-tray")
                    .menu(&menu)
                    .show_menu_on_left_click(true)
                    .on_menu_event(|app, event| match event.id().as_ref() {
                        "open" => show_main_window(app),
                        "capture" => {
                            show_main_window(app);
                            let _ = app.emit("quick-capture", ());
                        }
                        "quit" => app.exit(0),
                        _ => {}
                    });
                if let Some(icon) = app.default_window_icon() {
                    tray = tray.icon(icon.clone()).icon_as_template(false);
                }
                tray.build(app)?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Tauri uygulaması başlatılamadı");
}
