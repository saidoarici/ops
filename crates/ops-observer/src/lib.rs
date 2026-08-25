//! ops-observer — arka plan gözlemcisi.
//!
//! Yalnızca kullanıcının açıkça seçtiği proje klasörlerini izler; tüm disk
//! asla indekslenmez. Salt-okunurdur; shell çalıştırmaz; dosya içeriği değil
//! metadata (ad/sayı/özet) toplar. FSEvents (notify) + 5 dakikalık periyodik
//! tarama; olaylar 3 sn debounce ile toplanır.

pub mod detect;
pub mod gitscan;
pub mod health;
pub mod scan;

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use notify::{RecursiveMode, Watcher};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio::time::{interval, sleep, Duration, Instant, MissedTickBehavior};
use tracing::{info, warn};

use ops_core::models::{
    Actor, AuditResult, EvidenceType, NewAudit, NewEvidence, Origin, RiskLevel,
};
use ops_core::store::Store;
use ops_core::time;

const SCAN_PERIOD_SECS: u64 = 300;
const DEBOUNCE_SECS: u64 = 3;
const FS_EVIDENCE_MIN_GAP_MINS: i64 = 15;

/// İzlenmeyecek dizin/dosya bileşenleri (gürültü + gizlilik).
const IGNORED_COMPONENTS: &[&str] = &[
    "node_modules",
    "target",
    "dist",
    "build",
    ".next",
    ".venv",
    "__pycache__",
    ".idea",
    ".vscode",
    ".DS_Store",
    ".cache",
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    pub projects: usize,
    pub repos: usize,
    pub evidence_added: usize,
    pub detected_open: usize,
    pub errors: Vec<String>,
    pub finished_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObserverStatus {
    pub running: bool,
    pub watched_paths: Vec<String>,
    pub last_scan_at: Option<DateTime<Utc>>,
    pub last_summary: Option<ScanSummary>,
}

struct ObserverState {
    watcher: Option<notify::RecommendedWatcher>,
    /// (canonical kök, project_id) — event → proje eşlemesi için.
    watched: Vec<(PathBuf, String)>,
    /// spawn'da kurulur; talep üzerine watcher tazelemek için saklanır.
    event_tx: Option<UnboundedSender<notify::Event>>,
    last_scan_at: Option<DateTime<Utc>>,
    last_summary: Option<ScanSummary>,
    fs_evidence_at: HashMap<String, DateTime<Utc>>,
}

pub struct Observer {
    store: Store,
    scan_lock: Mutex<()>,
    state: Mutex<ObserverState>,
}

impl Observer {
    pub fn new(store: Store) -> Arc<Self> {
        Arc::new(Self {
            store,
            scan_lock: Mutex::new(()),
            state: Mutex::new(ObserverState {
                watcher: None,
                watched: Vec::new(),
                event_tx: None,
                last_scan_at: None,
                last_summary: None,
                fs_evidence_at: HashMap::new(),
            }),
        })
    }

    /// Proje yolları değişmiş olabilir (create/update): watcher'ları tazele.
    /// spawn henüz çalışmadıysa sessizce no-op.
    pub fn refresh(&self) {
        let tx = {
            let st = self.state.lock().unwrap_or_else(|e| e.into_inner());
            st.event_tx.clone()
        };
        if let Some(tx) = tx {
            self.refresh_watchers(tx);
        }
    }

    /// Tüm projeleri tarar (seri; eşzamanlı çağrılar sıraya girer).
    pub fn scan_all(&self) -> ScanSummary {
        let _guard = self.scan_lock.lock().unwrap_or_else(|e| e.into_inner());
        let now = time::now();
        let mut summary = ScanSummary {
            projects: 0,
            repos: 0,
            evidence_added: 0,
            detected_open: 0,
            errors: Vec::new(),
            finished_at: now,
        };
        match self.store.list_projects(false) {
            Ok(projects) => {
                for p in projects {
                    summary.projects += 1;
                    match scan::scan_project(&self.store, &p.project, now) {
                        Ok(out) => {
                            summary.repos += out.repos;
                            summary.evidence_added += out.evidence_added;
                            summary.errors.extend(
                                out.errors.into_iter().map(|e| format!("{}: {e}", p.project.name)),
                            );
                        }
                        Err(e) => summary.errors.push(format!("{}: {e}", p.project.name)),
                    }
                }
            }
            Err(e) => summary.errors.push(format!("projeler listelenemedi: {e}")),
        }
        if let Err(e) = scan::scan_stale_tasks(&self.store, now) {
            summary.errors.push(format!("stale taraması: {e}"));
        }
        if let Ok(open) = self.store.list_detected(false) {
            summary.detected_open = open.len();
        }
        summary.finished_at = time::now();

        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        st.last_scan_at = Some(summary.finished_at);
        st.last_summary = Some(summary.clone());
        summary
    }

    pub fn status(&self) -> ObserverStatus {
        let st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        ObserverStatus {
            running: st.event_tx.is_some(),
            watched_paths: st.watched.iter().map(|(p, _)| p.display().to_string()).collect(),
            last_scan_at: st.last_scan_at,
            last_summary: st.last_summary.clone(),
        }
    }

    /// Onaylı kökler değiştiyse FSEvents watcher'larını yeniden kurar.
    fn refresh_watchers(&self, tx: UnboundedSender<notify::Event>) {
        let desired: Vec<(PathBuf, String)> = match self.store.list_projects(false) {
            Ok(projects) => {
                let mut seen: BTreeSet<PathBuf> = BTreeSet::new();
                let mut out = Vec::new();
                for p in projects {
                    for lp in &p.project.local_paths {
                        if let Ok(canon) = std::fs::canonicalize(lp) {
                            if canon.is_dir() && seen.insert(canon.clone()) {
                                out.push((canon, p.project.id.clone()));
                            }
                        }
                    }
                }
                out
            }
            Err(e) => {
                warn!(error = %e, "watcher için projeler okunamadı");
                return;
            }
        };

        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let current: BTreeSet<&PathBuf> = st.watched.iter().map(|(p, _)| p).collect();
        let wanted: BTreeSet<&PathBuf> = desired.iter().map(|(p, _)| p).collect();
        if current == wanted {
            st.watched = desired; // pid eşleşmeleri güncel kalsın
            return;
        }

        let mut watcher =
            match notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
                if let Ok(ev) = res {
                    let _ = tx.send(ev);
                }
            }) {
                Ok(w) => w,
                Err(e) => {
                    warn!(error = %e, "watcher kurulamadı");
                    return;
                }
            };
        for (path, _) in &desired {
            if let Err(e) = watcher.watch(path, RecursiveMode::Recursive) {
                warn!(path = %path.display(), error = %e, "yol izlenemiyor");
            }
        }
        info!(count = desired.len(), "dosya izleyici kuruldu");
        st.watcher = Some(watcher);
        st.watched = desired;
    }

    /// Event yolunu (en uzun eşleşen) onaylı köke ve projeye eşler.
    fn locate(&self, path: &Path) -> Option<(String, PathBuf)> {
        let st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        st.watched
            .iter()
            .filter(|(root, _)| path.starts_with(root))
            .max_by_key(|(root, _)| root.components().count())
            .map(|(root, pid)| (pid.clone(), root.clone()))
    }

    /// Debounce sonrası: FS aktivitesini işle — aktivite damgası, (rate-limitli)
    /// FILE_CHANGE evidence ve ilgili projenin git taraması. `files` boşsa
    /// yalnızca `.git` durumu değişmiştir (commit/branch); o zaman sadece tarama yapılır.
    fn record_fs_activity(&self, project_id: &str, files: BTreeSet<String>) {
        let now = time::now();
        if let Err(e) = self.store.touch_project_activity(project_id, now) {
            warn!(error = %e, "aktivite güncellenemedi");
        }

        if !files.is_empty() {
            let allowed = {
                let st = self.state.lock().unwrap_or_else(|e| e.into_inner());
                st.fs_evidence_at
                    .get(project_id)
                    .map(|t| now - *t >= ChronoDuration::minutes(FS_EVIDENCE_MIN_GAP_MINS))
                    .unwrap_or(true)
            };
            if allowed {
                let names: Vec<&String> = files.iter().take(3).collect();
                let summary = if files.len() == 1 {
                    format!("{} değişti", names[0])
                } else {
                    let extra = if files.len() > 3 { ", …" } else { "" };
                    format!(
                        "{} dosya değişti: {}{}",
                        files.len(),
                        names.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "),
                        extra
                    )
                };
                let bucket = now.timestamp() / (FS_EVIDENCE_MIN_GAP_MINS * 60);
                let hash = hex::encode(Sha256::digest(format!("fs:{project_id}:{bucket}")));
                match self.store.add_evidence(NewEvidence {
                    task_id: None,
                    project_id: Some(project_id.to_string()),
                    kind: EvidenceType::FileChange,
                    source: "observer:fs".into(),
                    timestamp: now,
                    summary,
                    confidence: None,
                    source_reference: None,
                    content_hash: Some(hash),
                }) {
                    Ok(Some(_)) => {
                        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
                        st.fs_evidence_at.insert(project_id.to_string(), now);
                        drop(st);
                        let _ = self.store.append_audit(NewAudit {
                            actor: Actor::Daemon,
                            origin: Origin::Daemon,
                            action: "OBSERVER_OBSERVATION".into(),
                            target: Some(format!("project:{project_id}")),
                            risk_level: RiskLevel::R0,
                            capability: Some("READ_PROJECT_FILES".into()),
                            result: AuditResult::Ok,
                            metadata: serde_json::json!({ "files": files.len() }),
                        });
                    }
                    Ok(None) => {}
                    Err(e) => warn!(error = %e, "fs evidence yazılamadı"),
                }
            }
        }

        // Dosya değişimi ya da .git güncellemesi → repo durumunu tazele.
        if let Ok(project) = self.store.get_project(project_id) {
            let _guard = self.scan_lock.lock().unwrap_or_else(|e| e.into_inner());
            if let Err(e) = scan::scan_project(&self.store, &project, now) {
                warn!(error = %e, "fs sonrası tarama hatası");
            }
        }
    }
}

/// Debounce penceresinde biriken, proje başına değişen dosya kümesi.
/// Boş küme = yalnızca `.git` durumu değişti.
type Pending = BTreeSet<String>;

fn interesting(kind: &notify::EventKind) -> bool {
    use notify::EventKind::*;
    matches!(kind, Create(_) | Modify(_) | Remove(_) | Any | Other)
}

/// Event yolunu sınıflandırır: None = yoksay; Some((rel, is_git_state)).
fn classify(root: &Path, path: &Path) -> Option<(String, bool)> {
    let rel = path.strip_prefix(root).ok()?;
    let comps: Vec<String> =
        rel.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect();
    if comps.is_empty() {
        return None;
    }
    if comps[0] == ".git" {
        // Yalnızca HEAD/refs değişimi anlamlı (commit/branch sinyali).
        let is_state = comps.iter().any(|c| c == "HEAD" || c == "refs" || c == "packed-refs");
        return is_state.then(|| (String::new(), true));
    }
    if comps.iter().any(|c| IGNORED_COMPONENTS.contains(&c.as_str())) {
        return None;
    }
    Some((rel.display().to_string(), false))
}

/// Observer arka plan görevini başlatır: periyodik tarama + FSEvents debounce.
pub fn spawn(
    observer: Arc<Observer>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let (tx, mut rx) = unbounded_channel::<notify::Event>();
        {
            let mut st = observer.state.lock().unwrap_or_else(|e| e.into_inner());
            st.event_tx = Some(tx.clone());
        }
        observer.refresh_watchers(tx.clone());

        let mut ticker = interval(Duration::from_secs(SCAN_PERIOD_SECS));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

        let flush_timer = sleep(Duration::from_secs(3600));
        tokio::pin!(flush_timer);
        let mut pending: HashMap<String, Pending> = HashMap::new();

        loop {
            tokio::select! {
                _ = shutdown.changed() => break,

                _ = ticker.tick() => {
                    observer.refresh_watchers(tx.clone());
                    let obs = observer.clone();
                    let summary = tokio::task::spawn_blocking(move || obs.scan_all()).await;
                    match summary {
                        Ok(s) if s.evidence_added > 0 || !s.errors.is_empty() => {
                            info!(projects = s.projects, evidence = s.evidence_added,
                                  errors = s.errors.len(), "periyodik tarama");
                        }
                        Ok(_) => {}
                        Err(e) => warn!(error = %e, "tarama görevi düştü"),
                    }
                }

                Some(ev) = rx.recv() => {
                    if !interesting(&ev.kind) { continue; }
                    for path in &ev.paths {
                        if let Some((pid, root)) = observer.locate(path) {
                            if let Some((rel, is_git)) = classify(&root, path) {
                                let files = pending.entry(pid).or_default();
                                if !is_git {
                                    files.insert(rel);
                                }
                            }
                        }
                    }
                    if !pending.is_empty() {
                        flush_timer.as_mut().reset(Instant::now() + Duration::from_secs(DEBOUNCE_SECS));
                    }
                }

                () = &mut flush_timer, if !pending.is_empty() => {
                    for (pid, files) in pending.drain() {
                        let obs = observer.clone();
                        let _ = tokio::task::spawn_blocking(move || {
                            obs.record_fs_activity(&pid, files)
                        }).await;
                    }
                }
            }
        }
        info!("observer kapandı");
    })
}
