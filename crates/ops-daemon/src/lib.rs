//! personal-opsd — Personal Ops arka plan servisi.
//! UI kapalıyken de çalışır: UDS server + scheduler + observer + remote gateway.
//! Root yetkisi istemez; yalnızca mevcut macOS kullanıcı hesabıyla çalışır.

pub mod dispatch;
pub mod full_access;
pub mod launchd;
pub mod notify;
pub mod routines;
pub mod scheduler;
pub mod server;

use std::sync::Arc;
use std::time::Instant;

use ops_core::store::Store;

use crate::full_access::FullAccessAuth;

/// Daemon'ın paylaşılan durumu. Alt sistemler kurulur ama arka plan görevleri
/// (`ops_observer::spawn`, `ops_remote::spawn`, `scheduler::run`) yalnızca
/// `personal-opsd run` tarafından başlatılır; CLI ve testler bunları çalıştırmaz.
pub struct AppState {
    pub store: Store,
    pub started_at: Instant,
    pub observer: Arc<ops_observer::Observer>,
    pub agent: Arc<ops_agent::AgentManager>,
    pub remote: Arc<ops_remote::Remote>,
    /// Yerel Tam Erişim parola doğrulaması ve brute-force kilidi.
    pub full_access: FullAccessAuth,
}

impl AppState {
    pub fn new(store: Store) -> Arc<Self> {
        Arc::new(Self {
            observer: ops_observer::Observer::new(store.clone()),
            agent: ops_agent::AgentManager::new(store.clone()),
            remote: ops_remote::Remote::new(store.clone()),
            store,
            started_at: Instant::now(),
            full_access: FullAccessAuth::default(),
        })
    }
}
