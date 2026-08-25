use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::{OpsError, Result};

/// Tüm veri yolları buradan türetilir. `PERSONAL_OPS_DATA_DIR` env değişkeni
/// (test/geliştirme için) kök dizini override eder.
pub fn data_dir() -> PathBuf {
    if let Ok(p) = std::env::var("PERSONAL_OPS_DATA_DIR") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    dirs::data_dir()
        .unwrap_or_else(|| {
            dirs::home_dir().expect("home dizini bulunamadı").join("Library/Application Support")
        })
        .join("PersonalOps")
}

pub fn db_path() -> PathBuf {
    data_dir().join("personalops.db")
}

pub fn socket_path() -> PathBuf {
    data_dir().join("daemon.sock")
}

pub fn backups_dir() -> PathBuf {
    data_dir().join("Backups")
}

pub fn logs_dir() -> PathBuf {
    dirs::home_dir().expect("home dizini bulunamadı").join("Library/Logs/PersonalOps")
}

/// Veri dizinlerini kurar ve kök dizini yalnızca kullanıcıya açar (0700).
pub fn ensure_data_dirs() -> Result<()> {
    let dir = data_dir();
    fs::create_dir_all(&dir)?;
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
    fs::create_dir_all(backups_dir())?;
    Ok(())
}

/// Güvenlik kapısı: `candidate`'ın (symlink'ler çözüldükten sonra) `root`
/// içinde kaldığını doğrular; kanonik yolu döner. Observer ve agent executor
/// dosya sistemine yalnızca bu fonksiyondan geçen yollarla dokunabilir.
/// `../../` kaçışlarını ve root dışına işaret eden symlink'leri engeller
/// (docs/threat-model.md T9/T10).
pub fn ensure_within(root: &Path, candidate: &Path) -> Result<PathBuf> {
    let root_c = root
        .canonicalize()
        .map_err(|e| OpsError::Security(format!("proje kökü çözümlenemedi: {e}")))?;
    let joined =
        if candidate.is_absolute() { candidate.to_path_buf() } else { root_c.join(candidate) };
    let cand_c =
        joined.canonicalize().map_err(|e| OpsError::Security(format!("yol çözümlenemedi: {e}")))?;
    if cand_c.starts_with(&root_c) {
        Ok(cand_c)
    } else {
        Err(OpsError::Security(format!(
            "proje kökü dışına erişim engellendi: {}",
            cand_c.display()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_within_allows_inside_and_blocks_escape() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("proj");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn main(){}").unwrap();

        // içeride: serbest
        let ok = ensure_within(&root, Path::new("src/main.rs")).unwrap();
        assert!(ok.ends_with("src/main.rs"));

        // ../ kaçışı: engelli (S3)
        fs::write(tmp.path().join("dis.txt"), "x").unwrap();
        let err = ensure_within(&root, Path::new("../dis.txt"));
        assert!(matches!(err, Err(OpsError::Security(_))));

        // mutlak yol root dışı: engelli
        let err = ensure_within(&root, Path::new("/etc/hosts"));
        assert!(matches!(err, Err(OpsError::Security(_))));
    }

    #[test]
    fn ensure_within_blocks_symlink_escape() {
        // S4: proje içindeki symlink root dışına işaret ediyorsa reddedilir.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("proj");
        fs::create_dir_all(&root).unwrap();
        let outside = tmp.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret.txt"), "gizli").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();

        let err = ensure_within(&root, Path::new("link/secret.txt"));
        assert!(matches!(err, Err(OpsError::Security(_))));
    }
}
