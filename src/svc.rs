//! Windows servisleri: listeleme, gruplama, imza denetimi, toplu durdurma.
#![allow(non_snake_case)]

use crate::lang::t;
use crate::reg::{Key, HKLM};
use crate::sig::{self, Risk};
use crate::startup::parse_exe;
use crate::win;
use std::path::{Path, PathBuf};

const SERVICES: &str = r"SYSTEM\CurrentControlSet\Services";

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Group {
    Safe,
    Optional,
    Remote,
    Updater,
    Third,
    Vm,
    Core,
}

impl Group {
    pub const ALL: [Group; 7] = [
        Group::Safe,
        Group::Optional,
        Group::Remote,
        Group::Updater,
        Group::Third,
        Group::Vm,
        Group::Core,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Group::Safe => t("Gereksiz — güvenle kapatılabilir", "Unneeded — safe to turn off"),
            Group::Optional => t("Kullanmıyorsanız kapatılabilir", "Optional — turn off if you don't use them"),
            Group::Remote => t("Uzaktan erişim programları", "Remote access programs"),
            Group::Updater => t("Güncelleyiciler ve yardımcılar", "Updaters and helpers"),
            Group::Third => t("Diğer üçüncü taraf servisler", "Other third-party services"),
            Group::Vm => t("VMware (korunuyor)", "VMware (protected)"),
            Group::Core => t("Windows çekirdek servisleri (dokunmayın)", "Windows core system services (do not touch)"),
        }
    }

    pub fn locked(self) -> bool {
        matches!(self, Group::Vm | Group::Core)
    }
}

#[derive(Clone)]
pub struct Service {
    pub name: String,
    pub display: String,
    pub desc: String,
    pub running: bool,
    pub pid: u32,
    /// 2 otomatik, 3 elle, 4 devre disi.
    pub start: u32,
    pub delayed: bool,
    /// Imzasi denetlenen dosya (svchost servislerinde DLL).
    pub file: String,
    pub risk: Risk,
    pub publisher: String,
    pub group: Group,
    pub note: &'static str,
}

impl Service {
    pub fn start_text(&self) -> &'static str {
        match (self.start, self.delayed) {
            (2, true) => t("Otomatik (gecikmeli)", "Automatic (delayed)"),
            (2, false) => t("Otomatik", "Automatic"),
            (3, _) => t("Elle", "Manual"),
            (4, _) => t("Devre dışı", "Disabled"),
            _ => t("Sistem", "System"),
        }
    }

    fn start_mode(&self) -> &'static str {
        match (self.start, self.delayed) {
            (2, true) => "delayed-auto",
            (2, false) => "auto",
            (4, _) => "disabled",
            _ => "demand",
        }
    }
}

/// Elle secilmis, kapatilmasi bilinen bir soruna yol acmayan Windows servisleri.
/// (ad, grup, Turkce aciklama, Ingilizce aciklama)
const KNOWN: &[(&str, Group, &str, &str)] = &[
    ("diagtrack", Group::Safe, "Microsoft'a kullanım verisi (telemetri) gönderir", "Sends usage data (telemetry) to Microsoft"),
    ("dmwappushservice", Group::Safe, "Telemetri mesaj yönlendirmesi", "Telemetry message routing"),
    ("diagnosticshub.standardcollector.service", Group::Safe, "Geliştirici tanılama toplayıcısı", "Developer diagnostics collector"),
    ("wersvc", Group::Safe, "Hata raporlarını Microsoft'a gönderir", "Sends error reports to Microsoft"),
    ("pcasvc", Group::Safe, "Eski programlar için uyumluluk önerileri gösterir", "Shows compatibility suggestions for old programs"),
    ("xblauthmanager", Group::Safe, "Xbox Live oturumu", "Xbox Live sign-in"),
    ("xblgamesave", Group::Safe, "Xbox oyun kayıtları eşitlemesi", "Xbox game save sync"),
    ("xboxnetapisvc", Group::Safe, "Xbox Live ağ servisi", "Xbox Live networking service"),
    ("xboxgipsvc", Group::Safe, "Xbox oyun kumandası aksesuarları", "Xbox controller accessories"),
    ("mapsbroker", Group::Safe, "Çevrimdışı harita indirme", "Offline map downloads"),
    ("retaildemo", Group::Safe, "Mağaza tanıtım modu", "Retail demo mode"),
    ("wisvc", Group::Safe, "Windows Insider deneme sürümleri", "Windows Insider preview builds"),
    ("fax", Group::Safe, "Faks gönderme / alma", "Sending / receiving faxes"),
    ("remoteregistry", Group::Safe, "Kayıt defterine ağdan erişim (güvenlik riski)", "Network access to the registry (security risk)"),
    ("wmpnetworksvc", Group::Safe, "Media Player ağ paylaşımı", "Media Player network sharing"),
    ("phonesvc", Group::Safe, "Telefon durumu yönetimi", "Phone state management"),
    ("walletservice", Group::Safe, "Cüzdan", "Wallet"),
    ("ajrouter", Group::Safe, "AllJoyn akıllı cihaz yönlendiricisi", "AllJoyn smart device router"),
    ("trkwks", Group::Safe, "Ağda taşınan dosya kısayollarını izler", "Tracks shortcuts to files moved across the network"),
    ("wpcmonsvc", Group::Safe, "Ebeveyn denetimleri", "Parental controls"),
    ("semgrsvc", Group::Safe, "NFC ile ödeme", "NFC payments"),
    ("sensorservice", Group::Safe, "Tablet sensörleri (döndürme, ışık)", "Tablet sensors (rotation, light)"),
    ("sensrsvc", Group::Safe, "Sensör izleme", "Sensor monitoring"),
    ("sensordataservice", Group::Safe, "Sensör verisi", "Sensor data"),
    ("sharedrealitysvc", Group::Safe, "Karma gerçeklik", "Mixed reality"),
    ("perceptionsimulation", Group::Safe, "Karma gerçeklik benzetimi", "Mixed reality simulation"),
    ("spooler", Group::Optional, "Yazıcı ve \"PDF olarak yazdır\" kullanmıyorsanız", "If you don't use a printer or \"Print to PDF\""),
    ("printnotify", Group::Optional, "Yazıcı bildirimleri", "Printer notifications"),
    ("stisvc", Group::Optional, "Tarayıcı / kamera görüntü alma kullanmıyorsanız", "If you don't use scanner / camera image acquisition"),
    ("sysmain", Group::Optional, "Sık kullanılan programları belleğe önyükler; RAM kullanır", "Preloads frequently used programs into memory; uses RAM"),
    ("wsearch", Group::Optional, "Arama dizini; kapatılırsa Başlat ve Gezgin araması yavaşlar", "Search index; if turned off, Start and Explorer search get slower"),
    ("bthserv", Group::Optional, "Bluetooth kullanmıyorsanız", "If you don't use Bluetooth"),
    ("btagservice", Group::Optional, "Bluetooth ses kullanmıyorsanız", "If you don't use Bluetooth audio"),
    ("termservice", Group::Optional, "Bu bilgisayara Uzak Masaüstü ile bağlanılmıyorsa", "If nobody connects to this PC with Remote Desktop"),
    ("sessionenv", Group::Optional, "Uzak Masaüstü yapılandırması", "Remote Desktop configuration"),
    ("umrdpservice", Group::Optional, "Uzak Masaüstü aygıt yönlendirmesi", "Remote Desktop device redirection"),
    ("lfsvc", Group::Optional, "Konum servisi (hava durumu, harita)", "Location service (weather, maps)"),
    ("wbiosrvc", Group::Optional, "Parmak izi / yüz tanıma kullanmıyorsanız", "If you don't use fingerprint / face recognition"),
    ("icssvc", Group::Optional, "Mobil etkin nokta kullanmıyorsanız", "If you don't use mobile hotspot"),
    ("ssdpsrv", Group::Optional, "Ağdaki cihazları (TV, yazıcı) keşfetme", "Discovering devices on the network (TV, printer)"),
    ("upnphost", Group::Optional, "UPnP cihaz barındırma", "UPnP device hosting"),
    ("dusmsvc", Group::Optional, "Veri kullanımı sayaçları", "Data usage counters"),
    ("iphlpsvc", Group::Optional, "IPv6 geçiş tünelleri", "IPv6 transition tunnels"),
    ("fhsvc", Group::Optional, "Dosya Geçmişi yedeklemesi kullanmıyorsanız", "If you don't use File History backup"),
    ("tapisrv", Group::Optional, "Telefon / modem API", "Telephony / modem API"),
    ("vmicguestinterface", Group::Optional, "Yalnızca Hyper-V sanal makinesi içinde gerekir", "Only needed inside a Hyper-V virtual machine"),
    ("vmicheartbeat", Group::Optional, "Yalnızca Hyper-V sanal makinesi içinde gerekir", "Only needed inside a Hyper-V virtual machine"),
    ("vmickvpexchange", Group::Optional, "Yalnızca Hyper-V sanal makinesi içinde gerekir", "Only needed inside a Hyper-V virtual machine"),
    ("vmicrdv", Group::Optional, "Yalnızca Hyper-V sanal makinesi içinde gerekir", "Only needed inside a Hyper-V virtual machine"),
    ("vmicshutdown", Group::Optional, "Yalnızca Hyper-V sanal makinesi içinde gerekir", "Only needed inside a Hyper-V virtual machine"),
    ("vmictimesync", Group::Optional, "Yalnızca Hyper-V sanal makinesi içinde gerekir", "Only needed inside a Hyper-V virtual machine"),
    ("vmicvmsession", Group::Optional, "Yalnızca Hyper-V sanal makinesi içinde gerekir", "Only needed inside a Hyper-V virtual machine"),
    ("vmicvss", Group::Optional, "Yalnızca Hyper-V sanal makinesi içinde gerekir", "Only needed inside a Hyper-V virtual machine"),
];

const REMOTE: &[&str] = &[
    "anydesk", "teamviewer", "rustdesk", "uzakekran", "splashtop", "ammyy", "vnc", "logmein",
    "supremo", "parsec", "screenconnect", "remoteutilities", "radmin", "dwservice", "alpemix",
    "chrome remote", "chromoting",
];
const UPDATER: &[&str] = &[
    "update", "updater", "elevation", "maintenance", "telemetry", "crashhandler", "helper",
];

#[repr(C)]
struct Ess {
    name: *const u16,
    display: *const u16,
    st: [u32; 9],
}

#[link(name = "advapi32")]
extern "system" {
    fn OpenSCManagerW(machine: *const u16, db: *const u16, access: u32) -> isize;
    fn EnumServicesStatusExW(
        h: isize,
        level: u32,
        ty: u32,
        state: u32,
        buf: *mut u8,
        size: u32,
        needed: *mut u32,
        returned: *mut u32,
        resume: *mut u32,
        group: *const u16,
    ) -> i32;
    fn CloseServiceHandle(h: isize) -> i32;
}

unsafe fn pw(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut n = 0;
    while *p.add(n) != 0 {
        n += 1;
    }
    String::from_utf16_lossy(std::slice::from_raw_parts(p, n))
}

/// (ad, gorunen ad, tur, durum, pid)
fn enumerate() -> Vec<(String, String, u32, u32, u32)> {
    let mut out = Vec::new();
    unsafe {
        let h = OpenSCManagerW(std::ptr::null(), std::ptr::null(), 0x1 | 0x4);
        if h == 0 {
            return out;
        }
        let mut buf = vec![0u64; 32 * 1024];
        let mut resume = 0u32;
        for _ in 0..64 {
            let (mut needed, mut count) = (0u32, 0u32);
            let ok = EnumServicesStatusExW(
                h,
                0,
                0x30,
                3,
                buf.as_mut_ptr() as *mut u8,
                (buf.len() * 8) as u32,
                &mut needed,
                &mut count,
                &mut resume,
                std::ptr::null(),
            );
            let more = ok == 0 && std::io::Error::last_os_error().raw_os_error() == Some(234);
            if ok == 0 && !more {
                break;
            }
            let arr = buf.as_ptr() as *const Ess;
            for i in 0..count as usize {
                let e = &*arr.add(i);
                out.push((pw(e.name), pw(e.display), e.st[0], e.st[1], e.st[7]));
            }
            if !more {
                break;
            }
        }
        CloseServiceHandle(h);
    }
    out
}

fn windir() -> String {
    std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into())
}

pub fn scan() -> Vec<Service> {
    let win = windir().to_lowercase();
    let mut out = Vec::new();
    for (name, display, ty, state, pid) in enumerate() {
        // Kullanici basina uretilen servis kopyalari (Ad_1a2b3c) yonetilmez.
        if ty & 0xC0 != 0 {
            continue;
        }
        let Some(k) = Key::open(HKLM, &format!("{SERVICES}\\{name}"), false) else {
            continue;
        };
        let image = k.get_str("ImagePath").unwrap_or_default();
        let exe = parse_exe(&image);
        let dll = Key::open(HKLM, &format!("{SERVICES}\\{name}\\Parameters"), false)
            .and_then(|p| p.get_str("ServiceDll"))
            .or_else(|| k.get_str("ServiceDll"))
            .map(|d| crate::reg::expand_env(&d));
        let hosted = exe.to_lowercase().ends_with("svchost.exe");
        let file = match (&dll, hosted) {
            (Some(d), true) => d.clone(),
            _ => exe.clone(),
        };
        let low = name.to_lowercase();
        let hay = format!("{low} {} {}", display.to_lowercase(), file.to_lowercase());
        let known = KNOWN.iter().find(|x| x.0 == low);
        let in_win = file.to_lowercase().starts_with(&win);

        let (mut risk, mut publisher) = (Risk::Unknown, String::new());
        if !file.is_empty() && Path::new(&file).is_file() {
            let s = sig::verify_cached(&file);
            let stem = Path::new(&file)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let r = sig::assess(&stem, &file, Some(&s));
            risk = r.0;
            publisher = r.1;
        } else if file.contains('\\')
            && matches!(std::fs::metadata(&file), Err(e) if e.kind() == std::io::ErrorKind::NotFound)
        {
            publisher = t("Dosya bulunamadı (eski kayıt)", "File not found (stale entry)").into();
        }

        let group = if let Some(x) = known {
            x.1
        } else if hay.contains("vmware") || hay.contains("vmnat") || hay.contains("vmauthd") {
            Group::Vm
        } else if in_win && risk <= Risk::Ok {
            Group::Core
        } else if REMOTE.iter().any(|r| hay.contains(r)) {
            Group::Remote
        } else if UPDATER.iter().any(|r| hay.contains(r)) {
            Group::Updater
        } else {
            Group::Third
        };
        out.push(Service {
            display: if display.is_empty() { name.clone() } else { display },
            desc: k.get_mui("Description").unwrap_or_default(),
            running: state != 1,
            pid,
            start: k.get_u32("Start").unwrap_or(3),
            delayed: k.get_u32("DelayedAutostart") == Some(1),
            file,
            risk,
            publisher,
            group,
            note: known.map_or("", |x| if crate::lang::en() { x.3 } else { x.2 }),
            name,
        });
    }
    out.sort_by(|a, b| {
        a.group
            .cmp(&b.group)
            .then_with(|| b.running.cmp(&a.running))
            .then_with(|| a.display.to_lowercase().cmp(&b.display.to_lowercase()))
    });
    out
}

// ------------------------------------------------------------- degisiklikler

fn backup_path() -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.join("servis-yedek.txt"))
}

/// Bu programin degistirdigi servislerin ilk ayarlari: (ad, sc baslangic modu).
pub fn backup() -> Vec<(String, String)> {
    backup_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|t| {
            t.lines()
                .filter_map(|l| l.split_once('='))
                .map(|(a, b)| (a.trim().to_string(), b.trim().to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn remember(s: &Service) {
    let mut b = backup();
    if b.iter().any(|x| x.0.eq_ignore_ascii_case(&s.name)) {
        return;
    }
    b.push((s.name.clone(), s.start_mode().to_string()));
    save_backup(&b);
}

fn save_backup(b: &[(String, String)]) {
    let Some(p) = backup_path() else { return };
    if b.is_empty() {
        let _ = std::fs::remove_file(p);
    } else {
        let t: String = b.iter().map(|x| format!("{}={}\n", x.0, x.1)).collect();
        let _ = std::fs::write(p, t);
    }
}

fn config(name: &str, mode: &str) -> bool {
    win::run_status("sc", &["config", name, "start=", mode])
}

/// Servisi durdurur ve baslangic turunu degistirir. `disable`: devre disi, degilse elle.
pub fn turn_off(s: &Service, disable: bool) -> bool {
    if s.group.locked() {
        return false;
    }
    remember(s);
    let ok = config(&s.name, if disable { "disabled" } else { "demand" });
    if s.running {
        win::run_status("net", &["stop", &s.name, "/y"]);
    }
    ok
}

/// Kayitli ilk ayarlari geri yukler; otomatik olanlari yeniden baslatir.
pub fn restore_all() -> (usize, usize) {
    let b = backup();
    let mut left = Vec::new();
    let mut ok = 0;
    for (name, mode) in &b {
        if config(name, mode) {
            ok += 1;
            if mode.contains("auto") {
                win::run_status("sc", &["start", name]);
            }
        } else {
            left.push((name.clone(), mode.clone()));
        }
    }
    save_backup(&left);
    (ok, b.len())
}
