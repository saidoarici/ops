//! ops-core entegrasyon testleri.
//! Güvenlik regresyonları (docs/threat-model.md, "Regression gate"): S2 burada,
//! S3/S4 paths.rs unit testlerinde, S8 aşağıdaki tamper testinde, S9 daemon
//! UDS testinde.

use chrono::{Duration, FixedOffset, Utc};
use ops_core::models::*;
use ops_core::store::Store;
use ops_core::{seed, today};
use serde_json::json;

const CTX: Ctx = Ctx::LOCAL_USER;

fn store() -> Store {
    Store::in_memory().unwrap()
}

#[test]
fn task_fields_are_data_never_commands() {
    // S2: shell metakarakterleri içeren başlık aynen veri olarak saklanır,
    // hiçbir yerde yorumlanmaz, hiçbir process doğmaz.
    let s = store();
    let evil = "$(touch /tmp/pwned-personalops-test) ; rm -rf ~ && `id`";
    let t = s.create_task(&CTX, TaskCreate { title: evil.into(), ..Default::default() }).unwrap();
    assert_eq!(t.title, evil);
    assert!(
        !std::path::Path::new("/tmp/pwned-personalops-test").exists(),
        "S2 İHLALİ: task başlığı çalıştırılmış!"
    );
    let fetched = s.get_task(&t.id).unwrap();
    assert_eq!(fetched.title, evil);
}

#[test]
fn task_crud_patch_and_status_side_effects() {
    let s = store();
    let p = s
        .create_project(&CTX, ProjectCreate { name: "Atlas CRM".into(), ..Default::default() })
        .unwrap();
    let t = s
        .create_task(
            &CTX,
            TaskCreate { title: "Notification frontend".into(), ..Default::default() },
        )
        .unwrap();
    assert_eq!(t.status, TaskStatus::Inbox);
    assert_eq!(t.priority, 3);

    // PATCH: alan ekle
    let patch: TaskPatch = serde_json::from_value(json!({
        "projectId": p.id, "dueAt": "2026-09-01T10:00:00Z", "status": "IN_PROGRESS"
    }))
    .unwrap();
    let t2 = s.update_task(&CTX, &t.id, patch).unwrap();
    assert_eq!(t2.project_id.as_deref(), Some(p.id.as_str()));
    assert_eq!(t2.project_name.as_deref(), Some("Atlas CRM"));
    assert!(t2.due_at.is_some());
    assert_eq!(t2.status, TaskStatus::InProgress);

    // PATCH: null ile temizle
    let clear: TaskPatch = serde_json::from_value(json!({ "dueAt": null })).unwrap();
    let t3 = s.update_task(&CTX, &t.id, clear).unwrap();
    assert!(t3.due_at.is_none());

    // WAITING'e geçiş waiting_since damgalar
    let wait: TaskPatch =
        serde_json::from_value(json!({ "status": "WAITING", "waitingFor": "Hukuk ekibi" }))
            .unwrap();
    let t4 = s.update_task(&CTX, &t.id, wait).unwrap();
    assert!(t4.waiting_since.is_some());

    // DONE → completed_at dolu; WAITING'den çıkınca waiting_since temiz
    let t5 = s.complete_task(&CTX, &t.id).unwrap();
    assert!(t5.completed_at.is_some());
    assert!(t5.waiting_since.is_none());

    // Yeniden aç → completed_at temizlenir
    let reopen: TaskPatch = serde_json::from_value(json!({ "status": "NEXT" })).unwrap();
    let t6 = s.update_task(&CTX, &t.id, reopen).unwrap();
    assert!(t6.completed_at.is_none());

    // Arşiv soft delete'tir: default listede görünmez, kayıt durur
    s.archive_task(&CTX, &t.id).unwrap();
    let visible = s.list_tasks(&TaskFilter::default()).unwrap();
    assert!(visible.iter().all(|x| x.id != t.id));
    let all = s.list_tasks(&TaskFilter { include_archived: true, ..Default::default() }).unwrap();
    assert!(all.iter().any(|x| x.id == t.id));

    // Geçersiz değerler reddedilir
    assert!(s.create_task(&CTX, TaskCreate { title: "   ".into(), ..Default::default() }).is_err());
    assert!(s
        .create_task(
            &CTX,
            TaskCreate { title: "x".into(), priority: Some(9), ..Default::default() }
        )
        .is_err());
}

#[test]
fn project_duplicate_name_conflicts() {
    let s = store();
    s.create_project(&CTX, ProjectCreate { name: "Nova Mobil".into(), ..Default::default() })
        .unwrap();
    let dup =
        s.create_project(&CTX, ProjectCreate { name: "nova mobil".into(), ..Default::default() });
    assert!(matches!(dup, Err(ops_core::OpsError::Conflict(_))));
}

#[test]
fn audit_chain_verifies_and_detects_tampering() {
    // S8: dosya tabanlı DB'de zincir doğrulanır; ham SQL ile kayıt oynandığında
    // verify kırığı raporlar.
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("audit.db");
    {
        let s = Store::new(ops_core::db::Db::open(&db_path).unwrap());
        s.create_project(&CTX, ProjectCreate { name: "P1".into(), ..Default::default() }).unwrap();
        s.create_task(&CTX, TaskCreate { title: "a".into(), ..Default::default() }).unwrap();
        s.create_task(&CTX, TaskCreate { title: "b".into(), ..Default::default() }).unwrap();
        let report = s.verify_audit().unwrap();
        assert!(report.ok, "temiz zincir doğrulanmalı: {}", report.message);
        assert_eq!(report.checked, 3);
    }
    // Tahrifat: seq 2 kaydının action'ı değiştirilir
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute("UPDATE audit_events SET action = 'TASK_DELETE' WHERE seq = 2", []).unwrap();
    }
    let s = Store::new(ops_core::db::Db::open(&db_path).unwrap());
    let report = s.verify_audit().unwrap();
    assert!(!report.ok);
    assert_eq!(report.broken_at_seq, Some(2));
}

#[test]
fn reminders_fire_dismiss_and_repeat() {
    let s = store();
    let now = ops_core::time::now();

    let once = s
        .create_reminder(
            &CTX,
            ReminderCreate {
                title: "Tek seferlik".into(),
                remind_at: now - Duration::minutes(1),
                notes: None,
                task_id: None,
                repeat_rule: None,
                channels: None,
            },
        )
        .unwrap();
    let daily = s
        .create_reminder(
            &CTX,
            ReminderCreate {
                title: "Günlük".into(),
                remind_at: now - Duration::minutes(5),
                notes: None,
                task_id: None,
                repeat_rule: Some(RepeatRule::Daily),
                channels: None,
            },
        )
        .unwrap();

    let fired = s.fire_due_reminders(now).unwrap();
    assert_eq!(fired.len(), 2);

    let all = s.list_reminders(&ReminderFilter::default()).unwrap();
    let once_after = all.iter().find(|r| r.id == once.id).unwrap();
    assert_eq!(once_after.status, ReminderStatus::Fired);
    let daily_after = all.iter().find(|r| r.id == daily.id).unwrap();
    assert_eq!(daily_after.status, ReminderStatus::Scheduled);
    assert!(daily_after.remind_at > now, "tekrarlı hatırlatma ileri kurulmalı");

    // 24 saatten eski → MISSED
    let old = s
        .create_reminder(
            &CTX,
            ReminderCreate {
                title: "Çok eski".into(),
                remind_at: now - Duration::days(3),
                notes: None,
                task_id: None,
                repeat_rule: None,
                channels: None,
            },
        )
        .unwrap();
    let n = s.mark_missed_reminders(now).unwrap();
    assert_eq!(n, 1);
    let old_after = s
        .list_reminders(&ReminderFilter::default())
        .unwrap()
        .into_iter()
        .find(|r| r.id == old.id)
        .unwrap();
    assert_eq!(old_after.status, ReminderStatus::Missed);
}

#[test]
fn settings_allowlist_blocks_secrets() {
    let s = store();
    assert!(s.get_settings().unwrap().is_empty(), "taze DB'de ayar yok");

    s.set_setting(&CTX, "display_name", json!("Demo")).unwrap();
    assert_eq!(s.get_setting("display_name").unwrap(), Some(json!("Demo")));
    assert_eq!(s.get_settings().unwrap().get("display_name"), Some(&json!("Demo")));

    // Secret benzeri anahtar Keychain'e aittir, DB'ye yazılamaz (T14)
    let sec = s.set_setting(&CTX, "telegram_token", json!("abc"));
    assert!(matches!(sec, Err(ops_core::OpsError::Security(_))));
    let sec2 = s.set_setting_unaudited("whatsapp_api_key", json!("abc"));
    assert!(matches!(sec2, Err(ops_core::OpsError::Security(_))));
    // Bilinmeyen anahtar reddedilir (allowlist)
    let unknown = s.set_setting(&CTX, "random_key", json!(1));
    assert!(matches!(unknown, Err(ops_core::OpsError::Validation(_))));
    // Audit'siz yazım audit üretmez, audit'li yazım üretir
    let before = s.list_audit(50, None).unwrap().len();
    s.set_setting_unaudited("telegram_last_update_id", json!(42)).unwrap();
    assert_eq!(s.list_audit(50, None).unwrap().len(), before);
    s.set_setting(&CTX, "telegram_enabled", json!(true)).unwrap();
    assert_eq!(s.list_audit(50, None).unwrap().len(), before + 1);
}

#[test]
fn mutation_schemas_reject_unknown_fields() {
    // API sıkılığı: yazım şemaları bilinmeyen alanı sessizce yutmaz.
    assert!(
        serde_json::from_value::<TaskCreate>(json!({ "title": "x", "shell": "rm -rf" })).is_err()
    );
    assert!(serde_json::from_value::<TaskPatch>(json!({ "command": "ls" })).is_err());
    assert!(serde_json::from_value::<ProjectCreate>(json!({ "name": "p", "exec": true })).is_err());
    assert!(serde_json::from_value::<RoutinePatch>(json!({ "actionType": "RUN" })).is_err());
}

#[test]
fn seed_demo_and_today_view() {
    let s = store();
    let report = seed::seed_demo(&s, false).unwrap();
    assert_eq!(report.projects, 3);
    assert!(report.tasks >= 12);
    // İkinci seed korumalı
    assert!(matches!(seed::seed_demo(&s, false), Err(ops_core::OpsError::Conflict(_))));

    let offset = FixedOffset::east_opt(3 * 3600).unwrap(); // Europe/Istanbul (+03)
    let view = today::build(&s, Utc::now(), offset).unwrap();

    assert!(!view.focus.is_empty() && view.focus.len() <= 3);
    // Skorlar azalan sırada
    for w in view.focus.windows(2) {
        assert!(w[0].score >= w[1].score);
    }
    for f in &view.focus {
        assert!(!f.why_now.is_empty(), "her odak görevi why-now taşımalı");
    }
    // 9 gündür bekleyen sözleşme görevi dikkat listesinde olmalı
    assert!(
        view.needs_attention.iter().any(|a| matches!(a.kind, today::AttentionKind::WaitingLong)),
        "uzun bekleyen iş dikkat listesine düşmeli"
    );
    assert!(view.stats.overdue >= 1, "geciken vergi görevi sayılmalı");
    assert!(view.stats.open_tasks >= 10);
    // Timeline öğeleri gün sınırları içinde
    for item in &view.timeline {
        assert!(item.at >= view.day_start && item.at < view.day_end);
    }
}
