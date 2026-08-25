//! launchd user agent kurulumu. Yalnızca `gui/<uid>` domain'i kullanılır —
//! sistem domain'i / root asla hedeflenmez. Bu komutlar yalnızca kullanıcının
//! açık CLI çağrısıyla çalışır.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{bail, Context};

pub const LABEL: &str = "com.personalops.daemon";

/// Tek doğruluk kaynağı: repo'daki plist şablonu ({{EXE}} / {{LOG}} doldurulur).
const TEMPLATE: &str =
    include_str!("../../../resources/launchd/com.personalops.daemon.plist.template");

fn plist_path() -> anyhow::Result<PathBuf> {
    Ok(dirs::home_dir()
        .context("home dizini bulunamadı")?
        .join("Library/LaunchAgents")
        .join(format!("{LABEL}.plist")))
}

fn current_uid() -> anyhow::Result<String> {
    let out = Command::new("/usr/bin/id").arg("-u").output().context("id -u çalıştırılamadı")?;
    if !out.status.success() {
        bail!("id -u başarısız");
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn render_plist(exe: &str, log: &str) -> String {
    TEMPLATE.replace("{{EXE}}", &xml_escape(exe)).replace("{{LOG}}", &xml_escape(log))
}

pub fn install() -> anyhow::Result<()> {
    let exe = std::env::current_exe()?.canonicalize()?;
    let logs_dir = ops_core::paths::logs_dir();
    fs::create_dir_all(&logs_dir)?;
    let log_file = logs_dir.join("daemon.log");
    let plist = plist_path()?;
    if let Some(parent) = plist.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&plist, render_plist(&exe.display().to_string(), &log_file.display().to_string()))?;

    let uid = current_uid()?;
    // Önce varsa eskisini kaldır (hata yoksayılır: kurulu olmayabilir).
    let _ = Command::new("/bin/launchctl")
        .args(["bootout", &format!("gui/{uid}")])
        .arg(&plist)
        .output();
    let out = Command::new("/bin/launchctl")
        .args(["bootstrap", &format!("gui/{uid}")])
        .arg(&plist)
        .output()
        .context("launchctl çalıştırılamadı")?;
    if !out.status.success() {
        bail!("launchctl bootstrap başarısız: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    println!("✓ launchd agent kuruldu: {}", plist.display());
    println!("  binary : {}", exe.display());
    println!("  log    : {}", log_file.display());
    println!("  kaldırmak için: personal-opsd uninstall-launchd");
    Ok(())
}

pub fn uninstall() -> anyhow::Result<()> {
    let plist = plist_path()?;
    let uid = current_uid()?;
    let _ = Command::new("/bin/launchctl")
        .args(["bootout", &format!("gui/{uid}")])
        .arg(&plist)
        .output();
    if plist.exists() {
        fs::remove_file(&plist)?;
    }
    println!("✓ launchd agent kaldırıldı");
    Ok(())
}

pub fn status() -> anyhow::Result<()> {
    let uid = current_uid()?;
    let out = Command::new("/bin/launchctl")
        .args(["print", &format!("gui/{uid}/{LABEL}")])
        .output()
        .context("launchctl çalıştırılamadı")?;
    if out.status.success() {
        let text = String::from_utf8_lossy(&out.stdout);
        let state = text
            .lines()
            .find(|l| l.trim_start().starts_with("state ="))
            .map(|l| l.trim().to_string())
            .unwrap_or_else(|| "state bilinmiyor".into());
        println!("✓ {LABEL} yüklü ({state})");
    } else {
        println!("✗ {LABEL} launchd'de kayıtlı değil (kurmak için: personal-opsd install-launchd)");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plist_renders_placeholders_and_escapes_xml() {
        let out = render_plist("/Users/x/a&b/personal-opsd", "/Users/x/Library/Logs/<d>.log");
        assert!(!out.contains("{{EXE}}") && !out.contains("{{LOG}}"));
        assert!(out.contains("<string>/Users/x/a&amp;b/personal-opsd</string>"));
        assert!(out.contains("&lt;d&gt;.log"));
        assert!(out.contains("<string>run</string>"));
        assert!(out.contains(LABEL));
    }
}
