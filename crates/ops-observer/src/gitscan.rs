//! git2 (libgit2) ile SALT OKUNUR repo anlık görüntüsü.
//! Shell çağrısı yoktur; network transportları derleme düzeyinde kapalıdır.

use std::path::Path;

use chrono::{DateTime, Utc};
use git2::{BranchType, Repository, StatusOptions};

#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub id: String,
    pub summary: String,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct GitSnapshot {
    pub branch: Option<String>,
    pub head: Option<String>,
    pub dirty_files: i64,
    pub last_commit_at: Option<DateTime<Utc>>,
    /// Upstream'e göre push'lanmamış commit sayısı (upstream yoksa 0).
    pub ahead: i64,
    /// `prev_head`'den bu yana yeni commit'ler (en yeni önce, en fazla 20).
    pub new_commits: Vec<CommitInfo>,
}

fn commit_time(t: git2::Time) -> DateTime<Utc> {
    DateTime::from_timestamp(t.seconds(), 0).unwrap_or_else(Utc::now)
}

fn short(s: Option<&str>) -> String {
    let line = s.unwrap_or("(mesaj yok)").lines().next().unwrap_or("").trim();
    let mut out: String = line.chars().take(200).collect();
    if out.is_empty() {
        out = "(mesaj yok)".into();
    }
    out
}

pub fn snapshot(repo_root: &Path, prev_head: Option<&str>) -> Result<GitSnapshot, git2::Error> {
    let repo = Repository::open(repo_root)?;

    let head_ref = repo.head().ok();
    let branch =
        head_ref.as_ref().and_then(|h| h.shorthand()).filter(|s| *s != "HEAD").map(str::to_string);
    let head_oid = head_ref.as_ref().and_then(|h| h.target());

    let dirty_files = if repo.is_bare() {
        0
    } else {
        let mut opts = StatusOptions::new();
        opts.include_untracked(true).recurse_untracked_dirs(true).exclude_submodules(true);
        repo.statuses(Some(&mut opts))?
            .iter()
            .filter(|e| !e.status().is_ignored() && e.status() != git2::Status::CURRENT)
            .count() as i64
    };

    let mut last_commit_at = None;
    let mut new_commits = Vec::new();
    if let Some(oid) = head_oid {
        if let Ok(c) = repo.find_commit(oid) {
            last_commit_at = Some(commit_time(c.time()));
        }
        let mut walk = repo.revwalk()?;
        walk.push(oid)?;
        if let Some(prev) = prev_head.and_then(|p| git2::Oid::from_str(p).ok()) {
            if repo.find_commit(prev).is_ok() {
                // hide hata verirse (ör. rewrite) 20'lik tavan zaten korur
                let _ = walk.hide(prev);
            }
        }
        for item in walk.take(20) {
            let oid = item?;
            if let Ok(c) = repo.find_commit(oid) {
                new_commits.push(CommitInfo {
                    id: oid.to_string(),
                    summary: short(c.summary()),
                    at: commit_time(c.time()),
                });
            }
        }
    }

    let mut ahead = 0i64;
    if let (Some(oid), Some(b)) = (head_oid, branch.as_deref()) {
        if let Ok(local) = repo.find_branch(b, BranchType::Local) {
            if let Ok(up) = local.upstream() {
                if let Some(up_oid) = up.get().target() {
                    if let Ok((a, _behind)) = repo.graph_ahead_behind(oid, up_oid) {
                        ahead = a as i64;
                    }
                }
            }
        }
    }

    Ok(GitSnapshot {
        branch,
        head: head_oid.map(|o| o.to_string()),
        dirty_files,
        last_commit_at,
        ahead,
        new_commits,
    })
}

#[cfg(test)]
pub mod testutil {
    //! Testler için git2 ile (shell'siz) repo kurulumu.
    use std::path::Path;

    use git2::{Repository, Signature};

    pub fn init_repo(dir: &Path) -> Repository {
        Repository::init(dir).unwrap()
    }

    pub fn commit_all(repo: &Repository, message: &str) -> git2::Oid {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_tracks_commits_dirty_and_new() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let repo = testutil::init_repo(root);

        // boş repo: head yok, temiz
        let s0 = snapshot(root, None).unwrap();
        assert!(s0.head.is_none());
        assert_eq!(s0.dirty_files, 0);

        // dosya ekle → dirty
        std::fs::write(root.join("a.txt"), "merhaba").unwrap();
        let s1 = snapshot(root, None).unwrap();
        assert_eq!(s1.dirty_files, 1);

        // commit → head oluşur, temizlenir, yeni commit listelenir
        let first = testutil::commit_all(&repo, "ilk commit").to_string();
        let s2 = snapshot(root, None).unwrap();
        assert_eq!(s2.head.as_deref(), Some(first.as_str()));
        assert_eq!(s2.dirty_files, 0);
        assert_eq!(s2.new_commits.len(), 1);
        assert_eq!(s2.new_commits[0].summary, "ilk commit");

        // ikinci commit; prev_head verilince yalnızca yenisi gelir
        std::fs::write(root.join("b.txt"), "dünya").unwrap();
        testutil::commit_all(&repo, "ikinci commit");
        let s3 = snapshot(root, Some(&first)).unwrap();
        assert_eq!(s3.new_commits.len(), 1);
        assert_eq!(s3.new_commits[0].summary, "ikinci commit");
        assert!(s3.last_commit_at.is_some());
    }
}
