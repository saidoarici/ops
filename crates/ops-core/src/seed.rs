//! Geliştirme ve ekran görüntüleri için kurgusal demo çalışma alanı.
//! Yalnızca açık kullanıcı komutuyla yüklenir (`personal-opsd seed-demo`);
//! production'da otomatik çalışmaz. Tüm adlar ve içerikler kurgusaldır.

use chrono::Duration;
use rusqlite::params;

use crate::models::{
    Ctx, ProjectCreate, ReminderCreate, RepeatRule, TaskCreate, TaskSource, TaskStatus,
};
use crate::store::Store;
use crate::{time, OpsError, Result};

pub struct SeedReport {
    pub projects: usize,
    pub tasks: usize,
    pub reminders: usize,
}

pub fn seed_demo(store: &Store, force: bool) -> Result<SeedReport> {
    if !store.list_projects(true)?.is_empty() && !force {
        return Err(OpsError::Conflict(
            "veritabanı boş değil; demo verisi için --force kullan".into(),
        ));
    }
    let ctx = Ctx::CLI;
    let now = time::now();

    let atlas = store.create_project(
        &ctx,
        ProjectCreate {
            name: "Atlas CRM".into(),
            description: Some(
                "Müşteri yönetimi web uygulaması — bildirim ve yetki sistemleri".into(),
            ),
            priority: Some(5),
            keywords: Some(vec!["atlas".into(), "crm".into()]),
            ..Default::default()
        },
    )?;
    let nova = store.create_project(
        &ctx,
        ProjectCreate {
            name: "Nova Mobil".into(),
            description: Some("iOS harcama takip uygulaması".into()),
            priority: Some(3),
            ..Default::default()
        },
    )?;
    let research = store.create_project(
        &ctx,
        ProjectCreate {
            name: "Pazar Araştırması".into(),
            description: Some("Rakip analizi ve fiyatlandırma çalışması".into()),
            priority: Some(4),
            ..Default::default()
        },
    )?;

    let task = |input: TaskCreate| store.create_task(&ctx, input);
    let mut created: Vec<crate::models::Task> = Vec::new();

    let in_progress = task(TaskCreate {
        title: "Atlas bildirim frontend entegrasyonu".into(),
        description: Some(
            "Backend hazır; frontend bildirim hook'u bağlanacak, uçtan uca test kaldı.".into(),
        ),
        project_id: Some(atlas.id.clone()),
        status: Some(TaskStatus::InProgress),
        importance: Some(5),
        urgency: Some(4),
        estimated_minutes: Some(45),
        ..Default::default()
    })?;
    created.push(task(TaskCreate {
        title: "Atlas yetki sistemi testleri".into(),
        project_id: Some(atlas.id.clone()),
        status: Some(TaskStatus::Next),
        importance: Some(4),
        urgency: Some(3),
        estimated_minutes: Some(60),
        ..Default::default()
    })?);
    let done = task(TaskCreate {
        title: "Atlas backend bildirim servisi".into(),
        project_id: Some(atlas.id.clone()),
        status: Some(TaskStatus::Done),
        importance: Some(4),
        ..Default::default()
    })?;
    let stale = task(TaskCreate {
        title: "Nova senkronizasyon hatası".into(),
        description: Some(
            "Banka API'sinden çift kayıt geliyor; tekilleştirme mantığı yarım kaldı.".into(),
        ),
        project_id: Some(nova.id.clone()),
        status: Some(TaskStatus::InProgress),
        importance: Some(3),
        urgency: Some(2),
        ..Default::default()
    })?;
    created.push(task(TaskCreate {
        title: "Nova App Store ekran görüntüleri".into(),
        project_id: Some(nova.id.clone()),
        status: Some(TaskStatus::Inbox),
        source: Some(TaskSource::QuickCapture),
        ..Default::default()
    })?);
    created.push(task(TaskCreate {
        title: "Rakip fiyat tablosunu tamamla".into(),
        description: Some("Beş rakipten üçü işlendi; ikisi kaldı.".into()),
        project_id: Some(research.id.clone()),
        status: Some(TaskStatus::InProgress),
        importance: Some(4),
        urgency: Some(3),
        ..Default::default()
    })?);
    created.push(task(TaskCreate {
        title: "Rakip UX incelemesi".into(),
        project_id: Some(research.id.clone()),
        status: Some(TaskStatus::Someday),
        ..Default::default()
    })?);
    let waiting_contract = task(TaskCreate {
        title: "Ortaklık sözleşmesi dönüşü".into(),
        status: Some(TaskStatus::Waiting),
        waiting_for: Some("Hukuk ekibi — sözleşme taslağı".into()),
        followup_at: Some(now + Duration::hours(3)),
        importance: Some(4),
        urgency: Some(3),
        estimated_minutes: Some(5),
        ..Default::default()
    })?;
    let waiting_accounting = task(TaskCreate {
        title: "Muhasebe mutabakat cevabı".into(),
        status: Some(TaskStatus::Waiting),
        waiting_for: Some("Muhasebe".into()),
        ..Default::default()
    })?;
    created.push(task(TaskCreate {
        title: "Apple Developer başvurusunu tamamla".into(),
        status: Some(TaskStatus::Next),
        due_at: Some(now + Duration::hours(8)),
        importance: Some(5),
        urgency: Some(5),
        estimated_minutes: Some(15),
        ..Default::default()
    })?);
    created.push(task(TaskCreate {
        title: "Vergi beyannamesi evraklarını gönder".into(),
        status: Some(TaskStatus::Planned),
        due_at: Some(now - Duration::days(2)),
        importance: Some(5),
        urgency: Some(5),
        ..Default::default()
    })?);
    created.push(task(TaskCreate {
        title: "Yatırımcı sunum taslağını hazırla".into(),
        description: Some("10 slayt; problem, çözüm, metrikler.".into()),
        status: Some(TaskStatus::Planned),
        scheduled_at: Some(now + Duration::days(1)),
        ..Default::default()
    })?);
    created.push(task(TaskCreate {
        title: "Konferans biletini al".into(),
        status: Some(TaskStatus::Inbox),
        source: Some(TaskSource::Telegram),
        ..Default::default()
    })?);
    created.push(task(TaskCreate {
        title: "Eski landing page'i arşivle".into(),
        status: Some(TaskStatus::Cancelled),
        ..Default::default()
    })?);
    let tasks = created.len() + 5;

    // Demoda "geçmiş" hissi: bazı zaman damgalarını geriye çek. Normal akışta
    // zamanlar store tarafından atanır; bu yalnızca seed'e özgü bir kısayoldur.
    {
        let conn = store.db.conn();
        let back = |days: i64| time::to_db(&(now - Duration::days(days)));
        conn.execute(
            "UPDATE tasks SET updated_at=?2, created_at=?3 WHERE id=?1",
            params![stale.id, back(5), back(9)],
        )?;
        conn.execute(
            "UPDATE tasks SET waiting_since=?2, created_at=?3, updated_at=?3 WHERE id=?1",
            params![waiting_contract.id, back(9), back(9)],
        )?;
        conn.execute(
            "UPDATE tasks SET waiting_since=?2, created_at=?3, updated_at=?3 WHERE id=?1",
            params![waiting_accounting.id, back(3), back(3)],
        )?;
        conn.execute(
            "UPDATE tasks SET completed_at=?2 WHERE id=?1",
            params![done.id, time::to_db(&(now - Duration::hours(20)))],
        )?;
        conn.execute(
            "UPDATE tasks SET updated_at=?2 WHERE id=?1",
            params![in_progress.id, time::to_db(&(now - Duration::hours(14)))],
        )?;
    }

    let reminders = [
        ReminderCreate {
            title: "Apple Developer başvurusu".into(),
            remind_at: now + Duration::hours(2),
            notes: Some("15 dakika sürer; kimlik kartını hazırla.".into()),
            task_id: None,
            repeat_rule: None,
            channels: None,
        },
        ReminderCreate {
            title: "Sözleşme takibi".into(),
            remind_at: now + Duration::hours(26),
            notes: Some("9 gündür cevap yok; kısa bir mesaj yeterli.".into()),
            task_id: Some(waiting_contract.id.clone()),
            repeat_rule: None,
            channels: None,
        },
        ReminderCreate {
            title: "Günlük plan gözden geçirme".into(),
            remind_at: now + Duration::hours(22),
            notes: None,
            task_id: None,
            repeat_rule: Some(RepeatRule::Daily),
            channels: None,
        },
    ];
    let reminder_count = reminders.len();
    for r in reminders {
        store.create_reminder(&ctx, r)?;
    }

    Ok(SeedReport { projects: 3, tasks, reminders: reminder_count })
}
