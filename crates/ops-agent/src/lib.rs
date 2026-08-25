//! ops-agent — Claude Code / Codex CLI orkestrasyonu.
//!
//! İlkeler:
//! - Kullanıcının kurulu resmi CLI'ları çalıştırılır; API key zorunlu değildir,
//!   OAuth credential'ları asla okunmaz/kopyalanmaz.
//! - Mod → capability eşlemesi allowlist'tir; FULL yalnızca yerel parola
//!   kapısından sonra current-user kapsamında açılır; sudo/root verilmez.
//! - Prompt stdin'den verilir; shell interpolasyonu yoktur.
//! - Sandbox hijyeni: proje köküne sabit cwd, minimal env, timeout, çıktı sınırı.

pub mod claude;
pub mod codex;
pub mod detect;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::sync::oneshot;
use tokio::time::{sleep_until, Duration, Instant};
use tracing::{info, warn};

use ops_core::models::{
    AgentChatRequest, AgentDetectReport, AgentMessageRole, AgentMode, AgentProviderKind,
    AgentSession, AgentSessionStatus, Ctx, TaskCreate, TaskSource, TaskStatus,
};
use ops_core::store::Store;
use ops_core::{paths, time, OpsError};

const SESSION_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const OUTPUT_CAP_BYTES: usize = 8 * 1024 * 1024;
const DETECT_CACHE_SECS: i64 = 300;
pub const FULL_ACCESS_IDLE_MINUTES: u64 = 30;
const FULL_ACCESS_IDLE_TIMEOUT: Duration = Duration::from_secs(FULL_ACCESS_IDLE_MINUTES * 60);

#[derive(Debug, Clone)]
pub struct LaunchPlan {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub assigned_session_id: Option<String>,
    /// Doluysa prompt stdin'den verilir (variadic bayrakların pozisyonel
    /// argümanı yutmasına karşı güvenli yol; shell'e asla gömülmez).
    pub stdin_payload: Option<String>,
}

/// Agent process'ine geçen minimal ortam: kimlik/konfig için HOME,
/// binary çözümü için sınırlı PATH — başka hiçbir şey miras alınmaz.
pub fn minimal_env(program: &Path) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = Vec::new();
    for key in ["HOME", "USER", "LOGNAME", "TMPDIR"] {
        if let Ok(v) = std::env::var(key) {
            env.push((key.to_string(), v));
        }
    }
    let bin_dir = program.parent().map(|p| p.display().to_string()).unwrap_or_default();
    let cargo_bin =
        dirs::home_dir().map(|p| p.join(".cargo/bin").display().to_string()).unwrap_or_default();
    env.push((
        "PATH".into(),
        format!(
            "{bin_dir}:{cargo_bin}:/usr/bin:/bin:/usr/sbin:/sbin:/opt/homebrew/bin:/usr/local/bin"
        ),
    ));
    env.push(("LANG".into(), "en_US.UTF-8".into()));
    env.push(("TERM".into(), "dumb".into()));
    env
}

struct Outcome {
    status: AgentSessionStatus,
    summary: Option<String>,
}

pub struct AgentManager {
    store: Store,
    cancels: Mutex<HashMap<String, oneshot::Sender<()>>>,
    detect_cache: tokio::sync::Mutex<Option<AgentDetectReport>>,
    /// Parola doğrulanmış FULL oturumları yalnızca daemon belleğinde tutulur.
    full_unlocks: Mutex<HashMap<String, Instant>>,
}

impl AgentManager {
    pub fn new(store: Store) -> Arc<Self> {
        Arc::new(Self {
            store,
            cancels: Mutex::new(HashMap::new()),
            detect_cache: tokio::sync::Mutex::new(None),
            full_unlocks: Mutex::new(HashMap::new()),
        })
    }

    pub fn full_access_is_unlocked(&self, session_id: Option<&str>) -> bool {
        let Some(id) = session_id else { return false };
        let mut unlocks = self.full_unlocks.lock().unwrap_or_else(|e| e.into_inner());
        let valid = unlocks.get(id).is_some_and(|last| last.elapsed() < FULL_ACCESS_IDLE_TIMEOUT);
        if valid {
            unlocks.insert(id.to_string(), Instant::now());
        } else {
            unlocks.remove(id);
        }
        valid
    }

    pub fn lock_full_access(&self, session_id: &str) {
        self.full_unlocks.lock().unwrap_or_else(|e| e.into_inner()).remove(session_id);
        let _ = self.cancel(session_id);
    }

    pub async fn detect(&self, force: bool) -> AgentDetectReport {
        let mut cache = self.detect_cache.lock().await;
        if !force {
            if let Some(cached) = &*cache {
                if (time::now() - cached.checked_at).num_seconds() < DETECT_CACHE_SECS {
                    return cached.clone();
                }
            }
        }
        let report = detect::detect_all().await;
        *cache = Some(report.clone());
        report
    }

    /// Chat turu başlatır: oturumu açar/sürdürür, kullanıcı mesajını yazar ve
    /// CLI'yı arka planda koşturur. UI mesajları `agent.messages` ile poll eder.
    pub async fn chat(
        self: &Arc<Self>,
        req: AgentChatRequest,
        full_access_authorized: bool,
    ) -> Result<AgentSession, OpsError> {
        let ctx = Ctx::LOCAL_USER;
        let prompt = req.prompt.trim().to_string();
        if prompt.is_empty() {
            return Err(OpsError::Validation("mesaj boş olamaz".into()));
        }
        if prompt.chars().count() > 20_000 {
            return Err(OpsError::Validation("mesaj çok uzun (max 20.000 karakter)".into()));
        }

        let session = match &req.session_id {
            Some(id) => {
                let s = self.store.get_agent_session(id)?;
                if s.mode == AgentMode::Full
                    && !full_access_authorized
                    && !self.full_access_is_unlocked(Some(id))
                {
                    return Err(OpsError::Security(
                        "Tam Erişim oturumu kilitli; yerel parolanı yeniden gir".into(),
                    ));
                }
                if s.status == AgentSessionStatus::Running {
                    return Err(OpsError::Conflict(
                        "oturum zaten çalışıyor; bitmesini bekle".into(),
                    ));
                }
                self.store.mark_agent_session_running(id)?;
                self.store.get_agent_session(id)?
            }
            None => {
                let provider = req
                    .provider
                    .ok_or_else(|| OpsError::Validation("yeni oturum için provider seç".into()))?;
                let mode = req.mode.unwrap_or(AgentMode::Ask);
                if mode == AgentMode::Full && !full_access_authorized {
                    return Err(OpsError::Security(
                        "Tam Erişim modu yerel parola onayı gerektirir".into(),
                    ));
                }
                if mode == AgentMode::Act && !req.confirm_act {
                    // ACT yalnızca lokal UI'daki açık onay kutusuyla açılır.
                    return Err(OpsError::Security(
                        "ACT modu yalnızca lokal onay kutusuyla açılabilir".into(),
                    ));
                }
                let (project_id, workdir) =
                    resolve_workdir(&self.store, mode, req.project_id.clone())?;
                let title: String = prompt.chars().take(60).collect();
                self.store.create_agent_session(
                    &ctx,
                    provider,
                    project_id,
                    mode,
                    Some(workdir.display().to_string()),
                    &title,
                )?
            }
        };

        if session.mode == AgentMode::Full {
            self.full_unlocks
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(session.id.clone(), Instant::now());
        }

        if let Some(done) = self.run_typed_command(&ctx, &session, &prompt)? {
            return Ok(done);
        }

        let report = self.detect(false).await;
        let info = match session.provider {
            AgentProviderKind::Claude => &report.claude,
            AgentProviderKind::Codex => &report.codex,
        };
        let Some(bin) = info.installed.then_some(()).and(info.path.clone()) else {
            let name = session.provider.as_ref().to_lowercase();
            self.store.append_agent_message(
                &session.id,
                AgentMessageRole::Error,
                &format!("{name} CLI bulunamadı. Kurulum sonrası Ayarlar'dan yeniden tara."),
                None,
            )?;
            self.store.finish_agent_session(&session.id, AgentSessionStatus::Failed, None)?;
            return Err(OpsError::Validation(format!("{name} kurulu değil")));
        };

        self.store.append_agent_message(&session.id, AgentMessageRole::User, &prompt, None)?;

        let extra_dirs = extra_project_dirs(&self.store, &session);
        let effective_prompt = assistant_prompt(&self.store, &session, &prompt);
        let plan = match session.provider {
            AgentProviderKind::Claude => claude::plan(
                Path::new(&bin),
                session.mode,
                &effective_prompt,
                session.provider_session_id.as_deref(),
                &extra_dirs,
            ),
            AgentProviderKind::Codex => codex::plan(
                Path::new(&bin),
                session.mode,
                &effective_prompt,
                session.provider_session_id.as_deref(),
                &extra_dirs,
            ),
        };
        if let Some(sid) = &plan.assigned_session_id {
            self.store.set_agent_provider_session(&session.id, sid)?;
        }

        let (cancel_tx, cancel_rx) = oneshot::channel();
        self.cancels
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(session.id.clone(), cancel_tx);

        let mgr = self.clone();
        let spawned = session.clone();
        tokio::spawn(async move {
            let sid = spawned.id.clone();
            let outcome = drive(&mgr, &spawned, plan, cancel_rx).await;
            mgr.cancels.lock().unwrap_or_else(|e| e.into_inner()).remove(&sid);
            let (status, summary) = match outcome {
                Ok(o) => (o.status, o.summary),
                Err(e) => {
                    let _ = mgr.store.append_agent_message(
                        &sid,
                        AgentMessageRole::Error,
                        &e.to_string(),
                        None,
                    );
                    (AgentSessionStatus::Failed, Some(e.to_string()))
                }
            };
            if let Err(e) = mgr.store.finish_agent_session(&sid, status, summary.as_deref()) {
                warn!(error = %e, "oturum kapatılamadı");
            }
        });

        self.store.get_agent_session(&session.id)
    }

    /// Typed chat aksiyonları: "/görev <başlık>" provider'a gitmeden doğrudan
    /// görev oluşturur. Bu bir shell komutu değil, uygulama içi aksiyondur.
    /// Eşleşme yoksa `None` döner ve prompt normal yoldan CLI'ya gider.
    fn run_typed_command(
        &self,
        ctx: &Ctx,
        session: &AgentSession,
        prompt: &str,
    ) -> Result<Option<AgentSession>, OpsError> {
        let Some(rest) = prompt.strip_prefix("/görev ").or_else(|| prompt.strip_prefix("/task "))
        else {
            return Ok(None);
        };
        self.store.append_agent_message(&session.id, AgentMessageRole::User, prompt, None)?;
        let task = self.store.create_task(
            ctx,
            TaskCreate {
                title: rest.trim().to_string(),
                project_id: session.project_id.clone(),
                source: Some(TaskSource::AgentChat),
                status: Some(TaskStatus::Next),
                ..Default::default()
            },
        )?;
        self.store.append_agent_message(
            &session.id,
            AgentMessageRole::System,
            &format!("✓ Görev oluşturuldu: {}", task.title),
            None,
        )?;
        let finished = self.store.finish_agent_session(
            &session.id,
            AgentSessionStatus::Completed,
            session.summary.as_deref(),
        )?;
        Ok(Some(finished))
    }

    pub fn cancel(&self, session_id: &str) -> bool {
        if let Some(tx) = self.cancels.lock().unwrap_or_else(|e| e.into_inner()).remove(session_id)
        {
            let _ = tx.send(());
            true
        } else {
            false
        }
    }
}

/// Çalışma dizini güvenliği: EDIT/ACT/READ yalnızca projenin ONAYLI ve var olan
/// kökünde koşar. FULL parola sonrası kullanıcının home dizininde başlar.
fn resolve_workdir(
    store: &Store,
    mode: AgentMode,
    project_id: Option<String>,
) -> Result<(Option<String>, PathBuf), OpsError> {
    match project_id {
        Some(pid) => {
            let project = store.get_project(&pid)?;
            let root = project
                .local_paths
                .iter()
                .find_map(|lp| std::fs::canonicalize(lp).ok().filter(|c| c.is_dir()));
            match root {
                Some(r) => Ok((Some(pid), r)),
                None if mode == AgentMode::Ask => Ok((Some(pid), paths::data_dir())),
                None => Err(OpsError::Validation(
                    "bu modda çalışmak için projeye geçerli bir yerel klasör ekle".into(),
                )),
            }
        }
        None if mode == AgentMode::Ask => Ok((None, paths::data_dir())),
        None if mode == AgentMode::Full => {
            Ok((None, dirs::home_dir().unwrap_or_else(paths::data_dir)))
        }
        None => Err(OpsError::Validation("READ/EDIT/ACT modları için proje seç".into())),
    }
}

fn assistant_prompt(store: &Store, session: &AgentSession, user_prompt: &str) -> String {
    use ops_core::models::TaskFilter;

    let display_name = store
        .get_settings()
        .ok()
        .and_then(|s| s.get("display_name").and_then(|v| v.as_str()).map(str::to_owned))
        .unwrap_or_else(|| "kullanıcı".into());
    let projects = store
        .list_projects(false)
        .unwrap_or_default()
        .into_iter()
        .take(20)
        .map(|p| {
            format!(
                "- {} [{}] yollar={}",
                p.project.name,
                p.project.id,
                p.project.local_paths.join(", ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let tasks = store
        .list_tasks(&TaskFilter { limit: Some(30), ..Default::default() })
        .unwrap_or_default()
        .into_iter()
        .filter(|t| t.status.is_open())
        .take(20)
        .map(|t| {
            format!(
                "- {} [{}] durum={} proje={}",
                t.title,
                t.id,
                t.status.as_ref(),
                t.project_name.unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let opsctl = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("personal-opsctl")))
        .filter(|p| p.exists())
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "personal-opsctl".into());

    format!(
        r#"[PERSONAL OPS YEREL ASİSTAN BAĞLAMI]
Sen Personal Ops masaüstü uygulamasının yerleşik asistanısın. Kullanıcının adı {display_name}.
"görev uygulaması", "uygulama" veya "buraya ekle" denince Personal Ops kastedilir; Todoist/Trello gibi başka bir uygulama sorma.
Oturum modu: {mode}. Çalışma dizini: {workdir}.
Personal Ops verisini doğrudan SQLite yazarak değiştirme. Bunun yerine `{opsctl}` aracını kullan.
Kullanılabilir komutlar: `context`, `project list`, `project add --name <ad> --path <yol>`, `task list`, `task add --title <başlık> [--project-id <id>]`, `task complete --id <id>`.
Tam Erişim modunda macOS kullanıcı hesabının izinleriyle çalışırsın. Parola/credential arama, okuma veya gösterme; sudo/root kullanma; geniş ve geri döndürülemez silme yapma.
Bir iş mevcut modda yapılamıyorsa kullanıcıdan kaldıramayacağı "talimatları kaldırmasını" isteme; gerekli Personal Ops modunu ve yeni oturum gerekip gerekmediğini net söyle.

Mevcut projeler:
{projects}

Mevcut açık görevlerden bir kesit:
{tasks}

[KULLANICI MESAJI]
{user_prompt}"#,
        mode = session.mode.as_ref(),
        workdir = session.working_directory.as_deref().unwrap_or("bilinmiyor"),
    )
}

fn extra_project_dirs(store: &Store, session: &AgentSession) -> Vec<String> {
    let Some(pid) = &session.project_id else { return Vec::new() };
    let Ok(project) = store.get_project(pid) else { return Vec::new() };
    let wd = session.working_directory.clone().unwrap_or_default();
    project
        .local_paths
        .iter()
        .filter_map(|lp| std::fs::canonicalize(lp).ok())
        .filter(|c| c.is_dir())
        .map(|c| c.display().to_string())
        .filter(|c| *c != wd)
        .collect()
}

async fn drive(
    mgr: &Arc<AgentManager>,
    session: &AgentSession,
    plan: LaunchPlan,
    mut cancel_rx: oneshot::Receiver<()>,
) -> Result<Outcome, OpsError> {
    let workdir = session
        .working_directory
        .clone()
        .ok_or_else(|| OpsError::Internal("oturumun çalışma dizini yok".into()))?;

    info!(session = %session.id, provider = %session.provider.as_ref(),
          mode = %session.mode.as_ref(), "agent oturumu başlıyor");

    let mut cmd = tokio::process::Command::new(&plan.program);
    cmd.args(&plan.args)
        .current_dir(&workdir)
        .env_clear()
        .envs(minimal_env(&plan.program))
        .stdin(if plan.stdin_payload.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child =
        cmd.spawn().map_err(|e| OpsError::Internal(format!("CLI başlatılamadı: {e}")))?;
    if let Some(payload) = &plan.stdin_payload {
        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            let _ = stdin.write_all(payload.as_bytes()).await;
            let _ = stdin.shutdown().await;
        }
    }
    let stdout = child.stdout.take().expect("stdout piped");
    let mut stderr = child.stderr.take().expect("stderr piped");

    let stderr_task = tokio::spawn(async move {
        let mut buf = String::new();
        let _ = stderr.read_to_string(&mut buf).await;
        let tail: String = buf.chars().rev().take(2000).collect::<String>().chars().rev().collect();
        tail
    });

    let mut lines = BufReader::new(stdout).lines();
    let deadline = Instant::now() + SESSION_TIMEOUT;
    let mut total_bytes = 0usize;
    let mut result_text: Option<String> = None;
    let mut result_is_error = false;
    let mut last_assistant: Option<String> = None;

    loop {
        tokio::select! {
            _ = &mut cancel_rx => {
                let _ = child.start_kill();
                let _ = child.wait().await;
                return Ok(Outcome {
                    status: AgentSessionStatus::Cancelled,
                    summary: Some("kullanıcı iptal etti".into()),
                });
            }
            _ = sleep_until(deadline) => {
                let _ = child.start_kill();
                let _ = child.wait().await;
                return Ok(Outcome {
                    status: AgentSessionStatus::Failed,
                    summary: Some("zaman aşımı (15 dk)".into()),
                });
            }
            line = lines.next_line() => {
                match line {
                    Ok(Some(l)) => {
                        total_bytes += l.len();
                        if total_bytes > OUTPUT_CAP_BYTES {
                            let _ = child.start_kill();
                            let _ = child.wait().await;
                            return Ok(Outcome {
                                status: AgentSessionStatus::Failed,
                                summary: Some("çıktı sınırı aşıldı".into()),
                            });
                        }
                        handle_line(
                            mgr, session, &l,
                            &mut result_text, &mut result_is_error, &mut last_assistant,
                        )?;
                    }
                    Ok(None) => break,
                    Err(e) => {
                        warn!(error = %e, "stdout okuma hatası");
                        break;
                    }
                }
            }
        }
    }

    let status = child.wait().await.map_err(|e| OpsError::Internal(e.to_string()))?;
    let err_tail = stderr_task.await.unwrap_or_default();

    // Sonuç metni asistan mesajı olarak hiç düşmediyse tamamla.
    if let Some(rt) = &result_text {
        if last_assistant.is_none() && !rt.trim().is_empty() && !result_is_error {
            mgr.store.append_agent_message(&session.id, AgentMessageRole::Assistant, rt, None)?;
            last_assistant = Some(rt.clone());
        }
    }

    let success = status.success() && !result_is_error;
    if success {
        let summary = result_text.filter(|t| !t.trim().is_empty()).or(last_assistant);
        Ok(Outcome { status: AgentSessionStatus::Completed, summary })
    } else {
        let detail = result_text.filter(|t| !t.trim().is_empty()).unwrap_or_else(|| {
            if err_tail.trim().is_empty() {
                format!("CLI hata koduyla kapandı: {status}")
            } else {
                err_tail.trim().to_string()
            }
        });
        mgr.store.append_agent_message(&session.id, AgentMessageRole::Error, &detail, None)?;
        Ok(Outcome { status: AgentSessionStatus::Failed, summary: Some(detail) })
    }
}

fn handle_line(
    mgr: &Arc<AgentManager>,
    session: &AgentSession,
    line: &str,
    result_text: &mut Option<String>,
    result_is_error: &mut bool,
    last_assistant: &mut Option<String>,
) -> Result<(), OpsError> {
    match session.provider {
        AgentProviderKind::Claude => {
            for event in claude::parse(line) {
                match event {
                    claude::Event::Init { session_id } => {
                        if session.provider_session_id.is_none() {
                            mgr.store.set_agent_provider_session(&session.id, &session_id)?;
                        }
                    }
                    claude::Event::Text(t) => {
                        mgr.store.append_agent_message(
                            &session.id,
                            AgentMessageRole::Assistant,
                            &t,
                            None,
                        )?;
                        *last_assistant = Some(t);
                    }
                    claude::Event::Tool { name, detail } => {
                        mgr.store.append_agent_message(
                            &session.id,
                            AgentMessageRole::Tool,
                            &format!("{name} · {detail}"),
                            None,
                        )?;
                    }
                    claude::Event::Result { text, is_error } => {
                        *result_text = Some(text);
                        *result_is_error = is_error;
                    }
                }
            }
        }
        AgentProviderKind::Codex => {
            for event in codex::parse(line) {
                match event {
                    codex::Event::Init { session_id } => {
                        if session.provider_session_id.is_none() {
                            mgr.store.set_agent_provider_session(&session.id, &session_id)?;
                        }
                    }
                    codex::Event::Text(t) => {
                        if last_assistant.as_deref() != Some(t.as_str()) {
                            mgr.store.append_agent_message(
                                &session.id,
                                AgentMessageRole::Assistant,
                                &t,
                                None,
                            )?;
                            *last_assistant = Some(t);
                        }
                    }
                    codex::Event::Tool { name, detail } => {
                        mgr.store.append_agent_message(
                            &session.id,
                            AgentMessageRole::Tool,
                            &format!("{name} · {detail}"),
                            None,
                        )?;
                    }
                    codex::Event::Error(m) => {
                        mgr.store.append_agent_message(
                            &session.id,
                            AgentMessageRole::Error,
                            &m,
                            None,
                        )?;
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ops_core::models::ProjectCreate;

    #[test]
    fn minimal_env_does_not_inherit_arbitrary_variables() {
        std::env::set_var("PERSONAL_OPS_TEST_LEAK", "secret");
        let env = minimal_env(Path::new("/usr/local/bin/claude"));
        let keys: Vec<&str> = env.iter().map(|(k, _)| k.as_str()).collect();
        assert!(
            !keys.contains(&"PERSONAL_OPS_TEST_LEAK"),
            "yalnızca allowlist'teki değişkenler geçer"
        );
        assert!(keys.contains(&"PATH") && keys.contains(&"HOME"));
        let path = env.iter().find(|(k, _)| k == "PATH").map(|(_, v)| v.clone()).unwrap();
        assert!(path.starts_with("/usr/local/bin:"), "CLI'nın kendi dizini PATH'in başındadır");
        std::env::remove_var("PERSONAL_OPS_TEST_LEAK");
    }

    #[test]
    fn workdir_rules_by_mode() {
        let store = Store::in_memory().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let with_dir = store
            .create_project(
                &Ctx::LOCAL_USER,
                ProjectCreate {
                    name: "Onaylı".into(),
                    local_paths: Some(vec![tmp.path().display().to_string()]),
                    ..Default::default()
                },
            )
            .unwrap();
        let without_dir = store
            .create_project(
                &Ctx::LOCAL_USER,
                ProjectCreate { name: "Klasörsüz".into(), ..Default::default() },
            )
            .unwrap();

        // Proje kökü olan modlar yalnızca onaylı, var olan klasörde koşar.
        let (pid, dir) =
            resolve_workdir(&store, AgentMode::Edit, Some(with_dir.id.clone())).unwrap();
        assert_eq!(pid.as_deref(), Some(with_dir.id.as_str()));
        assert_eq!(dir, tmp.path().canonicalize().unwrap());
        assert!(resolve_workdir(&store, AgentMode::Edit, Some(without_dir.id.clone())).is_err());
        assert!(resolve_workdir(&store, AgentMode::Read, None).is_err());
        assert!(resolve_workdir(&store, AgentMode::Act, None).is_err());
        assert!(resolve_workdir(&store, AgentMode::Edit, Some("yok".into())).is_err());

        // ASK projesiz de çalışır; proje klasörsüzse veri dizinine düşer.
        assert!(resolve_workdir(&store, AgentMode::Ask, None).is_ok());
        assert!(resolve_workdir(&store, AgentMode::Ask, Some(without_dir.id)).is_ok());
    }
}
