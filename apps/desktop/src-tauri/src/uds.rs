//! Daemon'a NDJSON/UDS istemcisi. İstek başına kısa ömürlü bağlantı:
//! lokal socket'te maliyeti ihmal edilebilir, durum yönetimi basittir.

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::time::{timeout, Duration};

use crate::ErrPayload;

const CALL_TIMEOUT: Duration = Duration::from_secs(10);

pub async fn call(method: &str, params: Value) -> Result<Value, ErrPayload> {
    let path = ops_core::paths::socket_path();
    let stream = UnixStream::connect(&path).await.map_err(|e| ErrPayload {
        code: "DISCONNECTED".into(),
        message: format!("arka plan servisine bağlanılamadı ({e})"),
    })?;
    let (read_half, mut write_half) = stream.into_split();

    let request =
        json!({ "id": uuid_ish(), "method": method, "params": params }).to_string() + "\n";
    write_half.write_all(request.as_bytes()).await.map_err(io_err)?;

    let mut lines = BufReader::new(read_half).lines();
    let line = timeout(CALL_TIMEOUT, lines.next_line())
        .await
        .map_err(|_| ErrPayload {
            code: "TIMEOUT".into(), message: "daemon yanıt vermedi".into()
        })?
        .map_err(io_err)?
        .ok_or_else(|| ErrPayload {
            code: "DISCONNECTED".into(),
            message: "daemon bağlantıyı kapattı".into(),
        })?;

    let response: Value = serde_json::from_str(&line).map_err(|e| ErrPayload {
        code: "BAD_RESPONSE".into(),
        message: format!("daemon yanıtı çözümlenemedi: {e}"),
    })?;
    if let Some(err) = response.get("error") {
        return Err(ErrPayload {
            code: err.get("code").and_then(|c| c.as_str()).unwrap_or("INTERNAL").to_string(),
            message: err
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("bilinmeyen hata")
                .to_string(),
        });
    }
    Ok(response.get("result").cloned().unwrap_or(Value::Null))
}

fn io_err(e: std::io::Error) -> ErrPayload {
    ErrPayload { code: "IO".into(), message: e.to_string() }
}

/// Bağımlılık eklemeden yeterince benzersiz istek id'si.
fn uuid_ish() -> String {
    format!(
        "{:x}-{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
        std::process::id()
    )
}
