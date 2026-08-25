use clap::{Parser, Subcommand};
use tokio::signal::unix::{signal, SignalKind};
use tracing::info;
use tracing_subscriber::EnvFilter;

use ops_core::models::Ctx;
use ops_core::store::Store;
use ops_core::{paths, seed};
use ops_daemon::{launchd, scheduler, server, AppState};

#[derive(Parser)]
#[command(
    name = "personal-opsd",
    version,
    about = "Personal Ops arka plan servisi (UDS API + scheduler + observer)"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Daemon'ı ön planda çalıştır (varsayılan)
    Run,
    /// Kurgusal demo çalışma alanı yükle (yalnızca boş veritabanına; --force ile zorla)
    SeedDemo {
        #[arg(long)]
        force: bool,
    },
    /// launchd user agent olarak kur (login'de otomatik başlar)
    InstallLaunchd,
    /// launchd agent'ını kaldır
    UninstallLaunchd,
    /// launchd kayıt durumunu göster
    LaunchdStatus,
    /// Audit hash zincirini doğrula (tamper kontrolü)
    VerifyAudit,
    /// Yerel yedek al
    Backup,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    match Cli::parse().command.unwrap_or(Cmd::Run) {
        Cmd::Run => run_daemon().await,
        Cmd::SeedDemo { force } => {
            let store = open_store()?;
            let report = seed::seed_demo(&store, force)?;
            println!(
                "✓ demo verisi yüklendi: {} proje, {} görev, {} hatırlatma",
                report.projects, report.tasks, report.reminders
            );
            println!("  veri dizini: {}", paths::data_dir().display());
            Ok(())
        }
        Cmd::InstallLaunchd => launchd::install(),
        Cmd::UninstallLaunchd => launchd::uninstall(),
        Cmd::LaunchdStatus => launchd::status(),
        Cmd::VerifyAudit => {
            let store = open_store()?;
            let report = store.verify_audit()?;
            println!("{}", report.message);
            if !report.ok {
                std::process::exit(1);
            }
            Ok(())
        }
        Cmd::Backup => {
            let store = open_store()?;
            let info = store.backup_to(&Ctx::CLI, &paths::backups_dir())?;
            println!("✓ yedek alındı: {} ({} bayt)", info.path, info.size_bytes);
            Ok(())
        }
    }
}

fn open_store() -> anyhow::Result<Store> {
    paths::ensure_data_dirs()?;
    let store = Store::open_default()?;
    store.ensure_builtin_routines()?;
    Ok(store)
}

async fn run_daemon() -> anyhow::Result<()> {
    let store = open_store()?;
    let state = AppState::new(store);
    let socket = paths::socket_path();
    info!(
        version = env!("CARGO_PKG_VERSION"),
        data_dir = %paths::data_dir().display(),
        "personal-opsd başlıyor"
    );

    // SIGINT/SIGTERM → tüm görevlere yayınlanan kapanış sinyali.
    let (tx, rx) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        let ctrl_c = tokio::signal::ctrl_c();
        let mut term = signal(SignalKind::terminate()).expect("SIGTERM handler kurulamadı");
        tokio::select! {
            _ = ctrl_c => {},
            _ = term.recv() => {},
        }
        info!("kapanış sinyali alındı");
        let _ = tx.send(true);
    });

    let sched_state = state.clone();
    let mut sched_rx = rx.clone();
    let sched = tokio::spawn(async move {
        scheduler::run(sched_state, async move {
            let _ = sched_rx.changed().await;
        })
        .await;
    });
    let observer_task = ops_observer::spawn(state.observer.clone(), rx.clone());
    let remote_task = ops_remote::spawn(state.remote.clone(), rx.clone());

    let mut server_rx = rx.clone();
    server::run(state, &socket, async move {
        let _ = server_rx.changed().await;
    })
    .await?;

    let _ = sched.await;
    let _ = observer_task.await;
    let _ = remote_task.await;
    Ok(())
}
