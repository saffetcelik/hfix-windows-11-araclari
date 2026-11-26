//! Zamanlanmis gorevler: tum gorevlerin listesi ve guvenlik degerlendirmesi.

use crate::reg;
use crate::sig::{self, Risk};
use crate::startup::parse_exe;
use crate::tr;
use crate::win;
use std::path::Path;

#[derive(Clone)]
pub struct Task {
    pub name: String,
    /// Gorev Zamanlayici yolu, or. \Klasor\Ad
    pub path: String,
    pub microsoft: bool,
    pub enabled: bool,
    pub triggers: String,
    pub command: String,
    pub exe: String,
    pub author: String,
    pub hidden: bool,
    pub highest: bool,
    pub risk: Risk,
    pub trust: String,
}

const SCRIPT_HOSTS: &[&str] = &[
    "powershell", "pwsh", "cmd", "wscript", "cscript", "mshta", "rundll32", "regsvr32",
    "bitsadmin", "certutil", "curl", "wmic", "forfiles", "msiexec", "conhost", "bash",
];

/// Zararli gorevlerde sik gorulen komut parcalari.
/// (aranan parca, Turkce neden, Ingilizce neden)
const RED: &[(&str, &str, &str)] = &[
    ("-enc ", "kodlanmış komut", "encoded command"),
    ("encodedcommand", "kodlanmış komut", "encoded command"),
    ("frombase64string", "kodlanmış komut", "encoded command"),
    ("downloadstring", "internetten kod indiriyor", "downloads code from the internet"),
    ("downloadfile", "internetten dosya indiriyor", "downloads a file from the internet"),
    ("invoke-webrequest", "internetten indiriyor", "downloads from the internet"),
    ("iwr ", "internetten indiriyor", "downloads from the internet"),
    ("invoke-expression", "indirilen kodu çalıştırıyor", "runs downloaded code"),
    ("iex ", "indirilen kodu çalıştırıyor", "runs downloaded code"),
    ("iex(", "indirilen kodu çalıştırıyor", "runs downloaded code"),
    ("http://", "internet adresi içeriyor", "contains an internet address"),
    ("https://", "internet adresi içeriyor", "contains an internet address"),
    ("windowstyle hidden", "gizli pencere", "hidden window"),
    ("-w hidden", "gizli pencere", "hidden window"),
    ("javascript:", "satır içi betik", "inline script"),
    ("vbscript:", "satır içi betik", "inline script"),
    ("\\appdata\\local\\temp\\", "Temp klasöründen çalışıyor", "runs from the Temp folder"),
    ("\\users\\public\\", "Ortak klasörden çalışıyor", "runs from the Public folder"),
    ("\\windows\\temp\\", "Temp klasöründen çalışıyor", "runs from the Temp folder"),
];

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

fn unescape(s: &str) -> String {
    s.replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn triggers(xml: &str) -> String {
    let Some(a) = xml.find("<Triggers") else {
        return tr!("Tetikleyici yok", "No trigger").into();
    };
    let t = &xml[a..xml[a..].find("</Triggers>").map_or(xml.len(), |b| a + b)];
    let mut v: Vec<String> = Vec::new();
    for (key, label) in [
        ("<BootTrigger", tr!("Bilgisayar açılışı", "At startup")),
        ("<LogonTrigger", tr!("Oturum açılışı", "At sign-in")),
        ("<ScheduleByDay", tr!("Günlük", "Daily")),
        ("<ScheduleByWeek", tr!("Haftalık", "Weekly")),
        ("<ScheduleByMonth", tr!("Aylık", "Monthly")),
        ("<TimeTrigger", tr!("Belirli zamanda", "At a set time")),
        ("<IdleTrigger", tr!("Boştayken", "When idle")),
        ("<EventTrigger", tr!("Sistem olayında", "On a system event")),
        ("<SessionStateChangeTrigger", tr!("Oturum değişiminde", "On session change")),
        ("<RegistrationTrigger", tr!("Kaydedilince", "When registered")),
        ("<WnfStateChangeTrigger", tr!("Sistem durumu değişince", "On system state change")),
    ] {
        if t.contains(key) {
            v.push(label.into());
        }
    }
    if let Some(iv) = tag(t, "Interval") {
        // PT5M, PT1H, P1D
        let s = iv.trim_start_matches('P').trim_start_matches('T');
        let (num, unit) = s.split_at(s.len().saturating_sub(1));
        let unit = match unit {
            "M" => tr!("dk", "min"),
            "H" => tr!("saat", "h"),
            "S" => tr!("sn", "s"),
            "D" => tr!("gün", "d"),
            _ => "",
        };
        if !unit.is_empty() && num.chars().all(|c| c.is_ascii_digit()) {
            v.push(tr!("her {} {} yineler", "repeats every {} {}", num, unit));
        }
    }
    if v.is_empty() {
        tr!("Tetikleyici yok", "No trigger").into()
    } else {
        v.join(" · ")
    }
}

fn resolve(exe: &str) -> String {
    if exe.is_empty() || exe.contains('\\') {
        return exe.to_string();
    }
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
    let name = if Path::new(exe).extension().is_some() {
        exe.to_string()
    } else {
        format!("{exe}.exe")
    };
    for dir in ["System32", "System32\\WindowsPowerShell\\v1.0", ""] {
        let p = Path::new(&windir).join(dir).join(&name);
        if p.is_file() {
            return p.to_string_lossy().to_string();
        }
    }
    exe.to_string()
}

fn parse(name: String, path: String, xml: &str) -> Task {
    let settings = tag(xml, "Settings").unwrap_or("");
    let enabled = !settings.contains("<Enabled>false</Enabled>");
    let hidden = settings.contains("<Hidden>true</Hidden>");
    let highest = xml.contains("<RunLevel>HighestAvailable</RunLevel>");
    let mut author = unescape(tag(xml, "Author").unwrap_or("").trim());
    if author.starts_with("$(@") {
        author = "Microsoft".into();
    }
    let microsoft = path.to_lowercase().starts_with("\\microsoft\\");

    let (mut command, mut exe) = (String::new(), String::new());
    let mut com = false;
    if let Some(cmd) = tag(xml, "Command") {
        let cmd = unescape(cmd.trim());
        let args = unescape(tag(xml, "Arguments").unwrap_or("").trim());
        command = if cmd.contains(' ') && !cmd.starts_with('"') {
            format!("\"{cmd}\" {args}")
        } else {
            format!("{cmd} {args}")
        }
        .trim()
        .to_string();
        exe = resolve(&parse_exe(&reg::expand_env(&command)));
    } else if let Some(id) = tag(xml, "ClassId") {
        com = true;
        command = tr!("COM bileşeni {}", "COM component {}", id);
    }

    let (mut risk, mut trust) = (Risk::Unknown, String::new());
    if com {
        trust = tr!("Windows bileşeni (COM)", "Windows component (COM)").into();
        if microsoft {
            risk = Risk::Ok;
        }
    } else if !exe.is_empty() && Path::new(&exe).is_file() {
        let s = sig::verify_cached(&exe);
        let stem = Path::new(&exe)
            .file_stem()
            .map(|s| s.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let r = sig::assess(&stem, &exe, Some(&s));
        risk = r.0;
        trust = r.1;
        let low = command.to_lowercase();
        let script = SCRIPT_HOSTS.contains(&stem.as_str());
        if let Some(why) = RED
            .iter()
            .find(|r| low.contains(r.0))
            .filter(|_| script || risk >= Risk::Unsigned)
        {
            risk = Risk::Suspicious;
            trust = tr!(
                "Şüpheli komut: {}",
                "Suspicious command: {}",
                if crate::lang::en() { why.2 } else { why.1 }
            );
        } else if script && !microsoft && risk <= Risk::Ok {
            risk = Risk::Unsigned;
            trust = tr!("Betik / komut çalıştırıyor ({})", "Runs a script / command ({})", stem);
        }
        if hidden && !microsoft && risk == Risk::Unsigned {
            trust += tr!(" · gizli görev", " · hidden task");
        }
    } else if !exe.is_empty() {
        let missing = matches!(
            std::fs::metadata(&exe),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound
        );
        if missing {
            trust = tr!("Dosya bulunamadı (eski görev)", "File not found (stale task)").into();
            if !microsoft {
                risk = Risk::Unsigned;
            }
        }
    }

    Task {
        name,
        path,
        microsoft,
        enabled,
        triggers: triggers(xml),
        command,
        exe,
        author,
        hidden,
        highest,
        risk,
        trust,
    }
}

fn walk(out: &mut Vec<Task>, dir: &Path, rel: &str, depth: u32) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let p = e.path();
        let child = format!("{rel}\\{name}");
        if p.is_dir() {
            if depth < 8 {
                walk(out, &p, &child, depth + 1);
            }
            continue;
        }
        if let Some(xml) = read_text(&p) {
            if xml.contains("<Task") {
                out.push(parse(name, child, &xml));
            }
        }
    }
}

pub fn scan_dir(dir: &Path) -> Vec<Task> {
    let mut out = Vec::new();
    walk(&mut out, dir, "", 0);
    out.sort_by(|a, b| {
        b.risk
            .cmp(&a.risk)
            .then_with(|| a.microsoft.cmp(&b.microsoft))
            .then_with(|| a.path.to_lowercase().cmp(&b.path.to_lowercase()))
    });
    out
}

pub fn scan() -> Vec<Task> {
    // Test icin: gorevleri baska bir klasorden oku.
    if let Ok(d) = std::env::var("FB_TASKS") {
        return scan_dir(Path::new(&d));
    }
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
    scan_dir(&Path::new(&windir).join(r"System32\Tasks"))
}

pub fn set_enabled(t: &Task, on: bool) -> bool {
    let flag = if on { "/enable" } else { "/disable" };
    win::run_status("schtasks", &["/change", "/tn", &t.path, flag])
}
