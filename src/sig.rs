//! Dijital imza (Authenticode) denetimi: gomulu imza + Windows katalog imzasi.
#![allow(non_snake_case)]

use crate::lang::t;
use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::os::windows::io::AsRawHandle;
use std::ptr::{null, null_mut};
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Status {
    /// Gecerli gomulu imza.
    Signed,
    /// Windows katalogunda kayitli (isletim sistemi dosyasi / surucu).
    Catalog,
    /// Hic imza yok.
    Unsigned,
    /// Imza var ama gecersiz / guvenilmeyen kok / dosya degistirilmis.
    Invalid,
    /// Dosya okunamadi.
    Unknown,
}

#[derive(Clone, Debug)]
pub struct Sig {
    pub status: Status,
    pub signer: String,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Risk {
    Unknown,
    Ok,
    Unsigned,
    Suspicious,
}

const SYSTEM_NAMES: &[&str] = &[
    "svchost", "lsass", "csrss", "winlogon", "services", "smss", "explorer", "dwm", "taskhostw",
    "conhost", "rundll32", "spoolsv", "wininit", "sihost", "ctfmon", "dllhost", "runtimebroker",
];

/// Surecin guven duzeyini ve kisa aciklamasini doner.
pub fn assess(name: &str, exe: &str, sig: Option<&Sig>) -> (Risk, String) {
    if exe.is_empty() {
        return (Risk::Unknown, String::new());
    }
    let Some(sig) = sig else {
        return (Risk::Unknown, t("denetleniyor…", "checking…").into());
    };
    let p = exe.to_lowercase();
    let windir = std::env::var("WINDIR")
        .unwrap_or_else(|_| "C:\\Windows".into())
        .to_lowercase();
    if SYSTEM_NAMES.contains(&name.to_lowercase().as_str()) && !p.starts_with(&windir) {
        return (
            Risk::Suspicious,
            t(
                "Sistem süreci adını taklit ediyor (Windows klasörü dışında)",
                "Imitates a system process name (outside the Windows folder)",
            )
            .into(),
        );
    }
    match sig.status {
        Status::Signed => (Risk::Ok, sig.signer.clone()),
        Status::Catalog => (Risk::Ok, t("Microsoft Windows (katalog)", "Microsoft Windows (catalog)").into()),
        Status::Invalid => (
            Risk::Suspicious,
            t("Geçersiz / güvenilmeyen imza", "Invalid / untrusted signature").into(),
        ),
        Status::Unknown => (Risk::Unknown, String::new()),
        Status::Unsigned => {
            let hot = [
                "\\appdata\\local\\temp\\",
                "\\windows\\temp\\",
                "\\downloads\\",
                "\\users\\public\\",
                "\\appdata\\roaming\\",
                "\\$recycle.bin\\",
            ];
            if hot.iter().any(|h| p.contains(h)) {
                (
                    Risk::Suspicious,
                    t(
                        "İmzasız · riskli konumdan çalışıyor",
                        "Unsigned · running from a risky location",
                    )
                    .into(),
                )
            } else {
                (Risk::Unsigned, t("İmzasız", "Unsigned").into())
            }
        }
    }
}

// ------------------------------------------------------------------ FFI

#[repr(C)]
struct Guid {
    d1: u32,
    d2: u16,
    d3: u16,
    d4: [u8; 8],
}
#[repr(C)]
struct FileInfo {
    cb: u32,
    path: *const u16,
    hfile: *mut c_void,
    subject: *const Guid,
}
#[repr(C)]
struct TrustData {
    cb: u32,
    policy: *mut c_void,
    sip: *mut c_void,
    ui: u32,
    revocation: u32,
    union_choice: u32,
    info: *mut c_void,
    state_action: u32,
    state: *mut c_void,
    url: *mut u16,
    prov_flags: u32,
    ui_context: u32,
    sig_settings: *mut c_void,
}

#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryW(name: *const u16) -> *mut c_void;
    fn GetProcAddress(h: *mut c_void, name: *const u8) -> *mut c_void;
}

type FnVerify = unsafe extern "system" fn(isize, *const Guid, *mut TrustData) -> i32;
type FnProvData = unsafe extern "system" fn(*mut c_void) -> *mut c_void;
type FnSigner = unsafe extern "system" fn(*mut c_void, u32, i32, u32) -> *mut c_void;
type FnCert = unsafe extern "system" fn(*mut c_void, u32) -> *mut c_void;
type FnName =
    unsafe extern "system" fn(*const c_void, u32, u32, *const c_void, *mut u16, u32) -> u32;
type FnAcquire =
    unsafe extern "system" fn(*mut isize, *const Guid, *const u16, *const c_void, u32) -> i32;
type FnHash = unsafe extern "system" fn(isize, *mut c_void, *mut u32, *mut u8, u32) -> i32;
type FnEnum = unsafe extern "system" fn(isize, *const u8, u32, u32, *mut isize) -> isize;
type FnRelCat = unsafe extern "system" fn(isize, isize, u32) -> i32;
type FnRel = unsafe extern "system" fn(isize, u32) -> i32;

struct Api {
    verify: FnVerify,
    prov_data: FnProvData,
    signer: FnSigner,
    cert: FnCert,
    name: FnName,
    acquire: FnAcquire,
    hash: FnHash,
    enum_cat: FnEnum,
    rel_cat: FnRelCat,
    rel: FnRel,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn api() -> Option<&'static Api> {
    static API: OnceLock<Option<Api>> = OnceLock::new();
    API.get_or_init(|| unsafe {
        let wt = LoadLibraryW(wide("wintrust.dll").as_ptr());
        let c32 = LoadLibraryW(wide("crypt32.dll").as_ptr());
        if wt.is_null() || c32.is_null() {
            return None;
        }
        macro_rules! sym {
            ($lib:expr, $n:literal) => {{
                let p = GetProcAddress($lib, concat!($n, "\0").as_ptr());
                if p.is_null() {
                    return None;
                }
                std::mem::transmute(p)
            }};
        }
        Some(Api {
            verify: sym!(wt, "WinVerifyTrust"),
            prov_data: sym!(wt, "WTHelperProvDataFromStateData"),
            signer: sym!(wt, "WTHelperGetProvSignerFromChain"),
            cert: sym!(wt, "WTHelperGetProvCertFromChain"),
            name: sym!(c32, "CertGetNameStringW"),
            acquire: sym!(wt, "CryptCATAdminAcquireContext2"),
            hash: sym!(wt, "CryptCATAdminCalcHashFromFileHandle2"),
            enum_cat: sym!(wt, "CryptCATAdminEnumCatalogFromHash"),
            rel_cat: sym!(wt, "CryptCATAdminReleaseCatalogContext"),
            rel: sym!(wt, "CryptCATAdminReleaseContext"),
        })
    })
    .as_ref()
}

const ACTION_V2: Guid = Guid {
    d1: 0x00AA_C56B,
    d2: 0xCD44,
    d3: 0x11D0,
    d4: [0x8C, 0xC2, 0x00, 0xC0, 0x4F, 0xC2, 0x95, 0xEE],
};

unsafe fn in_catalog(a: &Api, path: &str) -> bool {
    let Ok(f) = std::fs::File::open(path) else {
        return false;
    };
    for alg in ["SHA256", "SHA1"] {
        let mut admin = 0isize;
        let w = wide(alg);
        if (a.acquire)(&mut admin, null(), w.as_ptr(), null(), 0) == 0 {
            continue;
        }
        let mut hash = [0u8; 64];
        let mut len = hash.len() as u32;
        let mut found = false;
        if (a.hash)(admin, f.as_raw_handle() as *mut c_void, &mut len, hash.as_mut_ptr(), 0) != 0 {
            let cat = (a.enum_cat)(admin, hash.as_ptr(), len, 0, null_mut());
            if cat != 0 {
                found = true;
                (a.rel_cat)(admin, cat, 0);
            }
        }
        (a.rel)(admin, 0);
        if found {
            return true;
        }
    }
    false
}

/// Dosyanin imzasini denetler (ag erisimi yapmaz).
pub fn verify(path: &str) -> Sig {
    let unknown = Sig {
        status: Status::Unknown,
        signer: String::new(),
    };
    let Some(a) = api() else {
        return unknown;
    };
    if !std::path::Path::new(path).is_file() {
        return unknown;
    }
    // Okunamayan dosya (erisim reddi, kilitli) "gecersiz imza" sayilmaz: bilinmiyor.
    if std::fs::File::open(path).is_err() {
        return unknown;
    }
    let wpath = wide(path);
    unsafe {
        let mut fi = FileInfo {
            cb: std::mem::size_of::<FileInfo>() as u32,
            path: wpath.as_ptr(),
            hfile: null_mut(),
            subject: null(),
        };
        let mut td = TrustData {
            cb: std::mem::size_of::<TrustData>() as u32,
            policy: null_mut(),
            sip: null_mut(),
            ui: 2,         // WTD_UI_NONE
            revocation: 0, // WTD_REVOKE_NONE
            union_choice: 1,
            info: &mut fi as *mut _ as *mut c_void,
            state_action: 1,                // VERIFY
            state: null_mut(),
            url: null_mut(),
            prov_flags: 0x1000 | 0x10, // yalniz onbellek, iptal denetimi yok
            ui_context: 0,
            sig_settings: null_mut(),
        };
        let rc = (a.verify)(-1, &ACTION_V2, &mut td) as u32;
        let mut signer = String::new();
        if rc == 0 && !td.state.is_null() {
            let prov = (a.prov_data)(td.state);
            if !prov.is_null() {
                let sg = (a.signer)(prov, 0, 0, 0);
                if !sg.is_null() {
                    let pc = (a.cert)(sg, 0);
                    if !pc.is_null() {
                        let cert = *(pc as *const *const c_void).add(1);
                        if !cert.is_null() {
                            let mut buf = [0u16; 256];
                            let n = (a.name)(cert, 4, 0, null(), buf.as_mut_ptr(), 256);
                            if n > 1 {
                                signer = String::from_utf16_lossy(&buf[..n as usize - 1]);
                            }
                        }
                    }
                }
            }
        }
        td.state_action = 2; // CLOSE
        (a.verify)(-1, &ACTION_V2, &mut td);

        let status = if rc == 0 {
            Status::Signed
        } else if in_catalog(a, path) {
            Status::Catalog
        } else if rc == 0x800B_0100 || rc == 0x800B_0003 || rc == 0x800B_0001 {
            // imza yok / bilinmeyen bicim
            Status::Unsigned
        } else {
            Status::Invalid
        };
        Sig { status, signer }
    }
}

// ------------------------------------------------------- onbellek + kuyruk

struct Store {
    cache: HashMap<String, Sig>,
    queue: Vec<String>,
    queued: HashSet<String>,
    started: bool,
}

fn store() -> &'static Mutex<Store> {
    static S: OnceLock<Mutex<Store>> = OnceLock::new();
    S.get_or_init(|| {
        Mutex::new(Store {
            cache: HashMap::new(),
            queue: Vec::new(),
            queued: HashSet::new(),
            started: false,
        })
    })
}

/// Onbellekteki sonucu doner; yoksa arka planda denetlenmek uzere siraya koyar.
pub fn lookup(path: &str) -> Option<Sig> {
    if path.is_empty() {
        return None;
    }
    let mut s = store().lock().ok()?;
    if let Some(v) = s.cache.get(path) {
        return Some(v.clone());
    }
    if s.queued.insert(path.to_string()) {
        s.queue.push(path.to_string());
    }
    if !s.started {
        s.started = true;
        std::thread::spawn(worker);
    }
    None
}

/// Hemen denetler (onbellegi kullanir). Yavas olabilir; arayuz is parcaciginda cagirmayin.
pub fn verify_cached(path: &str) -> Sig {
    if let Ok(s) = store().lock() {
        if let Some(v) = s.cache.get(path) {
            return v.clone();
        }
    }
    let v = verify(path);
    if let Ok(mut s) = store().lock() {
        s.cache.insert(path.to_string(), v.clone());
    }
    v
}

fn worker() {
    loop {
        let next = store().lock().ok().and_then(|mut s| s.queue.pop());
        match next {
            Some(p) => {
                let v = verify(&p);
                if let Ok(mut s) = store().lock() {
                    s.cache.insert(p, v);
                }
            }
            None => std::thread::sleep(std::time::Duration::from_millis(700)),
        }
    }
}
