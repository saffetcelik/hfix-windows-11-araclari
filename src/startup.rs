//! Baslangicta calisan her sey: Run anahtarlari, Baslangic klasorleri,
//! oturum acma gorevleri ve ucuncu taraf otomatik servisler.

use crate::lang::t;
use crate::reg::{self, Key, HKCU, HKLM};
use crate::sig::{self, Risk};
use crate::win;
use std::path::{Path, PathBuf};

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN32: &str = r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run";
const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved";
const SERVICES: &str = r"SYSTEM\CurrentControlSet\Services";

#[derive(Clone, PartialEq)]
pub enum Kind {
    Run { hklm: bool, approved: &'static str },
    Folder { hklm: bool },
    Task,
    Service,
}

#[derive(Clone)]
pub struct Item {
    pub name: String,
    /// Kayit degeri adi / gorev yolu / servis anahtari.
    pub id: String,
    pub command: String,
    pub source: &'static str,
    pub kind: Kind,
    pub enabled: bool,
    pub state: String,
    pub exe: String,
    pub risk: Risk,
    pub publisher: String,
}

fn windir() -> String {
    std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into())
}

/// Komut satirindan calistirilabilir dosyanin tam yolunu cikarir.
pub fn parse_exe(command: &str) -> String {
    let c = reg::expand_env(command.trim());
    let mut c = c.as_str();
    if let Some(s) = c.strip_prefix(r"\??\") {
        c = s;
    }
    let path = if let Some(rest) = c.strip_prefix('"') {
        rest.split('"').next().unwrap_or("").to_string()
    } else {
        let low = c.to_lowercase();
        match low.find(".exe") {
            Some(i) => c[..i + 4].to_string(),
            None => c.split(' ').next().unwrap_or("").to_string(),
        }
    };
    let low = path.to_lowercase();
    if low.starts_with(r"\systemroot") {
        format!("{}{}", windir(), &path[11..])
    } else if low.starts_with(r"system32\") || low.starts_with(r"syswow64\") {
        format!("{}\\{}", windir(), path)
    } else {
        path
    }
}

fn finish(mut it: Item) -> Item {
    it.exe = parse_exe(&it.command);
    if !it.exe.is_empty() && Path::new(&it.exe).is_file() {
        let s = sig::verify_cached(&it.exe);
        let stem = Path::new(&it.exe)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let (risk, text) = sig::assess(&stem, &it.exe, Some(&s));
        it.risk = risk;
        it.publisher = text;
    } else if !it.exe.is_empty() && it.exe.contains('\\') {
        let missing = matches!(
            std::fs::metadata(&it.exe),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound
        );
        if missing {
            it.publisher = t("Dosya bulunamadı (eski kayıt)", "File not found (stale entry)").into();
        }
    }
    it
}

fn approved_state(hklm: bool, sub: &str, id: &str) -> bool {
    let root = if hklm { HKLM } else { HKCU };
    Key::open(root, &format!("{APPROVED}\\{sub}"), false)
        .and_then(|k| k.get(id))
        .map_or(true, |(_, d)| d.first().map_or(true, |b| b & 1 == 0))
}

fn run_items(out: &mut Vec<Item>, hklm: bool, path: &str, approved: &'static str, source: &'static str) {
    let root = if hklm { HKLM } else { HKCU };
    let Some(k) = Key::open(root, path, false) else {
        return;
    };
    for (name, ty, data) in k.values() {
        if name.is_empty() || (ty != 1 && ty != 2) {
            continue;
        }
        let enabled = approved_state(hklm, approved, &name);
        out.push(finish(Item {
            name: name.clone(),
            id: name,
            command: reg::to_string(&data),
            source,
            kind: Kind::Run { hklm, approved },
            enabled,
            state: String::new(),
            exe: String::new(),
            risk: Risk::Unknown,
            publisher: String::new(),
        }));
    }
}

fn folder_items(out: &mut Vec<Item>, hklm: bool, dir: PathBuf, source: &'static str) {
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return;
    };
    for e in rd.flatten() {
        let fname = e.file_name().to_string_lossy().to_string();
        if fname.eq_ignore_ascii_case("desktop.ini") {
            continue;
        }
        let enabled = approved_state(hklm, "StartupFolder", &fname);
        let mut it = Item {
            name: fname.trim_end_matches(".lnk").to_string(),
            id: fname,
            command: e.path().to_string_lossy().to_string(),
            source,
            kind: Kind::Folder { hklm },
            enabled,
            state: String::new(),
            exe: String::new(),
            risk: Risk::Unknown,
            publisher: String::new(),
        };
        if it.command.to_lowercase().ends_with(".exe") {
            it = finish(it);
        } else {
            it.publisher = t("Kısayol", "Shortcut").into();
        }
        out.push(it);
    }
}

fn read_text(p: &Path) -> Option<String> {
    let b = std::fs::read(p).ok()?;
    if b.len() >= 2 && b[0] == 0xFF && b[1] == 0xFE {
        let u: Vec<u16> = b[2..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        Some(String::from_utf16_lossy(&u))
    } else {
        Some(String::from_utf8_lossy(&b).into_owned())
    }
}

fn tag<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    let a = xml.find(&format!("<{name}>"))? + name.len() + 2;
    let b = xml[a..].find(&format!("</{name}>"))? + a;
    Some(&xml[a..b])
}

fn task_items(out: &mut Vec<Item>, dir: &Path, rel: &str, depth: u32) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let p = e.path();
        let child = format!("{rel}\\{name}");
        if p.is_dir() {
            // Windows'un kendi gorevleri listelenmez.
            if depth == 0 && name.eq_ignore_ascii_case("Microsoft") {
                continue;
            }
            if depth < 6 {
                task_items(out, &p, &child, depth + 1);
            }
            continue;
        }
        let Some(xml) = read_text(&p) else {
            continue;
        };
        if !xml.contains("<LogonTrigger") && !xml.contains("<BootTrigger") {
            continue;
        }
        let enabled = tag(&xml, "Settings")
            .map_or(true, |s| !s.contains("<Enabled>false</Enabled>"));
        let cmd = tag(&xml, "Command").unwrap_or("").trim().to_string();
        let args = tag(&xml, "Arguments").unwrap_or("").trim();
        let command = if cmd.contains(' ') && !cmd.starts_with('"') {
            format!("\"{cmd}\" {args}")
        } else {
            format!("{cmd} {args}")
        };
        out.push(finish(Item {
            name,
            id: child,
            command: command.trim().to_string(),
            source: t("Zamanlanmış görev", "Scheduled task"),
            kind: Kind::Task,
            enabled,
            state: String::new(),
            exe: String::new(),
            risk: Risk::Unknown,
            publisher: String::new(),
        }));
    }
}

/// Ucuncu taraf (Windows klasoru disindan calisan) servisler.
pub fn services(include_manual: bool) -> Vec<Item> {
    let mut out = Vec::new();
    let Some(root) = Key::open(HKLM, SERVICES, false) else {
        return out;
    };
    let win = windir().to_lowercase();
    for name in root.subkeys() {
        let Some(k) = Key::open(HKLM, &format!("{SERVICES}\\{name}"), false) else {
            continue;
        };
        let ty = k.get_u32("Type").unwrap_or(0);
        if ty & 0x30 == 0 {
            continue; // surucu
        }
        let start = k.get_u32("Start").unwrap_or(3);
        if start < 2 || (start == 3 && !include_manual) {
            continue;
        }
        let Some(image) = k.get_str("ImagePath") else {
            continue;
        };
        let exe = parse_exe(&image);
        if exe.to_lowercase().starts_with(&win) {
            continue;
        }
        let disp = k
            .get_str("DisplayName")
            .filter(|d| !d.starts_with('@') && !d.is_empty())
            .unwrap_or_else(|| name.clone());
        out.push(Item {
            name: disp,
            id: name,
            command: image,
            source: t("Servis", "Service"),
            kind: Kind::Service,
            enabled: start == 2,
            state: match start {
                2 => t("Otomatik", "Automatic").into(),
                3 => t("Elle", "Manual").into(),
                _ => t("Devre dışı", "Disabled").into(),
            },
            exe,
            risk: Risk::Unknown,
            publisher: String::new(),
        });
    }
    out
}

pub fn scan() -> Vec<Item> {
    let mut out = Vec::new();
    run_items(&mut out, false, RUN, "Run", t("Kayıt (kullanıcı)", "Registry (user)"));
    run_items(&mut out, true, RUN, "Run", t("Kayıt (tüm kullanıcılar)", "Registry (all users)"));
    run_items(&mut out, true, RUN32, "Run32", t("Kayıt (32 bit)", "Registry (32-bit)"));
    if let Ok(a) = std::env::var("APPDATA") {
        folder_items(
            &mut out,
            false,
            Path::new(&a).join(r"Microsoft\Windows\Start Menu\Programs\Startup"),
            t("Başlangıç klasörü", "Startup folder"),
        );
    }
    if let Ok(a) = std::env::var("ProgramData") {
        folder_items(
            &mut out,
            true,
            Path::new(&a).join(r"Microsoft\Windows\Start Menu\Programs\Startup"),
            t("Başlangıç klasörü (ortak)", "Startup folder (shared)"),
        );
    }
    task_items(
        &mut out,
        &Path::new(&windir()).join(r"System32\Tasks"),
        "",
        0,
    );
    for it in services(true) {
        out.push(finish(it));
    }
    out
}

fn filetime_now() -> [u8; 8] {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    ((secs + 11_644_473_600) * 10_000_000).to_le_bytes()
}

pub fn set_enabled(it: &Item, on: bool) -> Result<(), String> {
    match &it.kind {
        Kind::Run { hklm, approved } => write_approved(*hklm, approved, &it.id, on),
        Kind::Folder { hklm } => write_approved(*hklm, "StartupFolder", &it.id, on),
        Kind::Task => {
            let flag = if on { "/enable" } else { "/disable" };
            if win::run_status("schtasks", &["/change", "/tn", &it.id, flag]) {
                Ok(())
            } else {
                Err(t(
                    "görev değiştirilemedi (yönetici gerekebilir)",
                    "could not change the task (administrator may be needed)",
                )
                .into())
            }
        }
        Kind::Service => {
            let mode = if on { "auto" } else { "demand" };
            if win::run_status("sc", &["config", &it.id, "start=", mode]) {
                Ok(())
            } else {
                Err(t(
                    "servis ayarı değiştirilemedi (yönetici gerekebilir)",
                    "could not change the service setting (administrator may be needed)",
                )
                .into())
            }
        }
    }
}

/// Baslangic kaydini kalici olarak siler (dosyanin kendisine dokunmaz;
/// Baslangic klasorundeki kisayol/dosya haric).
pub fn remove(it: &Item) -> bool {
    match &it.kind {
        Kind::Run { hklm, approved } => {
            let root = if *hklm { HKLM } else { HKCU };
            let path = if *approved == "Run32" { RUN32 } else { RUN };
            Key::open(root, path, true).is_some_and(|k| k.delete_value(&it.id))
        }
        Kind::Folder { .. } => std::fs::remove_file(&it.command).is_ok(),
        Kind::Task => win::run_status("schtasks", &["/delete", "/tn", &it.id, "/f"]),
        Kind::Service => win::run_status("sc", &["delete", &it.id]),
    }
}

fn write_approved(hklm: bool, sub: &str, id: &str, on: bool) -> Result<(), String> {
    let root = if hklm { HKLM } else { HKCU };
    let k = Key::create(root, &format!("{APPROVED}\\{sub}"))
        .ok_or(t(
            "kayıt anahtarı açılamadı (yönetici gerekebilir)",
            "could not open the registry key (administrator may be needed)",
        ))?;
    let mut data = [0u8; 12];
    if on {
        data[0] = 2;
    } else {
        data[0] = 3;
        data[4..12].copy_from_slice(&filetime_now());
    }
    if k.set_bin(id, &data) {
        Ok(())
    } else {
        Err(t("kayıt yazılamadı", "could not write to the registry").into())
    }
}
