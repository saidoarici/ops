//! Codex CLI adapter'ı (tespit: 0.148, `codex exec --json`).
//! Sandbox eşlemesi: ASK/READ → read-only, EDIT/ACT → workspace-write.
//! FULL yalnızca daemon tarafında parola doğrulandıktan sonra current-user
//! kapsamında `danger-full-access` kullanır; root/sudo yetkisi verilmez.

use std::path::{Path, PathBuf};

use ops_core::models::AgentMode;

use crate::LaunchPlan;

pub fn plan(
    bin: &Path,
    mode: AgentMode,
    prompt: &str,
    provider_session_id: Option<&str>,
    extra_dirs: &[String],
) -> LaunchPlan {
    let mut args: Vec<String> =
        vec!["exec".into(), "--json".into(), "--skip-git-repo-check".into()];
    let sandbox = match mode {
        AgentMode::Ask | AgentMode::Read => "read-only",
        AgentMode::Edit | AgentMode::Act => "workspace-write",
        AgentMode::Full => "danger-full-access",
    };
    args.push("--sandbox".into());
    args.push(sandbox.into());
    if mode == AgentMode::Full {
        // Non-interactive exec'te ek onay diyaloğu oluşmasın; parola kapısı
        // daemon tarafında, provider başlatılmadan önce geçilmiştir.
        args.push("-c".into());
        args.push("approval_policy=\"never\"".into());
    }
    for dir in extra_dirs {
        args.push("--add-dir".into());
        args.push(dir.clone());
    }
    if let Some(sid) = provider_session_id {
        args.push("resume".into());
        args.push(sid.to_string());
    }
    // ASK'ta yazma zaten sandbox'ta kapalı; niyeti prompt'ta da netleştir.
    let effective_prompt = if mode == AgentMode::Ask {
        format!("(Yalnızca soruyu yanıtla; dosya değiştirme, komut çalıştırma.)\n\n{prompt}")
    } else {
        prompt.to_string()
    };
    // "-" → prompt stdin'den okunur (resmi yol; argv karışıklığı yok).
    args.push("-".into());

    LaunchPlan {
        program: PathBuf::from(bin),
        args,
        assigned_session_id: None,
        stdin_payload: Some(effective_prompt),
    }
}

#[derive(Debug)]
pub enum Event {
    Init { session_id: String },
    Text(String),
    Tool { name: String, detail: String },
    Error(String),
}

fn join_command(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Array(parts) => parts
            .iter()
            .filter_map(|p| p.as_str())
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(160)
            .collect(),
        serde_json::Value::String(s) => s.chars().take(160).collect(),
        _ => String::new(),
    }
}

/// Codex JSONL sürümler arasında şekil değiştirir; bilinen tüm şekilleri
/// savunmacı biçimde dener, tanınmayan olayları sessizce atlar.
pub fn parse(line: &str) -> Vec<Event> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let vtype = v.get("type").and_then(|t| t.as_str()).unwrap_or("");

    // Yeni stil: thread.started / item.completed
    if vtype == "thread.started" {
        if let Some(tid) = v.get("thread_id").and_then(|t| t.as_str()) {
            out.push(Event::Init { session_id: tid.to_string() });
        }
    }
    if let Some(sid) = v.get("session_id").and_then(|s| s.as_str()) {
        if vtype.contains("session") || vtype.contains("thread") {
            out.push(Event::Init { session_id: sid.to_string() });
        }
    }
    if vtype == "item.completed" {
        if let Some(item) = v.get("item") {
            let itype = item
                .get("item_type")
                .or_else(|| item.get("type"))
                .and_then(|t| t.as_str())
                .unwrap_or("");
            match itype {
                "agent_message" | "assistant_message" => {
                    if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                        out.push(Event::Text(text.to_string()));
                    }
                }
                "command_execution" => out.push(Event::Tool {
                    name: "exec".into(),
                    detail: item.get("command").map(join_command).unwrap_or_default(),
                }),
                "file_change" | "patch" => out.push(Event::Tool {
                    name: "patch".into(),
                    detail: item
                        .get("path")
                        .and_then(|p| p.as_str())
                        .unwrap_or("dosya değişikliği")
                        .into(),
                }),
                "error" => {
                    if let Some(m) = item.get("message").and_then(|t| t.as_str()) {
                        out.push(Event::Error(m.to_string()));
                    }
                }
                _ => {}
            }
        }
    }

    // Eski stil: {"msg":{"type":...}}
    if let Some(msg) = v.get("msg") {
        match msg.get("type").and_then(|t| t.as_str()).unwrap_or("") {
            "session_configured" => {
                if let Some(sid) = msg.get("session_id").and_then(|s| s.as_str()) {
                    out.push(Event::Init { session_id: sid.to_string() });
                }
            }
            "agent_message" => {
                if let Some(text) = msg.get("message").and_then(|t| t.as_str()) {
                    out.push(Event::Text(text.to_string()));
                }
            }
            "task_complete" => {
                if let Some(text) = msg.get("last_agent_message").and_then(|t| t.as_str()) {
                    if !text.trim().is_empty() {
                        out.push(Event::Text(text.to_string()));
                    }
                }
            }
            "exec_command_begin" => out.push(Event::Tool {
                name: "exec".into(),
                detail: msg.get("command").map(join_command).unwrap_or_default(),
            }),
            "patch_apply_begin" => out.push(Event::Tool {
                name: "patch".into(),
                detail: "değişiklik uygulanıyor".into(),
            }),
            "error" => {
                if let Some(m) = msg.get("message").and_then(|t| t.as_str()) {
                    out.push(Event::Error(m.to_string()));
                }
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_maps_sandbox_by_mode() {
        let bin = Path::new("/usr/local/bin/codex");
        for mode in [AgentMode::Ask, AgentMode::Read, AgentMode::Edit, AgentMode::Act] {
            let p = plan(bin, mode, "selam", None, &[]);
            assert!(!p.args.iter().any(|a| a.contains("danger-full-access")));
            assert!(!p.args.iter().any(|a| a.contains("bypass")));
        }
        let read = plan(bin, AgentMode::Read, "oku", None, &[]);
        assert!(read.args.contains(&"read-only".to_string()));
        let edit = plan(bin, AgentMode::Edit, "düzenle", Some("sid-1"), &[]);
        assert!(edit.args.contains(&"workspace-write".to_string()));
        assert!(edit.args.contains(&"resume".to_string()));
        let full = plan(bin, AgentMode::Full, "tara", None, &[]);
        assert!(full.args.contains(&"danger-full-access".to_string()));
        assert!(full.args.iter().any(|a| a.contains("approval_policy")));
        assert!(!full.args.iter().any(|a| a.contains("bypass-approvals")));
    }

    #[test]
    fn parse_both_event_styles() {
        let old = r#"{"id":"1","msg":{"type":"agent_message","message":"Merhaba"}}"#;
        assert!(matches!(&parse(old)[0], Event::Text(t) if t == "Merhaba"));
        let new =
            r#"{"type":"item.completed","item":{"item_type":"agent_message","text":"Selam"}}"#;
        assert!(matches!(&parse(new)[0], Event::Text(t) if t == "Selam"));
        let exec = r#"{"msg":{"type":"exec_command_begin","command":["git","status"]}}"#;
        assert!(matches!(&parse(exec)[0], Event::Tool { detail, .. } if detail == "git status"));
        let started = r#"{"type":"thread.started","thread_id":"th-9"}"#;
        assert!(matches!(&parse(started)[0], Event::Init { session_id } if session_id == "th-9"));
    }
}
