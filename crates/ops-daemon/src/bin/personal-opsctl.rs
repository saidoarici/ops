//! Personal Ops'un yerel, tipli kontrol CLI'ı.
//!
//! Yerleşik asistan (Claude/Codex oturumu) uygulama verisine bu araçla dokunur;
//! SQLite'a doğrudan yazmaz. Tüm mutasyonlar normal Store doğrulaması ve
//! hash-chain audit'inden geçer.

use std::path::PathBuf;
use std::str::FromStr;

use clap::{Parser, Subcommand};
use ops_core::models::{Ctx, ProjectCreate, TaskCreate, TaskFilter, TaskSource, TaskStatus};
use ops_core::paths;
use ops_core::store::Store;
use serde::Serialize;
use serde_json::json;

#[derive(Parser)]
#[command(name = "personal-opsctl", version, about = "Personal Ops yerel kontrol CLI'ı")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Asistan için mevcut proje, açık görev ve kullanıcı bağlamını JSON yazdır.
    Context,
    Project {
        #[command(subcommand)]
        command: ProjectCommand,
    },
    Task {
        #[command(subcommand)]
        command: TaskCommand,
    },
}

#[derive(Subcommand)]
enum ProjectCommand {
    List,
    Add {
        #[arg(long)]
        name: String,
        #[arg(long)]
        description: Option<String>,
        /// Onaylanacak yerel proje yolu; birden fazla kez verilebilir.
        #[arg(long, required = true)]
        path: Vec<PathBuf>,
    },
}

#[derive(Subcommand)]
enum TaskCommand {
    List {
        #[arg(long)]
        project_id: Option<String>,
        #[arg(long, default_value_t = 100)]
        limit: i64,
    },
    Add {
        #[arg(long)]
        title: String,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        project_id: Option<String>,
        #[arg(long, default_value = "INBOX")]
        status: String,
    },
    Complete {
        #[arg(long)]
        id: String,
    },
}

fn main() -> anyhow::Result<()> {
    let store = open_store()?;
    match Cli::parse().command {
        Command::Context => {
            let projects = store.list_projects(false)?;
            let tasks = store
                .list_tasks(&TaskFilter { limit: Some(100), ..Default::default() })?
                .into_iter()
                .filter(|t| t.status.is_open())
                .collect::<Vec<_>>();
            print_json(&json!({
                "app": "Personal Ops",
                "dataDir": paths::data_dir(),
                "settings": store.get_settings()?,
                "projects": projects,
                "openTasks": tasks,
            }))?;
        }
        Command::Project { command: ProjectCommand::List } => {
            print_json(&store.list_projects(false)?)?;
        }
        Command::Project { command: ProjectCommand::Add { name, description, path } } => {
            let local_paths = path
                .into_iter()
                .map(|p| p.canonicalize())
                .collect::<std::io::Result<Vec<_>>>()?
                .into_iter()
                .map(|p| p.display().to_string())
                .collect();
            let project = store.create_project(
                &Ctx::CLI,
                ProjectCreate {
                    name,
                    description,
                    local_paths: Some(local_paths),
                    ..Default::default()
                },
            )?;
            print_json(&project)?;
        }
        Command::Task { command: TaskCommand::List { project_id, limit } } => {
            print_json(&store.list_tasks(&TaskFilter {
                project_id,
                limit: Some(limit),
                ..Default::default()
            })?)?;
        }
        Command::Task { command: TaskCommand::Add { title, description, project_id, status } } => {
            let status = TaskStatus::from_str(&status)
                .map_err(|_| anyhow::anyhow!("geçersiz görev durumu: {status}"))?;
            let task = store.create_task(
                &Ctx::CLI,
                TaskCreate {
                    title,
                    description,
                    project_id,
                    status: Some(status),
                    source: Some(TaskSource::AgentChat),
                    ..Default::default()
                },
            )?;
            print_json(&task)?;
        }
        Command::Task { command: TaskCommand::Complete { id } } => {
            print_json(&store.complete_task(&Ctx::CLI, &id)?)?;
        }
    }
    Ok(())
}

fn open_store() -> anyhow::Result<Store> {
    paths::ensure_data_dirs()?;
    let store = Store::open_default()?;
    store.ensure_builtin_routines()?;
    Ok(store)
}

fn print_json(value: &impl Serialize) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
