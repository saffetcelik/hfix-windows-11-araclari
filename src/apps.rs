//! Yuklu programlar (Program Ekle/Kaldir listesi).

use crate::reg::{Key, HKCU, HKLM};
use std::os::windows::process::CommandExt;

const UNINSTALL: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";
const UNINSTALL32: &str = r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall";

#[derive(Clone)]
pub struct Program {
    pub name: String,
    pub publisher: String,
    pub version: String,
    pub size: u64,
    pub date: String,
    pub uninstall: String,
}

pub fn list() -> Vec<Program> {
    let mut out: Vec<Program> = Vec::new();
    for (root, path) in [(HKLM, UNINSTALL), (HKLM, UNINSTALL32), (HKCU, UNINSTALL)] {
        let Some(k) = Key::open(root, path, false) else {
            continue;
        };
        for sub in k.subkeys() {
            let Some(p) = Key::open(root, &format!("{path}\\{sub}"), false) else {
                continue;
            };
            let Some(name) = p.get_str("DisplayName").filter(|n| !n.trim().is_empty()) else {
                continue;
            };
            if p.get_u32("SystemComponent") == Some(1) || p.get_str("ParentKeyName").is_some() {
                continue;
            }
            let uninstall = p.get_str("UninstallString").unwrap_or_default();
            if uninstall.is_empty() {
                continue;
            }
            let version = p.get_str("DisplayVersion").unwrap_or_default();
            if out.iter().any(|o| o.name == name && o.version == version) {
                continue;
            }
            let d = p.get_str("InstallDate").unwrap_or_default();
            let date = if d.len() == 8 && d.chars().all(|c| c.is_ascii_digit()) {
                format!("{}.{}.{}", &d[6..8], &d[4..6], &d[0..4])
            } else {
                String::new()
            };
            out.push(Program {
                name,
                publisher: p.get_str("Publisher").unwrap_or_default(),
                version,
                size: p.get_u32("EstimatedSize").unwrap_or(0) as u64 * 1024,
                date,
                uninstall,
            });
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

/// Programin kendi kaldirma sihirbazini baslatir.
pub fn uninstall(p: &Program) -> bool {
    let mut cmd = p.uninstall.clone();
    let low = cmd.to_lowercase();
    if low.starts_with("msiexec") {
        if let Some(i) = low.find("/i{") {
            cmd.replace_range(i..i + 2, "/X");
        }
    }
    std::process::Command::new("cmd")
        .raw_arg(format!("/s /c \"{cmd}\""))
        .creation_flags(0x0800_0000)
        .spawn()
        .is_ok()
}
