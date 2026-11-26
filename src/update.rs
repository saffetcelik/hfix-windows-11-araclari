//! GitHub uzerinden otomatik guncelleme.
//!
//! Her surum iki dosya tasir: HFixWin11.exe ve HFixWin11.exe.sig. .sig dosyasi kucuk bir JSON
//! bildirimidir (surum, boyut, SHA-256) ve acik yarisi programa gomulu bir Ed25519 anahtariyla
//! imzalanmistir. Imza, surum ve SHA-256 tutmadikca hicbir sey kurulmaz: degistirilmis bir
//! indirme ya da sahte bir surum kullaniciya kod gonderemez. Surum imzali metnin parcasi oldugu
//! icin eski (gercekten imzali) bir surum "daha yeni" diye yeniden sunulamaz.
//!
//! Program acildiktan 10 saniye sonra ve sonra alti saatte bir GitHub'a sorar (baglanti yoksa 10
//! dakika sonra yeniden dener). Dogrulanan guncelleme <program klasoru>\guncelleme icine iner;
//! kullanici "Yeniden baslat" dediginde ya da program bir sonraki acilisinda kurulur.

use crate::win;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use sha2::{Digest, Sha256};
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Mutex, OnceLock};
use std::time::Duration;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const REPO: &str = "saffetcelik/hfix-windows-11-araclari";
pub const ASSET_EXE: &str = "HFixWin11.exe";
pub const ASSET_SIG: &str = "HFixWin11.exe.sig";

/// Surum imzalama anahtarinin acik yarisi (onaltilik, 32 bayt). Gizli yarisi depoda durmaz.
const PUBLIC_KEY: &str = "addbd6d4e347512ef9025f08cd19aec2ac73e3b00455e85a57a17bc7886d512a";

const CHECK_EVERY: Duration = Duration::from_secs(6 * 3600);
const RETRY_AFTER: Duration = Duration::from_secs(600);
const FIRST_CHECK: Duration = Duration::from_secs(10);
const MAX_SIZE: u64 = 200 << 20;

pub fn project_page() -> String {
    format!("https://github.com/{REPO}")
}
fn releases_page() -> String {
    format!("https://github.com/{REPO}/releases/")
}
fn latest_url() -> String {
    format!("https://api.github.com/repos/{REPO}/releases/latest")
}

// ------------------------------------------------- surum karsilastirma

/// "v1.2.0" -> "1.2.0"
pub fn clean(v: &str) -> &str {
    let v = v.trim();
    v.strip_prefix('v').unwrap_or(v)
}

fn split(v: &str) -> (Vec<u64>, &str) {
    let (num, suffix) = match v.find(['-', '+']) {
        Some(i) => (&v[..i], &v[i + 1..]),
        None => (v, ""),
    };
    (num.split('.').map(|p| p.parse().unwrap_or(0)).collect(), suffix)
}

/// Noktali surumleri sayisal karsilastirir ("1.10.0" > "1.9.3"); "-beta" gibi bir ek, eksiz
/// surumden once gelir.
pub fn compare(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering::*;
    let (pa, sa) = split(clean(a));
    let (pb, sb) = split(clean(b));
    for i in 0..pa.len().max(pb.len()) {
        let x = pa.get(i).copied().unwrap_or(0);
        let y = pb.get(i).copied().unwrap_or(0);
        if x != y {
            return x.cmp(&y);
        }
    }
    match (sa.is_empty(), sb.is_empty()) {
        (true, true) => Equal,
        (true, false) => Greater,
        (false, true) => Less,
        _ => sa.cmp(sb),
    }
}

// ------------------------------------------------- imzali bildirim

#[derive(Clone, Default)]
pub struct Manifest {
    pub version: String,
    pub size: u64,
    pub sha256: String,
    /// Onaltilik Ed25519 imzasi (64 bayt).
    pub signature: String,
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if s.len() % 2 != 0 || !s.is_ascii() {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

fn sha256_hex(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

impl Manifest {
    /// Imzalanan metnin tam hali.
    fn message(&self) -> Vec<u8> {
        format!(
            "HFixWin11 update v1\nversion={}\nsize={}\nsha256={}\n",
            self.version, self.size, self.sha256
        )
        .into_bytes()
    }

    fn sign(seed: &[u8; 32], version: &str, exe: &[u8]) -> Manifest {
        let mut m = Manifest {
            version: clean(version).to_string(),
            size: exe.len() as u64,
            sha256: sha256_hex(exe),
            signature: String::new(),
        };
        m.signature = hex(&SigningKey::from_bytes(seed).sign(&m.message()).to_bytes());
        m
    }

    fn verify(&self, public_hex: &str) -> bool {
        let (Some(key), Some(sig)) = (unhex(public_hex), unhex(&self.signature)) else {
            return false;
        };
        let (Ok(key), Ok(sig)) = (<[u8; 32]>::try_from(key), <[u8; 64]>::try_from(sig)) else {
            return false;
        };
        let Ok(key) = VerifyingKey::from_bytes(&key) else {
            return false;
        };
        !self.version.is_empty()
            && self.size > 0
            && self.sha256.len() == 64
            && key
                .verify_strict(&self.message(), &Signature::from_bytes(&sig))
                .is_ok()
    }

    /// Bir surum icin kararin tamami: bizim anahtarla imzali, surum etiketiyle ayni ve calisan
    /// programdan yeni olmali.
    fn accept(&self, tag: &str, current: &str) -> bool {
        self.verify(PUBLIC_KEY)
            && clean(&self.version) == clean(tag)
            && compare(&self.version, current) == std::cmp::Ordering::Greater
    }

    fn matches(&self, size: u64, sha: &str) -> bool {
        size == self.size && sha.eq_ignore_ascii_case(&self.sha256)
    }

    fn to_json(&self, notes: &str) -> String {
        serde_json::json!({
            "version": self.version,
            "size": self.size,
            "sha256": self.sha256,
            "signature": self.signature,
            "notes": notes,
        })
        .to_string()
    }

    /// (bildirim, surum notlari adresi)
    fn from_json(text: &str) -> Option<(Manifest, String)> {
        let v: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
        Some((
            Manifest {
                version: v["version"].as_str()?.to_string(),
                size: v["size"].as_u64()?,
                sha256: v["sha256"].as_str()?.to_string(),
                signature: v["signature"].as_str()?.to_string(),
            },
            v["notes"].as_str().unwrap_or("").to_string(),
        ))
    }
}

// ------------------------------------------------- HTTPS (WinHTTP)

type H = *mut c_void;

struct Http {
    open: unsafe extern "system" fn(*const u16, u32, *const u16, *const u16, u32) -> H,
    connect: unsafe extern "system" fn(H, *const u16, u16, u32) -> H,
    request: unsafe extern "system" fn(H, *const u16, *const u16, *const u16, *const u16, *const *const u16, u32) -> H,
    send: unsafe extern "system" fn(H, *const u16, u32, *const c_void, u32, u32, usize) -> i32,
    receive: unsafe extern "system" fn(H, *mut c_void) -> i32,
    query: unsafe extern "system" fn(H, u32, *const u16, *mut c_void, *mut u32, *mut u32) -> i32,
    read: unsafe extern "system" fn(H, *mut u8, u32, *mut u32) -> i32,
    close: unsafe extern "system" fn(H) -> i32,
    timeouts: unsafe extern "system" fn(H, i32, i32, i32, i32) -> i32,
}

fn http() -> Option<&'static Http> {
    static API: OnceLock<Option<Http>> = OnceLock::new();
    API.get_or_init(|| unsafe {
        macro_rules! f {
            ($n:literal) => {
                std::mem::transmute(win::dll_fn("winhttp.dll", $n)?)
            };
        }
        Some(Http {
            open: f!(b"WinHttpOpen\0"),
            connect: f!(b"WinHttpConnect\0"),
            request: f!(b"WinHttpOpenRequest\0"),
            send: f!(b"WinHttpSendRequest\0"),
            receive: f!(b"WinHttpReceiveResponse\0"),
            query: f!(b"WinHttpQueryHeaders\0"),
            read: f!(b"WinHttpReadData\0"),
            close: f!(b"WinHttpCloseHandle\0"),
            timeouts: f!(b"WinHttpSetTimeouts\0"),
        })
    })
    .as_ref()
}

/// Son `fetch` 404 ile mi bitti? (Denetimler tek is parcaciginda sirayla yapilir.)
static NOT_FOUND: AtomicBool = AtomicBool::new(false);

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// `url` adresini (yalnizca https) indirir; her parca `sink`e verilir. `limit` asilirsa ya da
/// `sink` false donerse indirme kesilir. Yonlendirmeler izlenir; WinHTTP https'ten http'ye
/// yonlendirmeyi kendiliginden reddeder.
fn fetch(url: &str, limit: u64, mut sink: impl FnMut(&[u8]) -> bool) -> Result<u64, ()> {
    NOT_FOUND.store(false, Ordering::Relaxed);
    let rest = url.strip_prefix("https://").ok_or(())?;
    let (host, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    if host.is_empty() || host.contains(['@', ':']) {
        return Err(());
    }
    let api = http().ok_or(())?;
    struct Guard(&'static Http, H);
    impl Drop for Guard {
        fn drop(&mut self) {
            if !self.1.is_null() {
                unsafe { (self.0.close)(self.1) };
            }
        }
    }
    unsafe {
        let agent = wide(&format!("HFixWin11/{VERSION}"));
        // 4: sistemin vekil sunucu ayarini kullan.
        let session = Guard(api, (api.open)(agent.as_ptr(), 4, null(), null(), 0));
        if session.1.is_null() {
            return Err(());
        }
        (api.timeouts)(session.1, 15_000, 15_000, 30_000, 30_000);
        let conn = Guard(api, (api.connect)(session.1, wide(host).as_ptr(), 443, 0));
        if conn.1.is_null() {
            return Err(());
        }
        // 0x00800000: WINHTTP_FLAG_SECURE
        let req = Guard(
            api,
            (api.request)(conn.1, wide("GET").as_ptr(), wide(path).as_ptr(), null(), null(), null(), 0x0080_0000),
        );
        if req.1.is_null() {
            return Err(());
        }
        let headers = wide("Accept: application/vnd.github+json, application/octet-stream;q=0.9, */*;q=0.8\r\n");
        if (api.send)(req.1, headers.as_ptr(), u32::MAX, null(), 0, 0, 0) == 0
            || (api.receive)(req.1, null_mut()) == 0
        {
            return Err(());
        }
        // 19 | 0x20000000: durum kodu, sayi olarak
        let (mut status, mut len) = (0u32, 4u32);
        if (api.query)(req.1, 19 | 0x2000_0000, null(), &mut status as *mut u32 as *mut c_void, &mut len, null_mut()) == 0
            || status != 200
        {
            NOT_FOUND.store(status == 404, Ordering::Relaxed);
            return Err(());
        }
        let mut buf = vec![0u8; 64 * 1024];
        let mut total = 0u64;
        loop {
            let mut n = 0u32;
            if (api.read)(req.1, buf.as_mut_ptr(), buf.len() as u32, &mut n) == 0 {
                return Err(());
            }
            if n == 0 {
                return Ok(total);
            }
            total += n as u64;
            if total > limit || !sink(&buf[..n as usize]) {
                return Err(());
            }
        }
    }
}

fn fetch_text(url: &str, limit: u64) -> Result<String, ()> {
    let mut out = Vec::new();
    fetch(url, limit, |b| {
        out.extend_from_slice(b);
        true
    })?;
    String::from_utf8(out).map_err(|_| ())
}

// ------------------------------------------------- durum

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Phase {
    Idle,
    Checking,
    Downloading,
    Latest,
    /// Indirildi ve dogrulandi; yeniden baslatinca kurulur.
    Ready,
    Error,
}

#[derive(Clone, Debug)]
pub struct Info {
    pub phase: Phase,
    pub version: String,
    /// Surum notlari sayfasi (varsa).
    pub notes: String,
    /// network, signature, mismatch, no_asset, disk
    pub error: &'static str,
    /// Son denetimin saati.
    pub checked: String,
}

static INFO: Mutex<Info> = Mutex::new(Info {
    phase: Phase::Idle,
    version: String::new(),
    notes: String::new(),
    error: "",
    checked: String::new(),
});
static AUTO: AtomicBool = AtomicBool::new(true);
static BUSY: AtomicBool = AtomicBool::new(false);
static WAKE: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();
static KICK: OnceLock<Mutex<mpsc::Sender<()>>> = OnceLock::new();

pub fn info() -> Info {
    INFO.lock().map(|i| i.clone()).unwrap_or_else(|e| e.into_inner().clone())
}

fn set(phase: Phase, version: &str, notes: &str, error: &'static str) {
    if let Ok(mut i) = INFO.lock() {
        i.phase = phase;
        i.version = version.to_string();
        i.notes = notes.to_string();
        i.error = error;
        if !matches!(phase, Phase::Checking | Phase::Downloading) {
            i.checked = win::clock();
        }
    }
    if let Some(w) = WAKE.get() {
        w();
    }
}

pub fn dir() -> Option<PathBuf> {
    Some(crate::cfg::dir()?.join("guncelleme"))
}

/// Basarisiz denetimi bildirir; daha once indirilmis bir guncelleme bekliyorsa o gosterilmeye
/// devam eder.
fn fail(code: &'static str) {
    if let Some((m, notes)) = dir().and_then(|d| read_staged(&d)) {
        set(Phase::Ready, &m.version, &notes, "");
    } else {
        set(Phase::Error, "", "", code);
    }
}

/// Bu bilgisayarin baglantisi ya da diskiyle ilgili hatalar kisa surede yeniden denenir; imzasi
/// tutmayan bir surum bir sonraki olagan denetime kadar yeniden denenmez.
fn retryable(code: &str) -> bool {
    matches!(code, "network" | "mismatch" | "disk")
}

pub fn set_auto(on: bool) {
    let was = AUTO.swap(on, Ordering::Relaxed);
    if on && !was {
        kick();
    }
}

fn kick() {
    if let Some(k) = KICK.get() {
        if let Ok(k) = k.lock() {
            let _ = k.send(());
        }
    }
}

/// Arka plan denetimini baslatir. `wake` durum degistiginde pencereyi yeniden cizdirir.
pub fn start(auto: bool, wake: impl Fn() + Send + Sync + 'static) {
    AUTO.store(auto, Ordering::Relaxed);
    let _ = WAKE.set(Box::new(wake));
    let Some(d) = dir() else {
        return;
    };
    // Onceki oturumda indirilmis ama henuz kurulmamis guncelleme.
    if let Some((m, notes)) = read_staged(&d) {
        set(Phase::Ready, &m.version, &notes, "");
    }
    let (tx, rx) = mpsc::channel::<()>();
    let _ = KICK.set(Mutex::new(tx));
    std::thread::spawn(move || {
        let mut wait = FIRST_CHECK;
        loop {
            let _ = rx.recv_timeout(wait);
            wait = CHECK_EVERY;
            if AUTO.load(Ordering::Relaxed) {
                check();
                let i = info();
                if i.phase == Phase::Error && retryable(i.error) {
                    wait = RETRY_AFTER;
                }
            }
        }
    });
}

/// Hakkinda sayfasindaki "Guncellemeleri denetle" dugmesi: otomatik guncelleme kapali olsa da
/// simdi denetler.
pub fn check_now() {
    std::thread::spawn(check);
}

/// GitHub'a en son surumu sorar; daha yeni ve dogru imzaliysa indirip hazirlar.
fn check() {
    let Some(d) = dir() else {
        return;
    };
    if BUSY.swap(true, Ordering::SeqCst) {
        return;
    }
    check_inner(&d);
    BUSY.store(false, Ordering::SeqCst);
}

fn check_inner(d: &Path) {
    // "Yeniden baslat" bildirimi yeniden denetlenirken de gorunur kalsin.
    if info().phase != Phase::Ready {
        set(Phase::Checking, "", "", "");
    }
    let Some(rel) = fetch_text(&latest_url(), 1 << 20)
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
    else {
        // 404: depoda henuz yayimlanmis surum yok.
        if NOT_FOUND.load(Ordering::Relaxed) {
            return set(Phase::Latest, "", "", "");
        }
        return fail("network");
    };
    let tag = clean(rel["tag_name"].as_str().unwrap_or("")).to_string();
    let draft = rel["draft"].as_bool().unwrap_or(false) || rel["prerelease"].as_bool().unwrap_or(false);
    if tag.is_empty() || draft || compare(&tag, VERSION) != std::cmp::Ordering::Greater {
        // Calisandan eski bir hazir guncelleme ise yaramaz.
        let _ = std::fs::remove_dir_all(d);
        return set(Phase::Latest, "", "", "");
    }
    let page = rel["html_url"].as_str().unwrap_or("");
    let notes = if page.starts_with(&releases_page()) { page } else { "" }.to_string();
    if let Some((m, _)) = read_staged(d) {
        if compare(&m.version, &tag) != std::cmp::Ordering::Less {
            return set(Phase::Ready, &m.version, &notes, "");
        }
    }
    let asset = |name: &str| -> Option<(String, u64)> {
        rel["assets"].as_array()?.iter().find_map(|a| {
            (a["name"].as_str()? == name).then(|| {
                Some((
                    a["browser_download_url"].as_str()?.to_string(),
                    a["size"].as_u64().unwrap_or(0),
                ))
            })?
        })
    };
    let (Some((exe_url, exe_size)), Some((sig_url, _))) = (asset(ASSET_EXE), asset(ASSET_SIG)) else {
        return fail("no_asset");
    };
    set(Phase::Downloading, &tag, &notes, "");

    let Ok(raw) = fetch_text(&sig_url, 64 << 10) else {
        return fail("network");
    };
    let Some((m, _)) = Manifest::from_json(&raw) else {
        return fail("signature");
    };
    if !m.accept(&tag, VERSION) {
        return fail("signature");
    }
    if m.size > MAX_SIZE || (exe_size > 0 && exe_size != m.size) {
        return fail("mismatch");
    }
    match download(d, &exe_url, &m, &notes) {
        Ok(()) => set(Phase::Ready, &m.version, &notes, ""),
        Err(code) => fail(code),
    }
}

/// Yeni exe'yi bildirimiyle birlikte kaydeder. Dosya inerken ozeti hesaplanir ve yalnizca
/// imzalanan dosyanin tipatip aynisiysa saklanir.
fn download(d: &Path, url: &str, m: &Manifest, notes: &str) -> Result<(), &'static str> {
    use std::io::Write;
    let _ = std::fs::remove_dir_all(d);
    std::fs::create_dir_all(d).map_err(|_| "disk")?;
    let part = d.join(format!("{ASSET_EXE}.part"));
    let mut file = std::fs::File::create(&part).map_err(|_| "disk")?;
    let mut hash = Sha256::new();
    let mut disk_ok = true;
    let got = fetch(url, m.size, |b| {
        hash.update(b);
        disk_ok = file.write_all(b).is_ok();
        disk_ok
    });
    drop(file);
    let res = match got {
        _ if !disk_ok => Err("disk"),
        Err(()) => Err("network"),
        Ok(n) if !m.matches(n, &hex(&hash.finalize())) => Err("mismatch"),
        Ok(_) => std::fs::rename(&part, d.join(ASSET_EXE))
            .and_then(|_| std::fs::write(d.join("guncelleme.json"), m.to_json(notes)))
            .map_err(|_| "disk"),
    };
    if res.is_err() {
        let _ = std::fs::remove_dir_all(d);
    }
    res
}

/// Indirilmis guncellemeyi yalnizca hala gecerli, daha yeni ve bozulmamis bir surumse doner;
/// klasordeki baska her sey silinir.
fn read_staged(d: &Path) -> Option<(Manifest, String)> {
    if !d.exists() {
        return None;
    }
    let found = (|| {
        let text = std::fs::read_to_string(d.join("guncelleme.json")).ok()?;
        let (m, notes) = Manifest::from_json(&text)?;
        if !m.accept(&m.version, VERSION) {
            return None;
        }
        let data = std::fs::read(d.join(ASSET_EXE)).ok()?;
        m.matches(data.len() as u64, &sha256_hex(&data)).then_some((m, notes))
    })();
    if found.is_none() {
        let _ = std::fs::remove_dir_all(d);
    }
    found
}

/// `src` dosyasini `dst` uzerine tasir. Calisan program Windows'ta uzerine yazilamaz ama yeniden
/// adlandirilabilir; bu yuzden `dst` once kenara alinir. Geride kalan *.old dosyasi simdi ya da
/// hala kullaniliyorsa sonraki acilista silinir.
pub fn replace_file(src: &Path, dst: &Path) -> std::io::Result<()> {
    if !dst.exists() {
        return std::fs::rename(src, dst);
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let old = PathBuf::from(format!("{}.{stamp}.old", dst.display()));
    std::fs::rename(dst, &old)?;
    if let Err(e) = std::fs::rename(src, dst) {
        let _ = std::fs::rename(&old, dst);
        return Err(e);
    }
    let _ = std::fs::remove_file(&old);
    Ok(())
}

/// Onceki guncellemelerden kalan *.old dosyalarini siler.
pub fn cleanup_old() {
    let Some(d) = crate::cfg::dir() else {
        return;
    };
    for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
        if e.file_name().to_string_lossy().ends_with(".old") {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// Dogrulanmis yeni exe'yi calisanin yerine koyar.
fn install_staged() -> bool {
    let (Some(d), Ok(exe)) = (dir(), std::env::current_exe()) else {
        return false;
    };
    if read_staged(&d).is_none() || replace_file(&d.join(ASSET_EXE), &exe).is_err() {
        return false;
    }
    let _ = std::fs::remove_dir_all(d);
    true
}

/// Pencere acilmadan once: bekleyen guncelleme varsa kurar ve yeni surumu bu surecin yerine
/// baslatir. true donerse bu surec cikmalidir.
pub fn apply_at_start(args: &[String]) -> bool {
    if !dir().is_some_and(|d| d.exists()) || !install_staged() {
        return false;
    }
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    std::process::Command::new(exe).args(&args[1..]).spawn().is_ok()
}

/// "Yeniden baslat ve kur": guncellemeyi kurar, yeni surumu baslatir ve cikar.
pub fn apply_and_restart() {
    if !install_staged() {
        return fail("mismatch");
    }
    if let Ok(exe) = std::env::current_exe() {
        // Yeni surum, bu surec kapanana kadar bekler (tek kopya denetimi icin).
        let _ = std::process::Command::new(exe)
            .arg(format!("--after={}", win::own_pid()))
            .spawn();
    }
    crate::tray::quit();
}

// ------------------------------------------------- surum imzalama (yalnizca yayimlarken)

/// Yeni bir imza anahtari uretir: gizli yari `file` icine, acik yari `file`.pub icine yazilir.
pub fn cmd_keygen(file: &str) -> i32 {
    if Path::new(file).exists() {
        return 1; // var olan anahtarin uzerine yazilmaz
    }
    let mut seed = [0u8; 32];
    if !win::random(&mut seed) {
        return 1;
    }
    let public = SigningKey::from_bytes(&seed).verifying_key();
    let ok = std::fs::write(file, hex(&seed) + "\n").is_ok()
        && std::fs::write(format!("{file}.pub"), hex(public.as_bytes()) + "\n").is_ok();
    !ok as i32
}

/// `exe` icin imzali bildirimi `sig` dosyasina yazar. Anahtar programa gomulu olan degilse 2
/// doner: boyle bir surumu hicbir kullanici kabul etmez.
pub fn cmd_sign(key_file: &str, exe: &str, sig: &str) -> i32 {
    let seed = std::fs::read_to_string(key_file).ok().and_then(|t| unhex(&t));
    let Some(Ok(seed)) = seed.map(<[u8; 32]>::try_from) else {
        return 1;
    };
    let Ok(data) = std::fs::read(exe) else {
        return 1;
    };
    if hex(SigningKey::from_bytes(&seed).verifying_key().as_bytes()) != PUBLIC_KEY {
        return 2;
    }
    let m = Manifest::sign(&seed, VERSION, &data);
    let text = serde_json::json!({
        "version": m.version, "size": m.size, "sha256": m.sha256, "signature": m.signature,
    });
    let Ok(pretty) = serde_json::to_string_pretty(&text) else {
        return 1;
    };
    std::fs::write(sig, pretty + "\n").is_err() as i32
}

/// Bir surumu (exe + sig) programa gomulu anahtara ve bu surum numarasina gore dogrular.
pub fn cmd_verify(exe: &str, sig: &str) -> i32 {
    let ok = (|| {
        let (m, _) = Manifest::from_json(&std::fs::read_to_string(sig).ok()?)?;
        let data = std::fs::read(exe).ok()?;
        Some(
            m.verify(PUBLIC_KEY)
                && m.matches(data.len() as u64, &sha256_hex(&data))
                && clean(&m.version) == VERSION,
        )
    })();
    (ok != Some(true)) as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering::*;

    #[test]
    fn versions() {
        assert_eq!(compare("1.10.0", "1.9.3"), Greater);
        assert_eq!(compare("v1.2.0", "1.2"), Equal);
        assert_eq!(compare("1.2.0-beta", "1.2.0"), Less);
        assert_eq!(compare("1.0.0", "1.0.1"), Less);
    }

    #[test]
    fn manifest() {
        let seed = [7u8; 32];
        let public = hex(SigningKey::from_bytes(&seed).verifying_key().as_bytes());
        let m = Manifest::sign(&seed, "v9.9.9", b"abc");
        assert!(m.verify(&public));
        assert!(m.matches(3, &sha256_hex(b"abc")));
        assert!(!m.matches(3, &sha256_hex(b"abd")));
        // Surum imzali metnin parcasidir: degistirilirse imza tutmaz.
        let mut forged = m.clone();
        forged.version = "10.0.0".into();
        assert!(!forged.verify(&public));
        let (back, notes) = Manifest::from_json(&m.to_json("n")).unwrap();
        assert!(back.verify(&public) && notes == "n");
    }
}
