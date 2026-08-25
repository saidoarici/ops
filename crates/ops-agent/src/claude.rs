//! Claude Code CLI adapter'ı (2.x; `-p --output-format stream-json`).
//! Bayraklar kurulu CLI'ın `--help` çıktısıyla doğrulanmıştır. Prompt
//! stdin'den verilir; hiçbir shell'e gömülmez (docs/threat-model.md T8).

use std::path::{Path, PathBuf};

use ops_core::models::AgentMode;

use crate::LaunchPlan;

/// ACT modunda önceden onaylı komut aileleri (allowlist). Bunların dışındaki
/// Bash istekleri print modunda otomatik reddedilir.
const ACT_ALLOWED_BASH: &[&str] = &[
    "Bash(git *)",
    "Bash(ls *)",
    "Bash(cat *)",
    "Bash(rg *)",
    "Bash(grep *)",
    "Bash(find *)",
    "Bash(cargo *)",
    "Bash(npm *)",
    "Bash(pnpm *)",
    "Bash(node *)",
    "Bash(python3 *)",
    "Bash(pytest *)",
    "Bash(make *)",
];

/// Her modda açıkça yasak: Tam Erişim mevcut macOS kullanıcısı
/// kapsamındadır; root/sudo veya geniş kök silme yetkisi vermez.
const ALWAYS_DENIED: &str = "Bash(sudo *),Bash(su *),Bash(rm -rf /*)";

pub fn plan(
    bin: &Path,
    mode: AgentMode,
    prompt: &str,
    provider_session_id: Option<&str>,
    extra_dirs: &[String],
) -> LaunchPlan {
    let mut args: Vec<String> =
        vec!["-p".into(), "--output-format".into(), "stream-json".into(), "--verbose".into()];

    let mut assigned_session_id = None;
    match provider_session_id {
        Some(sid) => {
            args.push("--resume".into());
            args.push(sid.to_string());
        }
        None => {
            // Oturum kimliğini biz atarız → takip ve resume deterministik.
            let sid = uuid::Uuid::new_v4().to_string();
            args.push("--session-id".into());
            args.push(sid.clone());
            assigned_session_id = Some(sid);
        }
    }

    match mode {
        AgentMode::Ask => {
            args.push("--tools".into());
            args.push(String::new()); // "" = tüm araçlar kapalı
        }
        AgentMode::Read => {
            args.push("--tools".into());
            args.push("Read,Glob,Grep".into());
        }
        AgentMode::Edit => {
            args.push("--tools".into());
            args.push("Read,Glob,Grep,Edit,Write".into());
            args.push("--permission-mode".into());
            args.push("acceptEdits".into());
        }
        AgentMode::Act => {
            args.push("--permission-mode".into());
            args.push("acceptEdits".into());
            args.push("--allowedTools".into());
            args.push(ACT_ALLOWED_BASH.join(","));
        }
        AgentMode::Full => {
            args.push("--tools".into());
            args.push("default".into());
            args.push("--allow-dangerously-skip-permissions".into());
            args.push("--dangerously-skip-permissions".into());
        }
    }
    args.push("--disallowedTools".into());
    args.push(ALWAYS_DENIED.into());

    for dir in extra_dirs {
        args.push("--add-dir".into());
        args.push(dir.clone());
    }

    // Prompt STDIN'den: --tools/--disallowedTools variadic olduğundan
    // pozisyonel argüman güvenilir değil; print modu stdin'i resmi yol sayar.
    LaunchPlan {
        program: PathBuf::from(bin),
        args,
        assigned_session_id,
        stdin_payload: Some(prompt.to_string()),
    }
}

#[derive(Debug)]
pub enum Event {
    Init { session_id: String },
    Text(String),
    Tool { name: String, detail: String },
    Result { text: String, is_error: bool },
}

fn summarize_input(input: &serde_json::Value) -> String {
    for key in ["command", "file_path", "pattern", "description", "url", "query"] {
        if let Some(v) = input.get(key).and_then(|v| v.as_str()) {
            return v.chars().take(160).collect();
        }
    }
    let raw = input.to_string();
    raw.chars().take(120).collect()
}

pub fn parse(line: &str) -> Vec<Event> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    match v.get("type").and_then(|t| t.as_str()) {
        Some("system") => {
            if v.get("subtype").and_then(|s| s.as_str()) == Some("init") {
                if let Some(sid) = v.get("session_id").and_then(|s| s.as_str()) {
                    out.push(Event::Init { session_id: sid.to_string() });
                }
            }
        }
        Some("assistant") => {
            if let Some(content) = v.pointer("/message/content").and_then(|c| c.as_array()) {
                for block in content {
                    match block.get("type").and_then(|t| t.as_str()) {
                        Some("text") => {
                            if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                                if !text.trim().is_empty() {
                                    out.push(Event::Text(text.to_string()));
                                }
                            }
                        }
                        Some("tool_use") => {
                            let name = block
                                .get("name")
                                .and_then(|n| n.as_str())
                                .unwrap_or("araç")
                                .to_string();
                            let detail =
                                block.get("input").map(summarize_input).unwrap_or_default();
                            out.push(Event::Tool { name, detail });
                        }
                        _ => {}
                    }
                }
            }
        }
        Some("result") => {
            out.push(Event::Result {
                text: v.get("result").and_then(|r| r.as_str()).unwrap_or("").to_string(),
                is_error: v.get("is_error").and_then(|e| e.as_bool()).unwrap_or(false),
            });
        }
        _ => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_maps_modes_to_allowlists() {
        let bin = Path::new("/usr/local/bin/claude");
        let ask = plan(bin, AgentMode::Ask, "merhaba", None, &[]);
        assert!(ask.args.contains(&"--tools".to_string()));
        assert!(ask.args.contains(&String::new()), "ASK tüm araçları kapatmalı");
        assert!(ask.assigned_session_id.is_some());
        assert_eq!(ask.stdin_payload.as_deref(), Some("merhaba"));

        let act = plan(bin, AgentMode::Act, "test koş", Some("abc"), &[]);
        assert!(act.args.iter().any(|a| a.contains("Bash(sudo *)")), "sudo her modda yasak");
        assert!(act.args.iter().any(|a| a.contains("Bash(git *)")));
        assert!(act.args.contains(&"--resume".to_string()));
        assert!(act.assigned_session_id.is_none());
        // prompt stdin'den — argv'de yer almaz, shell interpolasyonu yok
        assert!(!act.args.iter().any(|a| a == "test koş"));
        assert_eq!(act.stdin_payload.as_deref(), Some("test koş"));
    }

    #[test]
    fn parse_stream_json_events() {
        let init = r#"{"type":"system","subtype":"init","session_id":"s-1"}"#;
        assert!(matches!(&parse(init)[0], Event::Init { session_id } if session_id == "s-1"));

        let asst = r#"{"type":"assistant","message":{"content":[
            {"type":"text","text":"Backend hazır."},
            {"type":"tool_use","name":"Bash","input":{"command":"git status"}}]}}"#;
        let events = parse(asst);
        assert!(matches!(&events[0], Event::Text(t) if t == "Backend hazır."));
        assert!(matches!(&events[1], Event::Tool { name, detail }
            if name == "Bash" && detail == "git status"));

        let result = r#"{"type":"result","result":"Bitti.","is_error":false}"#;
        assert!(
            matches!(&parse(result)[0], Event::Result { text, is_error: false } if text == "Bitti.")
        );
    }
}
