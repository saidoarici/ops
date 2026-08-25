//! Unix Domain Socket üzerinde NDJSON server.
//! Socket dosyası 0600, veri dizini 0700: yalnızca aynı kullanıcı erişebilir.
//! TCP portu (localhost dahi) açılmaz (docs/architecture.md).

use std::fs;
use std::future::Future;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Arc;

use anyhow::{bail, Context};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tracing::{debug, info, warn};

use ops_core::ipc::{self, Request};

use crate::dispatch::dispatch;
use crate::AppState;

/// Tek bir istek satırının üst sınırı. En büyük gerçek istek (20.000
/// karakterlik agent prompt'u) bunun çok altındadır; aşan istemci kapatılır.
pub const MAX_REQUEST_BYTES: usize = 1024 * 1024;

pub async fn run(
    state: Arc<AppState>,
    socket_path: &Path,
    shutdown: impl Future<Output = ()>,
) -> anyhow::Result<()> {
    prepare_socket(socket_path).await?;
    let listener = UnixListener::bind(socket_path)
        .with_context(|| format!("socket bind edilemedi: {}", socket_path.display()))?;
    fs::set_permissions(socket_path, fs::Permissions::from_mode(0o600))?;
    info!(socket = %socket_path.display(), "UDS server dinlemede (0600)");

    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            accepted = listener.accept() => match accepted {
                Ok((stream, _addr)) => {
                    let st = state.clone();
                    tokio::spawn(handle_conn(st, stream));
                }
                Err(e) => warn!(error = %e, "bağlantı kabul hatası"),
            },
        }
    }
    let _ = fs::remove_file(socket_path);
    info!("server kapandı, socket temizlendi");
    Ok(())
}

/// Tek instance garantisi: canlı bir daemon varsa çıkar, ölü socket'i temizler.
async fn prepare_socket(path: &Path) -> anyhow::Result<()> {
    if path.exists() {
        match UnixStream::connect(path).await {
            Ok(_) => bail!("başka bir personal-opsd zaten çalışıyor ({})", path.display()),
            Err(_) => {
                warn!("ölü socket dosyası temizleniyor");
                fs::remove_file(path)?;
            }
        }
    }
    Ok(())
}

async fn handle_conn(state: Arc<AppState>, stream: UnixStream) {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let mut buf: Vec<u8> = Vec::new();
    loop {
        buf.clear();
        // `take` ile satır başına üst sınır: sınır aşılırsa satır sonu okunmadan döner.
        let read =
            (&mut reader).take(MAX_REQUEST_BYTES as u64 + 1).read_until(b'\n', &mut buf).await;
        match read {
            Ok(0) => break,
            Ok(_) if buf.len() > MAX_REQUEST_BYTES => {
                let line = ipc::err_line(&None, "BAD_REQUEST", "istek çok büyük");
                let _ = write_half.write_all(format!("{line}\n").as_bytes()).await;
                break;
            }
            Ok(_) => {
                let text = String::from_utf8_lossy(&buf);
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let response = respond(&state, trimmed).await;
                if write_half.write_all(format!("{response}\n").as_bytes()).await.is_err() {
                    break;
                }
            }
            Err(e) => {
                debug!(error = %e, "bağlantı okuma hatası");
                break;
            }
        }
    }
}

async fn respond(state: &AppState, line: &str) -> String {
    match serde_json::from_str::<Request>(line) {
        Ok(req) => {
            debug!(method = %req.method, "istek");
            match dispatch(state, &req.method, req.params).await {
                Ok(result) => ipc::ok_line(&req.id, &result),
                Err(e) => {
                    debug!(method = %req.method, error = %e, "istek hatası");
                    ipc::err_line_from(&req.id, &e)
                }
            }
        }
        Err(e) => ipc::err_line(&None, "BAD_REQUEST", &format!("geçersiz istek: {e}")),
    }
}
