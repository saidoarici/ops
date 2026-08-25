//! Observer uçtan uca: gerçek (shell'siz) git reposu üzerinde tarama,
//! evidence üretimi, tespit yaşam döngüsü ve güvenlik sınırı.

use chrono::Duration;
use git2::{Repository, Signature};
use ops_core::models::*;
use ops_core::store::Store;
use ops_core::time;
use ops_observer::scan::{scan_project, scan_stale_tasks};

const CTX: Ctx = Ctx::LOCAL_USER;

fn commit_all(repo: &Repository, message: &str) -> git2::Oid {
    let sig = Signature::now("Test", "test@local").unwrap();
    let mut index = repo.index().unwrap();
    index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None).unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let parent =
        repo.head().ok().and_then(|h| h.target()).and_then(|oid| repo.find_commit(oid).ok());
    let parents: Vec<&git2::Commit> = parent.iter().collect();
    repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents).unwrap()
}

fn setup() -> (Store, tempfile::TempDir, Repository, Project) {
    let store = Store::in_memory().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let repo = Repository::init(tmp.path()).unwrap();
    let project = store
        .create_project(
            &CTX,
            ProjectCreate {
                name: "Atlas CRM".into(),
                priority: Some(5),
                local_paths: Some(vec![tmp.path().display().to_string()]),
                ..Default::default()
            },
        )
        .unwrap();
    (store, tmp, repo, project)
}

#[test]
fn scan_produces_commit_evidence_and_tracks_dirty() {
    let (store, tmp, repo, project) = setup();
    let now = time::now();

    // 1) commit → GIT_COMMIT evidence + repo state
    std::fs::write(tmp.path().join("main.rs"), "fn main() {}").unwrap();
    let oid = commit_all(&repo, "notification backend eklendi");
    scan_project(&store, &project, now).unwrap();

    let evidence = store.list_evidence(&EvidenceFilter::default()).unwrap();
    assert!(
        evidence.iter().any(|e| e.kind == EvidenceType::GitCommit
            && e.summary.contains("notification backend eklendi")),
        "commit evidence üretilmeli"
    );
    let states = store.list_repo_states(&project.id).unwrap();
    assert_eq!(states.len(), 1);
    assert_eq!(states[0].head_commit.as_deref(), Some(oid.to_string().as_str()));
    assert_eq!(states[0].dirty_files, 0);

    // idempotent: ikinci scan aynı commit'i tekrar yazmaz
    scan_project(&store, &project, now).unwrap();
    let evidence2 = store.list_evidence(&EvidenceFilter::default()).unwrap();
    assert_eq!(
        evidence.iter().filter(|e| e.kind == EvidenceType::GitCommit).count(),
        evidence2.iter().filter(|e| e.kind == EvidenceType::GitCommit).count()
    );

    // 2) dosya değişikliği → dirty + temiz→kirli FILE_CHANGE evidence
    std::fs::write(tmp.path().join("lib.rs"), "pub fn x() {}").unwrap();
    scan_project(&store, &project, now).unwrap();
    let st = &store.list_repo_states(&project.id).unwrap()[0];
    assert_eq!(st.dirty_files, 1);
    assert!(st.dirty_since.is_some());
    assert!(store
        .list_evidence(&EvidenceFilter::default())
        .unwrap()
        .iter()
        .any(|e| e.kind == EvidenceType::FileChange));

    // proje aktivitesi commit zamanına damgalanmış olmalı
    let p = store.get_project(&project.id).unwrap();
    assert!(p.last_activity_at.is_some());
}

#[test]
fn uncommitted_detection_lifecycle_open_resolve() {
    let (store, tmp, repo, project) = setup();
    let now = time::now();

    std::fs::write(tmp.path().join("a.txt"), "x").unwrap();
    commit_all(&repo, "ilk");
    std::fs::write(tmp.path().join("b.txt"), "yarım iş").unwrap();
    scan_project(&store, &project, now).unwrap();

    // dirty_since'i 2 gün geriye çek (daemon 2 gündür kirli görmüş gibi)
    let st = store.list_repo_states(&project.id).unwrap().remove(0);
    store
        .upsert_repo_state(&RepoState { dirty_since: Some(now - Duration::days(2)), ..st })
        .unwrap();
    scan_project(&store, &project, now).unwrap();

    let open = store.list_detected(false).unwrap();
    let det = open
        .iter()
        .find(|d| d.kind == DetectedKind::UncommittedChanges)
        .expect("uncommitted tespiti açılmalı");
    assert!(det.detail.contains("commit bekliyor"));
    assert_eq!(det.project_name.as_deref(), Some("Atlas CRM"));

    // Görev olarak ekleyeyim mi? → dönüştür (kullanıcı onayı simülasyonu)
    let task = store.convert_detected(&CTX, &det.id).unwrap();
    assert_eq!(task.source, TaskSource::AiDetected);
    assert_eq!(task.project_id.as_deref(), Some(project.id.as_str()));

    // commit edilince sinyal kaybolur → başka açık tespit kalmaz;
    // CONVERTED kayıt sistem tarafından yeniden açılmaz
    commit_all(&repo, "b tamam");
    scan_project(&store, &project, time::now()).unwrap();
    let after = store.list_detected(true).unwrap();
    let d = after.iter().find(|d| d.kind == DetectedKind::UncommittedChanges).unwrap();
    assert_eq!(d.status, DetectedStatus::Converted);
    assert!(store.list_detected(false).unwrap().is_empty());
}

#[test]
fn stale_task_detection_and_resolution() {
    let (store, _tmp, _repo, project) = setup();
    let task = store
        .create_task(
            &CTX,
            TaskCreate {
                title: "Frontend entegrasyonu".into(),
                project_id: Some(project.id.clone()),
                status: Some(TaskStatus::InProgress),
                ..Default::default()
            },
        )
        .unwrap();

    // henüz taze → tespit yok
    scan_stale_tasks(&store, time::now()).unwrap();
    assert!(store.list_detected(false).unwrap().is_empty());

    // Görevi 6 gün "yaşlandırmak" için taramayı gelecekteki now ile çalıştır
    // (in-memory DB'de zaman damgasını geriye çekmek yerine eşdeğer yol):
    let future = time::now() + Duration::days(6);
    scan_stale_tasks(&store, future).unwrap();
    let open = store.list_detected(false).unwrap();
    let det = open.iter().find(|d| d.kind == DetectedKind::StaleTask).expect("stale tespiti");
    assert_eq!(det.task_id.as_deref(), Some(task.id.as_str()));
    assert!(det.detail.contains("aktivite yok"));

    // görev güncellenince (kullanıcı dokundu) sinyal kaybolur → RESOLVED
    store
        .update_task(&CTX, &task.id, TaskPatch { priority: Some(4), ..Default::default() })
        .unwrap();
    scan_stale_tasks(&store, time::now()).unwrap();
    assert!(store.list_detected(false).unwrap().is_empty());
    let all = store.list_detected(true).unwrap();
    assert_eq!(
        all.iter().find(|d| d.kind == DetectedKind::StaleTask).unwrap().status,
        DetectedStatus::Resolved
    );
}

#[test]
fn repo_outside_approved_roots_is_blocked() {
    // S3: git_repositories onaylı köklerin dışına işaret ediyorsa gözlemlenmez;
    // hata olarak raporlanır.
    let store = Store::in_memory().unwrap();
    let approved = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    Repository::init(outside.path()).unwrap();

    let project = store
        .create_project(
            &CTX,
            ProjectCreate {
                name: "Sınır Testi".into(),
                local_paths: Some(vec![approved.path().display().to_string()]),
                git_repositories: Some(vec![outside.path().display().to_string()]),
                ..Default::default()
            },
        )
        .unwrap();
    let out = scan_project(&store, &project, time::now()).unwrap();
    assert_eq!(out.repos, 0, "onaylı kök dışındaki repo taranmamalı");
    assert!(out.errors.iter().any(|e| e.contains("onaylı proje kökleri dışında")));
    assert!(store.list_repo_states(&project.id).unwrap().is_empty());
}

#[test]
fn dismissed_detection_stays_dismissed() {
    let (store, tmp, repo, project) = setup();
    let now = time::now();
    std::fs::write(tmp.path().join("a.txt"), "x").unwrap();
    commit_all(&repo, "ilk");
    std::fs::write(tmp.path().join("b.txt"), "y").unwrap();
    scan_project(&store, &project, now).unwrap();
    let st = store.list_repo_states(&project.id).unwrap().remove(0);
    store
        .upsert_repo_state(&RepoState { dirty_since: Some(now - Duration::days(2)), ..st })
        .unwrap();
    scan_project(&store, &project, now).unwrap();

    let det = store.list_detected(false).unwrap().remove(0);
    store.dismiss_detected(&CTX, &det.id).unwrap();

    // sinyal sürse de kullanıcı kararı ezilmez
    scan_project(&store, &project, now).unwrap();
    assert!(store.list_detected(false).unwrap().is_empty());
    let d = store.list_detected(true).unwrap().remove(0);
    assert_eq!(d.status, DetectedStatus::Dismissed);
}
