//! Zamanlayıcı: 30 saniyede bir vadesi gelen hatırlatmaları ve rutinleri
//! tetikler. Deterministik iş — AI yok. UI kapalıyken de çalışır.

use std::future::Future;
use std::sync::Arc;

use tokio::time::{interval, Duration, MissedTickBehavior};
use tracing::{error, info, warn};

use ops_core::time;

use crate::{notify, routines, AppState};

const TICK_SECS: u64 = 30;

pub async fn run(state: Arc<AppState>, shutdown: impl Future<Output = ()>) {
    // Daemon kapalıyken 24 saatten fazla gecikenler MISSED'e çekilir;
    // daha tazeler ilk tick'te gecikmeli tetiklenir (offline telafisi).
    match state.store.mark_missed_reminders(time::now()) {
        Ok(n) if n > 0 => info!(count = n, "kaçırılmış hatırlatmalar işaretlendi"),
        Ok(_) => {}
        Err(e) => warn!(error = %e, "kaçırılmış hatırlatma taraması başarısız"),
    }

    let mut ticker = interval(Duration::from_secs(TICK_SECS));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            _ = ticker.tick() => tick(&state).await,
        }
    }
    info!("scheduler kapandı");
}

async fn tick(state: &AppState) {
    match state.store.fire_due_reminders(time::now()) {
        Ok(fired) => {
            for reminder in &fired {
                let body = if reminder.notes.is_empty() {
                    "Personal Ops hatırlatması"
                } else {
                    &reminder.notes
                };
                notify::deliver(
                    state,
                    &reminder.channels,
                    &reminder.title,
                    body,
                    &format!("reminder:{}", reminder.id),
                )
                .await;
            }
        }
        Err(e) => error!(error = %e, "hatırlatma tetikleme hatası"),
    }
    routines::run_due_routines(state).await;
}
