//! Daemon UDS entegrasyon testi: gerçek socket üzerinden istek/yanıt turu,
//! socket izinleri (S9) ve bilinmeyen metod reddi.

use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use ops_core::store::Store;
use ops_daemon::{server, AppState};

async fn call(
    w: &mut tokio::net::unix::OwnedWriteHalf,
    lines: &mut tokio::io::Lines<BufReader<tokio::net::unix::OwnedReadHalf>>,
    req: &str,
) -> serde_json::Value {
    w.write_all(req.as_bytes()).await.unwrap();
    w.write_all(b"\n").await.unwrap();
    let line = tokio::time::timeout(Duration::from_secs(5), lines.next_line())
        .await
        .expect("yanıt zaman aşımı")
        .unwrap()
        .expect("bağlantı kapandı");
    serde_json::from_str::<serde_json::Value>(&line).unwrap()
}

#[tokio::test]
async fn uds_roundtrip_perms_and_unknown_method() {
    let tmp = tempfile::tempdir().unwrap();
    let socket = tmp.path().join("test-daemon.sock");

    let state = AppState::new(Store::in_memory().unwrap());
    let (tx, mut rx) = tokio::sync::watch::channel(false);
    let socket_for_server = socket.clone();
    let server_task = tokio::spawn(async move {
        server::run(state, &socket_for_server, async move {
            let _ = rx.changed().await;
        })
        .await
        .unwrap();
    });

    // Socket hazır olana kadar bekle
    for _ in 0..100 {
        if socket.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(socket.exists(), "socket oluşmadı");

    // S9: socket izni 0600 olmalı
    let mode = std::fs::metadata(&socket).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "socket izni 0600 olmalı, bulundu: {mode:o}");

    let stream = UnixStream::connect(&socket).await.unwrap();
    let (r, mut w) = stream.into_split();
    let mut lines = BufReader::new(r).lines();

    // health.check
    let health = call(&mut w, &mut lines, r#"{"id":1,"method":"health.check"}"#).await;
    assert_eq!(health["id"], 1);
    assert_eq!(health["result"]["ok"], true);

    // task.create → task.list
    let created = call(
        &mut w,
        &mut lines,
        r#"{"id":2,"method":"task.create","params":{"title":"UDS üzerinden görev"}}"#,
    )
    .await;
    assert_eq!(created["result"]["title"], "UDS üzerinden görev");
    assert_eq!(created["result"]["status"], "INBOX");

    let listed = call(&mut w, &mut lines, r#"{"id":3,"method":"task.list"}"#).await;
    assert_eq!(listed["result"].as_array().unwrap().len(), 1);

    // Bilinmeyen metod tipli hata döner
    let unknown =
        call(&mut w, &mut lines, r#"{"id":4,"method":"shell.exec","params":{"cmd":"rm -rf /"}}"#)
            .await;
    assert_eq!(unknown["error"]["code"], "UNKNOWN_METHOD");

    // Aşırı büyük istek satırı: hata döner ve bağlantı kapanır; daemon ayakta kalır.
    let huge = format!(
        r#"{{"id":5,"method":"task.create","params":{{"title":"{}"}}}}"#,
        "x".repeat(server::MAX_REQUEST_BYTES + 10)
    );
    let stream2 = UnixStream::connect(&socket).await.unwrap();
    let (r2, mut w2) = stream2.into_split();
    let mut lines2 = BufReader::new(r2).lines();
    let too_big = call(&mut w2, &mut lines2, &huge).await;
    assert_eq!(too_big["error"]["code"], "BAD_REQUEST");
    assert!(lines2.next_line().await.unwrap().is_none(), "bağlantı kapatılmalı");
    let still = call(&mut w, &mut lines, r#"{"id":6,"method":"health.check"}"#).await;
    assert_eq!(still["result"]["ok"], true);

    let _ = tx.send(true);
    server_task.await.unwrap();
    assert!(!socket.exists(), "kapanışta socket temizlenmeli");
}
