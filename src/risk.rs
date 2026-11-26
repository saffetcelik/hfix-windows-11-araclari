//! Risk analizi: suphe uyandiran program, baslangic ogesi, servis, gorev,
//! dosya ve sistem ayarlarini tek listede toplar; karantina ve silme.

use crate::lang::t;
use crate::reg::{Key, HKCU, HKLM};
use crate::sig::{self, Risk, Status};
use crate::startup::{self, Item};
use crate::svc::{self, Service};
use crate::tasks::{self, Task};
use crate::tr;
use crate::win;
use std::collections::HashSet;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub enum Origin {
    Process,
    Startup(Item),
    Service(Service),
    Task(Task),
    File,
    /// Sistem ayari: (acilacak arac, parametre).
    Setting(String, String),
}

#[derive(Clone)]
pub struct Finding {
    pub key: String,
    pub level: Risk,
    pub name: String,
    pub reason: String,
    pub path: String,
    pub origins: Vec<Origin>,
    /// Defender sonucu: (tehdit var mi, aciklama).
    pub verdict: Option<(bool, String)>,
}

impl Finding {
    pub fn kinds(&self) -> String {
        let mut v: Vec<&str> = Vec::new();
        for o in &self.origins {
            let kind = match o {
                Origin::Process => t("Çalışıyor", "Running"),
                Origin::Startup(_) => t("Başlangıç", "Startup"),
                Origin::Service(_) => t("Servis", "Service"),
                Origin::Task(_) => t("Zamanlanmış görev", "Scheduled task"),
                Origin::File => t("Dosya", "File"),
                Origin::Setting(..) => t("Sistem ayarı", "System setting"),
            };
            if !v.contains(&kind) {
                v.push(kind);
            }
        }
        v.join(" · ")
    }

    pub fn setting(&self) -> Option<(&str, &str)> {
        self.origins.iter().find_map(|o| match o {
            Origin::Setting(a, b) => Some((a.as_str(), b.as_str())),
            _ => None,
        })
    }

    /// Karantinaya alinabilecek bir dosyasi var mi?
    pub fn has_file(&self) -> bool {
        removable(&self.path)
    }
}

fn windir() -> String {
    std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into())
}

fn in_windows(p: &str) -> bool {
    p.to_lowercase().starts_with(&windir().to_lowercase())
}

fn own_dir() -> Option<PathBuf> {
    crate::cfg::dir()
}

/// Windows dosyalarina ve bu programin kendi klasorune asla dokunulmaz.
fn removable(p: &str) -> bool {
    if p.is_empty() || in_windows(p) || !Path::new(p).is_file() {
        return false;
    }
    match own_dir() {
        Some(d) => !p.to_lowercase().starts_with(&d.to_string_lossy().to_lowercase()),
        None => false,
    }
}

fn add(out: &mut Vec<Finding>, key: &str, level: Risk, name: &str, reason: &str, path: &str, o: Origin) {
    let k = key.to_lowercase();
    if let Some(f) = out.iter_mut().find(|f| f.key == k) {
        if level > f.level {
            f.level = level;
            f.reason = reason.to_string();
        }
        f.origins.push(o);
    } else {
        out.push(Finding {
            key: k,
            level,
            name: name.to_string(),
            reason: reason.to_string(),
            path: path.to_string(),
            origins: vec![o],
            verdict: None,
        });
    }
}

// ------------------------------------------------------------------ tarama

const EXE: &[&str] = &["exe", "scr", "com", "pif"];
const SCRIPT: &[&str] = &["vbs", "vbe", "js", "jse", "wsf", "bat", "cmd", "ps1", "hta"];
const DOC: &[&str] = &[
    "pdf", "doc", "docx", "xls", "xlsx", "jpg", "jpeg", "png", "txt", "zip", "rar", "mp4", "udf",
];

fn ext(p: &Path) -> String {
    p.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

fn walk(dir: &Path, depth: u32, out: &mut Vec<PathBuf>) {
    if out.len() > 4000 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let Ok(md) = e.metadata() else { continue };
        if md.file_attributes() & 0x400 != 0 {
            continue;
        }
        let p = e.path();
        if md.is_dir() {
            if depth > 0 {
                walk(&p, depth - 1, out);
            }
        } else {
            let x = ext(&p);
            if EXE.contains(&x.as_str()) || SCRIPT.contains(&x.as_str()) {
                out.push(p);
            }
        }
    }
}

fn double_ext(p: &Path) -> bool {
    let stem = p.file_stem().map(PathBuf::from).unwrap_or_default();
    DOC.contains(&ext(&stem).as_str())
}

fn files(out: &mut Vec<Finding>, progress: &dyn Fn(&str)) {
    let env = |n: &str| std::env::var(n).ok().map(PathBuf::from);
    let mut dirs: Vec<(PathBuf, u32, &str, bool)> = Vec::new();
    if let Some(d) = env("TEMP") {
        dirs.push((d, 3, t("Temp klasöründe", "in the Temp folder"), false));
    }
    dirs.push((PathBuf::from(windir()).join("Temp"), 2, t("Windows\\Temp klasöründe", "in the Windows\\Temp folder"), false));
    if let Some(d) = env("PUBLIC") {
        dirs.push((d, 3, t("Ortak (Public) klasöründe", "in the Public folder"), true));
    }
    if let Some(d) = env("APPDATA") {
        dirs.push((
            d.join(r"Microsoft\Windows\Start Menu\Programs\Startup"),
            0,
            t("Başlangıç klasöründe", "in the Startup folder"),
            true,
        ));
        dirs.push((d, 1, t("AppData\\Roaming içinde", "in AppData\\Roaming"), false));
    }
    if let Some(d) = env("ProgramData") {
        dirs.push((
            d.join(r"Microsoft\Windows\Start Menu\Programs\Startup"),
            0,
            t("Ortak Başlangıç klasöründe", "in the all-users Startup folder"),
            true,
        ));
        dirs.push((d, 0, t("ProgramData kökünde", "in the ProgramData root"), true));
    }
    let own = own_dir().map(|d| d.to_string_lossy().to_lowercase());
    let mut checked = 0;
    for (dir, depth, place, scripts) in dirs {
        progress(&tr!("Dosyalar taranıyor: {}", "Scanning files: {}", dir.display()));
        let mut list = Vec::new();
        walk(&dir, depth, &mut list);
        for p in list {
            let ps = p.to_string_lossy().to_string();
            if own.as_ref().is_some_and(|o| ps.to_lowercase().starts_with(o)) {
                continue;
            }
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let x = ext(&p);
            if SCRIPT.contains(&x.as_str()) {
                if double_ext(&p) {
                    add(out, &ps, Risk::Suspicious, &name, t("Belge gibi görünen betik (çift uzantı)", "Script that looks like a document (double extension)"), &ps, Origin::File);
                } else if scripts {
                    add(out, &ps, Risk::Unsigned, &name, &tr!("{} betik dosyası", "Script file {}", place), &ps, Origin::File);
                }
                continue;
            }
            if double_ext(&p) {
                add(out, &ps, Risk::Suspicious, &name, t("Belge gibi görünen program (çift uzantı)", "Program that looks like a document (double extension)"), &ps, Origin::File);
                continue;
            }
            if checked > 400 {
                continue;
            }
            checked += 1;
            match sig::verify_cached(&ps).status {
                Status::Invalid => add(out, &ps, Risk::Unsigned, &name, &tr!("{} imzası geçersiz program (imzalandıktan sonra değişmiş)", "Program with an invalid signature {} (changed after it was signed)", place), &ps, Origin::File),
                Status::Unsigned => {
                    let mut why = tr!("{} imzasız program", "Unsigned program {}", place);
                    // npm paketleri (esbuild, node-gyp vb.) imzasiz gelir; zararli demek degildir.
                    if ps.to_lowercase().contains("\\node_modules\\") {
                        why += t(
                            " — node_modules içinde: büyük olasılıkla bir geliştirici aracı (npm paketi)",
                            " — inside node_modules: most likely a developer tool (npm package)",
                        );
                    }
                    add(out, &ps, Risk::Unsigned, &name, &why, &ps, Origin::File)
                }
                _ => {}
            }
        }
    }
}

fn settings(out: &mut Vec<Finding>) {
    let reg = || Origin::Setting("regedit.exe".into(), String::new());
    // Program acilisini baska bir programa yonlendirme (IFEO Debugger).
    const IFEO: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options";
    if let Some(k) = Key::open(HKLM, IFEO, false) {
        // Gecerli bir programa yonlendirmeyen kayitlar: programi yalnizca engeller.
        let mut blocked: Vec<String> = Vec::new();
        for sub in k.subkeys() {
            if let Some(d) = Key::open(HKLM, &format!("{IFEO}\\{sub}"), false).and_then(|s| s.get_str("Debugger")) {
                let low = d.trim().to_lowercase();
                // taskkill / systray gibi bos hedefler: program yalnizca engellenmis (gizlilik ve guncelleme araclari yapar).
                let dummy = low.ends_with("\\taskkill.exe") || low.ends_with("\\systray.exe") || low == "taskkill.exe";
                if !low.is_empty() && (dummy || (!low.contains('\\') && !low.contains(".exe"))) {
                    blocked.push(sub);
                } else if !d.trim().is_empty() {
                    add(
                        out,
                        &format!("ifeo:{sub}"),
                        Risk::Suspicious,
                        &sub,
                        &tr!("Bu program açılınca yerine şu çalıştırılıyor: {}", "When this program is opened, this runs instead: {}", d),
                        &format!("HKLM\\{IFEO}\\{sub}"),
                        reg(),
                    );
                }
            }
        }
        if !blocked.is_empty() {
            add(
                out,
                "ifeo:blocked",
                Risk::Unsigned,
                &tr!("{} programın açılması engellenmiş", "Programs blocked from starting: {}", blocked.len()),
                &tr!(
                    "Bir araç şu programların çalışmasını kayıt defterinden engellemiş (genelde Windows Update ve telemetri engelleyen araçlar yapar; virüs belirtisi değildir): {}",
                    "A tool has blocked these programs from running through the registry (usually done by tools that block Windows Update and telemetry; not a sign of a virus): {}",
                    blocked.join(", ")
                ),
                &format!("HKLM\\{IFEO}"),
                reg(),
            );
        }
    }
    const WL: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon";
    if let Some(k) = Key::open(HKLM, WL, false) {
        if let Some(s) = k.get_str("Shell") {
            if !s.trim().eq_ignore_ascii_case("explorer.exe") {
                add(out, "winlogon:shell", Risk::Suspicious, t("Windows kabuğu", "Windows shell"), &tr!("Oturum açılışında Gezgin yerine/yanında şu çalışıyor: {}", "At sign-in, this runs instead of / alongside Explorer: {}", s), &format!("HKLM\\{WL}"), reg());
            }
        }
        if let Some(s) = k.get_str("Userinit") {
            let low = s.trim().trim_end_matches(',').to_lowercase();
            if !low.ends_with("\\userinit.exe") || low.contains(',') {
                add(out, "winlogon:userinit", Risk::Suspicious, t("Oturum başlatıcı", "Sign-in launcher"), &tr!("Userinit değeri değiştirilmiş: {}", "The Userinit value has been changed: {}", s), &format!("HKLM\\{WL}"), reg());
            }
        }
    }
    const WIN: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Windows";
    if let Some(k) = Key::open(HKLM, WIN, false) {
        let dlls = k.get_str("AppInit_DLLs").unwrap_or_default();
        if !dlls.trim().is_empty() && k.get_u32("LoadAppInit_DLLs") == Some(1) {
            add(out, "appinit", Risk::Suspicious, "AppInit_DLLs", &tr!("Her programa şu DLL yükleniyor: {}", "This DLL is loaded into every program: {}", dlls), &format!("HKLM\\{WIN}"), reg());
        }
    }
    // hosts dosyasi yonlendirmeleri
    let hosts = PathBuf::from(windir()).join(r"System32\drivers\etc\hosts");
    if let Ok(text) = std::fs::read_to_string(&hosts) {
        let lines: Vec<&str> = text
            .lines()
            .map(|l| l.trim().trim_start_matches('\u{feff}'))
            .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.to_lowercase().ends_with("localhost"))
            .collect();
        if !lines.is_empty() {
            add(
                out,
                "hosts",
                Risk::Unsigned,
                t("hosts dosyası", "hosts file"),
                &tr!(
                    "{} site adresi elle yönlendirilmiş (ör. {}). Siz ya da bir programınız eklemediyse inceleyin.",
                    "{} site addresses are redirected manually (e.g. {}). Review them if neither you nor one of your programs added them.",
                    lines.len(),
                    lines.iter().take(2).cloned().collect::<Vec<_>>().join(" ; ")
                ),
                &hosts.to_string_lossy(),
                Origin::Setting("notepad.exe".into(), hosts.to_string_lossy().to_string()),
            );
        }
    }
    // Microsoft Defender
    let sec = || Origin::Setting("windowsdefender:".into(), String::new());
    const DEF: &str = r"SOFTWARE\Microsoft\Windows Defender";
    let rt_off = Key::open(HKLM, &format!("{DEF}\\Real-Time Protection"), false)
        .and_then(|k| k.get_u32("DisableRealtimeMonitoring"))
        == Some(1);
    let pol_off = Key::open(HKLM, r"SOFTWARE\Policies\Microsoft\Windows Defender", false)
        .and_then(|k| k.get_u32("DisableAntiSpyware"))
        == Some(1);
    if rt_off || pol_off {
        add(out, "defender:off", Risk::Unsigned, "Microsoft Defender", t("Gerçek zamanlı koruma kapalı. Başka bir antivirüs kullanmıyorsanız açın.", "Real-time protection is off. Turn it on unless you use another antivirus."), t("Windows Güvenliği", "Windows Security"), sec());
    }
    if let Some(k) = Key::open(HKLM, &format!("{DEF}\\Exclusions\\Paths"), false) {
        let v: Vec<String> = k.values().into_iter().map(|x| x.0).filter(|n| !n.is_empty()).collect();
        if !v.is_empty() {
            add(
                out,
                "defender:excl",
                Risk::Unsigned,
                t("Defender tarama dışı klasörler", "Defender scan exclusions"),
                &tr!(
                    "Defender şu {} konumu taramıyor: {}. Siz eklemediyseniz kaldırın.",
                    "Defender skips these locations ({}): {}. Remove them if you did not add them.",
                    v.len(),
                    v.join(" ; ")
                ),
                t("Windows Güvenliği > Dışlamalar", "Windows Security > Exclusions"),
                sec(),
            );
        }
    }
    const INET: &str = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";
    if let Some(k) = Key::open(HKCU, INET, false) {
        if k.get_u32("ProxyEnable") == Some(1) {
            let srv = k.get_str("ProxyServer").unwrap_or_default();
            add(out, "proxy", Risk::Unsigned, t("İnternet proxy ayarı", "Internet proxy setting"), &tr!("Tüm web trafiği şu sunucu üzerinden geçiyor: {}", "All web traffic goes through this server: {}", srv), t("Ayarlar > Ağ > Proxy", "Settings > Network > Proxy"), Origin::Setting("ms-settings:network-proxy".into(), String::new()));
        }
    }
}

/// `procs`: (ad, dosya, risk, aciklama)
pub fn scan(procs: Vec<(String, String, Risk, String)>, progress: &dyn Fn(&str)) -> Vec<Finding> {
    let mut out = Vec::new();
    progress(t("Çalışan programlar denetleniyor…", "Checking running programs…"));
    for (name, exe, mut risk, mut why) in procs {
        // Imzasi henuz denetlenmemis surecleri burada denetle.
        if risk == Risk::Unknown && !exe.is_empty() && Path::new(&exe).is_file() {
            let s = sig::verify_cached(&exe);
            (risk, why) = sig::assess(&name, &exe, Some(&s));
        }
        if risk >= Risk::Unsigned && !exe.is_empty() {
            add(&mut out, &exe, risk, &name, &why, &exe, Origin::Process);
        }
    }
    progress(t("Başlangıç öğeleri denetleniyor…", "Checking startup items…"));
    for it in startup::scan() {
        if it.enabled && it.risk >= Risk::Unsigned && it.kind != startup::Kind::Service && it.kind != startup::Kind::Task {
            let (name, why, exe) = (it.name.clone(), it.publisher.clone(), it.exe.clone());
            add(&mut out, &exe, it.risk, &name, &why, &exe, Origin::Startup(it));
        }
    }
    progress(t("Servisler denetleniyor…", "Checking services…"));
    for s in svc::scan() {
        if s.risk >= Risk::Unsigned && (s.running || s.start != 4) {
            let (name, why, file) = (s.display.clone(), s.publisher.clone(), s.file.clone());
            add(&mut out, &file, s.risk, &name, &why, &file, Origin::Service(s));
        }
    }
    progress(t("Zamanlanmış görevler denetleniyor…", "Checking scheduled tasks…"));
    for task in tasks::scan() {
        if task.enabled && task.risk >= Risk::Unsigned {
            // Betik calistiran gorevlerde dosya Windows'un kendi araci olur; anahtar gorev yoludur.
            let key = if task.exe.is_empty() || in_windows(&task.exe) {
                format!("task:{}", task.path)
            } else {
                task.exe.clone()
            };
            let path = if key.starts_with("task:") { task.command.clone() } else { task.exe.clone() };
            let (name, why) = (task.name.clone(), task.trust.clone());
            add(&mut out, &key, task.risk, &name, &why, &path, Origin::Task(task));
        }
    }
    files(&mut out, progress);
    progress(t("Sistem ayarları denetleniyor…", "Checking system settings…"));
    settings(&mut out);
    let trusted = trusted();
    out.retain(|f| !trusted.contains(&f.key));
    out.sort_by(|a, b| {
        b.level
            .cmp(&a.level)
            .then_with(|| b.origins.len().cmp(&a.origins.len()))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    out
}

// ---------------------------------------------------------- guvenilir liste

fn list_file(name: &str) -> Option<PathBuf> {
    Some(own_dir()?.join(name))
}

pub fn trusted() -> HashSet<String> {
    list_file("guvenilir.txt")
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|t| t.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect())
        .unwrap_or_default()
}

pub fn trust(key: &str) {
    let mut t = trusted();
    t.insert(key.to_lowercase());
    if let Some(p) = list_file("guvenilir.txt") {
        let _ = std::fs::write(p, t.into_iter().collect::<Vec<_>>().join("\n"));
    }
}

pub fn clear_trusted() {
    if let Some(p) = list_file("guvenilir.txt") {
        let _ = std::fs::remove_file(p);
    }
}

// ---------------------------------------------------------------- karantina

#[derive(Clone)]
pub struct Held {
    pub stored: String,
    pub original: String,
    pub time: String,
}

fn qdir() -> Option<PathBuf> {
    let d = own_dir()?.join("karantina");
    std::fs::create_dir_all(&d).ok()?;
    Some(d)
}

pub fn held() -> Vec<Held> {
    let Some(d) = own_dir().map(|d| d.join("karantina")) else {
        return Vec::new();
    };
    std::fs::read_to_string(d.join("liste.txt"))
        .map(|t| {
            t.lines()
                .filter_map(|l| {
                    let mut p = l.split('\t');
                    Some(Held {
                        stored: p.next()?.to_string(),
                        original: p.next()?.to_string(),
                        time: p.next().unwrap_or("").to_string(),
                    })
                })
                .filter(|h| d.join(&h.stored).is_file())
                .collect()
        })
        .unwrap_or_default()
}

fn save_held(v: &[Held]) {
    if let Some(d) = qdir() {
        let t: String = v
            .iter()
            .map(|h| format!("{}\t{}\t{}\n", h.stored, h.original, h.time))
            .collect();
        let _ = std::fs::write(d.join("liste.txt"), t);
    }
}

fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    std::fs::copy(from, to)?;
    std::fs::remove_file(from).inspect_err(|_| {
        let _ = std::fs::remove_file(to);
    })
}

/// Dosyayi calistirilamaz bir adla karantina klasorune tasir.
fn quarantine(path: &str) -> Result<(), String> {
    let d = qdir().ok_or(t("karantina klasörü oluşturulamadı", "could not create the quarantine folder"))?;
    let name = Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let stored = format!("{secs}_{name}.karantina");
    move_file(Path::new(path), &d.join(&stored)).map_err(|e| tr!("dosya taşınamadı ({})", "could not move the file ({})", e))?;
    let mut v = held();
    v.push(Held {
        stored,
        original: path.to_string(),
        time: win::clock(),
    });
    save_held(&v);
    Ok(())
}

pub fn restore(h: &Held) -> Result<(), String> {
    let d = qdir().ok_or(t("karantina klasörü yok", "the quarantine folder is missing"))?;
    if Path::new(&h.original).exists() {
        return Err(t("asıl konumda aynı adlı dosya var", "a file with the same name exists in the original location").into());
    }
    move_file(&d.join(&h.stored), Path::new(&h.original)).map_err(|e| e.to_string())?;
    let v: Vec<Held> = held().into_iter().filter(|x| x.stored != h.stored).collect();
    save_held(&v);
    Ok(())
}

pub fn purge(h: &Held) -> Result<(), String> {
    let d = qdir().ok_or(t("karantina klasörü yok", "the quarantine folder is missing"))?;
    std::fs::remove_file(d.join(&h.stored)).map_err(|e| e.to_string())?;
    let v: Vec<Held> = held().into_iter().filter(|x| x.stored != h.stored).collect();
    save_held(&v);
    Ok(())
}

/// Bulguyu etkisiz kilar: sureci sonlandirir, baslangic / servis / gorev kaydini
/// kapatir (delete=true ise siler), dosyayi karantinaya alir ya da siler.
pub fn neutralize(f: &Finding, delete: bool, pids: &[u32]) -> Result<String, String> {
    let mut done: Vec<String> = Vec::new();
    for o in &f.origins {
        match o {
            Origin::Startup(it) => {
                let ok = if delete {
                    startup::remove(it)
                } else {
                    startup::set_enabled(it, false).is_ok()
                };
                if ok {
                    done.push(if delete { t("başlangıç kaydı silindi", "startup entry deleted") } else { t("başlangıç kaydı kapatıldı", "startup entry turned off") }.into());
                }
            }
            Origin::Service(s) => {
                if svc::turn_off(s, true) {
                    done.push(t("servis durduruldu ve devre dışı bırakıldı", "service stopped and disabled").into());
                }
                if delete && win::run_status("sc", &["delete", &s.name]) {
                    done.push(t("servis kaydı silindi", "service entry deleted").into());
                }
            }
            Origin::Task(task) => {
                let ok = if delete {
                    win::run_status("schtasks", &["/delete", "/tn", &task.path, "/f"])
                } else {
                    tasks::set_enabled(task, false)
                };
                if ok {
                    done.push(if delete { t("görev silindi", "task deleted") } else { t("görev devre dışı bırakıldı", "task disabled") }.into());
                }
            }
            _ => {}
        }
    }
    let killed = pids.iter().filter(|p| win::kill(**p)).count();
    if killed > 0 {
        done.push(tr!("{} süreç sonlandırıldı", "processes ended: {}", killed));
        std::thread::sleep(std::time::Duration::from_millis(900));
    }
    if removable(&f.path) {
        if delete {
            std::fs::remove_file(&f.path).map_err(|e| {
                tr!("dosya silinemedi ({}); {}", "could not delete the file ({}); {}", e, done.join(", "))
            })?;
            done.push(t("dosya kalıcı olarak silindi", "file deleted permanently").into());
        } else {
            quarantine(&f.path).map_err(|e| format!("{e}; {}", done.join(", ")))?;
            done.push(t("dosya karantinaya alındı", "file quarantined").into());
        }
    }
    if done.is_empty() {
        Err(t("hiçbir işlem yapılamadı (yönetici modu gerekebilir)", "nothing could be done (administrator mode may be required)").into())
    } else {
        Ok(done.join(", "))
    }
}

// ----------------------------------------------------------------- Defender

fn mpcmdrun() -> Option<PathBuf> {
    let pf = std::env::var("ProgramFiles").ok()?;
    let p = Path::new(&pf).join(r"Windows Defender\MpCmdRun.exe");
    p.is_file().then_some(p)
}

/// Dosyayi Microsoft Defender'a taratir. (Some(true)=tehdit, Some(false)=temiz, None=taranamadi)
pub fn defender_file(path: &str) -> (Option<bool>, String) {
    let Some(mp) = mpcmdrun() else {
        return (None, t("Defender bulunamadı", "Defender not found").into());
    };
    let Some((code, out)) = win::run_code(
        &mp.to_string_lossy(),
        &["-Scan", "-ScanType", "3", "-File", path, "-DisableRemediation"],
    ) else {
        return (None, t("Defender çalıştırılamadı", "Could not run Defender").into());
    };
    match code {
        0 => (Some(false), t("Defender: temiz", "Defender: clean").into()),
        2 => {
            let name = out
                .lines()
                .find(|l| l.trim_start().starts_with("Threat") && l.contains(':') && !l.contains("information"))
                .and_then(|l| l.split_once(':'))
                .map(|x| x.1.trim().to_string())
                .unwrap_or_else(|| t("tehdit", "threat").into());
            (Some(true), format!("Defender: {name}"))
        }
        _ => (None, t("Defender tarayamadı (kapalı olabilir)", "Defender could not scan (it may be turned off)").into()),
    }
}

/// Defender hizli taramasini baslatir ve bitmesini bekler.
pub fn defender_quick() -> String {
    let Some(mp) = mpcmdrun() else {
        return t("Microsoft Defender bulunamadı.", "Microsoft Defender not found.").into();
    };
    match win::run_code(&mp.to_string_lossy(), &["-Scan", "-ScanType", "1"]) {
        Some((0, _)) => t("Defender hızlı taraması bitti: tehdit bulunmadı.", "Defender quick scan finished: no threats found.").into(),
        Some((2, _)) => t(
            "Defender hızlı taraması bitti: TEHDİT BULUNDU. Windows Güvenliği > Koruma geçmişi'nden ayrıntıya bakın.",
            "Defender quick scan finished: THREAT FOUND. See Windows Security > Protection history for details.",
        )
        .into(),
        _ => t(
            "Defender hızlı taraması çalıştırılamadı (Defender kapalı ya da başka bir antivirüs etkin olabilir).",
            "Could not run the Defender quick scan (Defender may be off or another antivirus may be active).",
        )
        .into(),
    }
}
