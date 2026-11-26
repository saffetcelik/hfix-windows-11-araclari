//! Disk temizligi: silinmesi zararsiz gecici dosya ve onbellek klasorleri.
#![allow(non_snake_case)]

use crate::lang;
use std::fs;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Clone)]
pub enum Target {
    /// Klasorun icindeki her sey (klasorun kendisi kalir).
    Dir(PathBuf),
    /// Klasordeki, adi verilen onek/sonekle eslesen dosyalar.
    Files(PathBuf, &'static str, &'static str),
    File(PathBuf),
    RecycleBin,
}

#[derive(Clone)]
pub struct Cat {
    pub id: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    pub on: bool,
    pub targets: Vec<Target>,
    /// Bu sureden yeni dosyalara dokunulmaz.
    pub age: Option<Duration>,
    pub bytes: u64,
    pub files: u64,
    pub scanned: bool,
    pub freed: u64,
    pub skipped: u64,
}

#[derive(Default)]
struct Stat {
    bytes: u64,
    files: u64,
    skipped: u64,
}

#[repr(C)]
struct RbInfo {
    cb: u32,
    size: i64,
    items: i64,
}

#[link(name = "shell32")]
extern "system" {
    fn SHQueryRecycleBinW(root: *const u16, info: *mut RbInfo) -> i32;
    fn SHEmptyRecycleBinW(hwnd: *mut u8, root: *const u16, flags: u32) -> i32;
}

fn env(name: &str) -> Option<PathBuf> {
    std::env::var(name)
        .ok()
        .filter(|v| v.len() > 3)
        .map(PathBuf::from)
}

/// Kok dizin ya da cok kisa yollar asla temizlenmez.
fn safe(p: &Path) -> bool {
    p.is_absolute() && p.components().count() >= 4
}

fn walk(dir: &Path, age: Option<Duration>, delete: bool, st: &mut Stat, depth: u32) {
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let Ok(md) = e.metadata() else {
            continue;
        };
        let attrs = md.file_attributes();
        if attrs & 0x400 != 0 {
            continue; // baglanti / kavsak noktasi: izleme, silme
        }
        let p = e.path();
        if md.is_dir() {
            if depth < 48 {
                walk(&p, age, delete, st, depth + 1);
            }
            if delete {
                let _ = fs::remove_dir(&p); // yalnizca bossa silinir
            }
        } else {
            one_file(&p, &md, age, delete, st);
        }
    }
}

fn one_file(p: &Path, md: &fs::Metadata, age: Option<Duration>, delete: bool, st: &mut Stat) {
    if let Some(age) = age {
        let old_enough = md
            .modified()
            .ok()
            .and_then(|m| m.elapsed().ok())
            .is_some_and(|el| el >= age);
        if !old_enough {
            return;
        }
    }
    let size = md.len();
    if !delete {
        st.bytes += size;
        st.files += 1;
        return;
    }
    if md.file_attributes() & 1 != 0 {
        let mut perm = md.permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perm.set_readonly(false);
        let _ = fs::set_permissions(p, perm);
    }
    match fs::remove_file(p) {
        Ok(()) => {
            st.bytes += size;
            st.files += 1;
        }
        Err(_) => st.skipped += 1,
    }
}

fn run(c: &Cat, delete: bool) -> Stat {
    let mut st = Stat::default();
    for t in &c.targets {
        match t {
            Target::Dir(d) => {
                if safe(d) {
                    walk(d, c.age, delete, &mut st, 0);
                }
            }
            Target::Files(d, pre, suf) => {
                if !safe(d) {
                    continue;
                }
                let Ok(rd) = fs::read_dir(d) else {
                    continue;
                };
                for e in rd.flatten() {
                    let n = e.file_name().to_string_lossy().to_lowercase();
                    if n.starts_with(pre) && n.ends_with(suf) {
                        if let Ok(md) = e.metadata() {
                            if md.is_file() {
                                one_file(&e.path(), &md, c.age, delete, &mut st);
                            }
                        }
                    }
                }
            }
            Target::File(f) => {
                if let Ok(md) = fs::symlink_metadata(f) {
                    if md.is_file() && safe(f) {
                        one_file(f, &md, c.age, delete, &mut st);
                    }
                }
            }
            Target::RecycleBin => unsafe {
                let mut info = RbInfo {
                    cb: std::mem::size_of::<RbInfo>() as u32,
                    size: 0,
                    items: 0,
                };
                if SHQueryRecycleBinW(std::ptr::null(), &mut info) == 0 {
                    if delete && info.items > 0 {
                        // onay sorma, ilerleme gosterme, ses calma
                        if SHEmptyRecycleBinW(std::ptr::null_mut(), std::ptr::null(), 7) == 0 {
                            st.bytes += info.size.max(0) as u64;
                            st.files += info.items.max(0) as u64;
                        }
                    } else if !delete {
                        st.bytes += info.size.max(0) as u64;
                        st.files += info.items.max(0) as u64;
                    }
                }
            },
        }
    }
    st
}

pub fn scan(c: &mut Cat) {
    let st = run(c, false);
    c.bytes = st.bytes;
    c.files = st.files;
    c.scanned = true;
}

/// Kategoriyi temizler; kazanilan bayti doner.
pub fn clean(c: &mut Cat) -> u64 {
    let st = run(c, true);
    c.freed = st.bytes;
    c.skipped = st.skipped;
    let left = run(c, false);
    c.bytes = left.bytes;
    c.files = left.files;
    st.bytes
}

fn subdirs(base: &Path) -> Vec<PathBuf> {
    fs::read_dir(base)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect()
        })
        .unwrap_or_default()
}

pub fn categories() -> Vec<Cat> {
    let local = env("LOCALAPPDATA");
    let windir = env("WINDIR");
    let pdata = env("ProgramData");
    let day = Some(Duration::from_secs(24 * 3600));
    let mut v = Vec::new();
    let mut add = |id, name, desc, on, age, targets: Vec<Target>| {
        v.push(Cat {
            id,
            name,
            desc,
            on,
            targets,
            age,
            bytes: 0,
            files: 0,
            scanned: false,
            freed: 0,
            skipped: 0,
        })
    };

    let mut t = Vec::new();
    if let Some(p) = env("TEMP") {
        t.push(Target::Dir(p));
    }
    add(
        "temp_user",
        lang::t("Kullanıcı geçici dosyaları (%TEMP%)", "User temporary files (%TEMP%)"),
        lang::t(
            "Programların bıraktığı geçici dosyalar. Son 24 saatte değişenlere dokunulmaz.",
            "Temporary files left behind by programs. Files changed in the last 24 hours are not touched.",
        ),
        true,
        day,
        t,
    );

    let mut t = Vec::new();
    if let Some(w) = &windir {
        t.push(Target::Dir(w.join("Temp")));
    }
    add(
        "temp_win",
        lang::t("Windows geçici dosyaları", "Windows temporary files"),
        lang::t(
            "C:\\Windows\\Temp. Son 24 saatte değişenlere dokunulmaz.",
            "C:\\Windows\\Temp. Files changed in the last 24 hours are not touched.",
        ),
        true,
        day,
        t,
    );

    let mut t = Vec::new();
    if let Some(w) = &windir {
        t.push(Target::Dir(w.join(r"SoftwareDistribution\Download")));
        t.push(Target::Dir(w.join(
            r"ServiceProfiles\NetworkService\AppData\Local\Microsoft\Windows\DeliveryOptimization\Cache",
        )));
    }
    add(
        "wu",
        lang::t("Windows Update indirme önbelleği", "Windows Update download cache"),
        lang::t(
            "Kurulmuş güncellemelerin indirme dosyaları. Gerekirse Windows yeniden indirir.",
            "Download files of updates that are already installed. Windows downloads them again if needed.",
        ),
        true,
        None,
        t,
    );

    let mut t = Vec::new();
    if let Some(p) = &pdata {
        t.push(Target::Dir(p.join(r"Microsoft\Windows\WER\ReportArchive")));
        t.push(Target::Dir(p.join(r"Microsoft\Windows\WER\ReportQueue")));
        t.push(Target::Dir(p.join(r"Microsoft\Windows\WER\Temp")));
    }
    if let Some(l) = &local {
        t.push(Target::Dir(l.join(r"Microsoft\Windows\WER")));
        t.push(Target::Dir(l.join("CrashDumps")));
    }
    if let Some(w) = &windir {
        t.push(Target::Dir(w.join("Minidump")));
        t.push(Target::Dir(w.join("LiveKernelReports")));
        t.push(Target::File(w.join("MEMORY.DMP")));
    }
    add(
        "dumps",
        lang::t("Hata raporları ve kilitlenme dökümleri", "Error reports and crash dumps"),
        lang::t(
            "Çöken programların ve mavi ekranların bıraktığı döküm dosyaları.",
            "Dump files left behind by crashed programs and blue screens.",
        ),
        true,
        None,
        t,
    );

    let mut t = Vec::new();
    if let Some(w) = &windir {
        t.push(Target::Dir(w.join(r"Logs\CBS")));
        t.push(Target::Dir(w.join(r"Logs\DISM")));
        t.push(Target::Dir(w.join(r"Logs\WindowsUpdate")));
    }
    add(
        "logs",
        lang::t("Windows bakım günlükleri", "Windows maintenance logs"),
        lang::t(
            "Güncelleme ve bileşen bakım günlükleri (CBS, DISM).",
            "Update and component servicing logs (CBS, DISM).",
        ),
        true,
        day,
        t,
    );

    let mut t = Vec::new();
    if let Some(l) = &local {
        t.push(Target::Dir(l.join("D3DSCache")));
        t.push(Target::Dir(l.join(r"NVIDIA\DXCache")));
        t.push(Target::Dir(l.join(r"NVIDIA\GLCache")));
    }
    if let Some(p) = &pdata {
        t.push(Target::Dir(p.join(r"NVIDIA Corporation\NV_Cache")));
    }
    add(
        "shader",
        lang::t("Ekran kartı gölgelendirici önbelleği", "Graphics card shader cache"),
        lang::t(
            "DirectX ve NVIDIA derlenmiş gölgelendiricileri. Oyun/uygulama ilk açılışta yeniden oluşturur.",
            "Compiled DirectX and NVIDIA shaders. Games/apps rebuild them the first time they start.",
        ),
        true,
        None,
        t,
    );

    let mut t = Vec::new();
    if let Some(l) = &local {
        let ex = l.join(r"Microsoft\Windows\Explorer");
        t.push(Target::Files(ex.clone(), "thumbcache_", ".db"));
        t.push(Target::Files(ex, "iconcache_", ".db"));
    }
    add(
        "thumbs",
        lang::t("Küçük resim ve simge önbelleği", "Thumbnail and icon cache"),
        lang::t(
            "Gezgin küçük resimleri. Kullanımda olanlar atlanır, gerisi yeniden oluşturulur.",
            "File Explorer thumbnails. Files in use are skipped; the rest are rebuilt.",
        ),
        true,
        None,
        t,
    );

    let mut t = Vec::new();
    if let Some(l) = &local {
        for base in [
            r"Google\Chrome\User Data",
            r"Microsoft\Edge\User Data",
            r"BraveSoftware\Brave-Browser\User Data",
            r"Vivaldi\User Data",
            r"Yandex\YandexBrowser\User Data",
        ] {
            let b = l.join(base);
            for prof in subdirs(&b) {
                if prof.join("Cache").is_dir() || prof.join("Code Cache").is_dir() {
                    t.push(Target::Dir(prof.join("Cache")));
                    t.push(Target::Dir(prof.join("Code Cache")));
                    t.push(Target::Dir(prof.join("GPUCache")));
                }
            }
            t.push(Target::Dir(b.join("ShaderCache")));
            t.push(Target::Dir(b.join("GrShaderCache")));
        }
        for prof in subdirs(&l.join(r"Mozilla\Firefox\Profiles")) {
            t.push(Target::Dir(prof.join("cache2")));
        }
    }
    add(
        "browser",
        lang::t("Tarayıcı önbellekleri", "Browser caches"),
        lang::t(
            "Chrome, Edge, Brave, Firefox vb. önbellek dosyaları. Geçmiş, şifreler ve oturumlar silinmez.",
            "Cache files of Chrome, Edge, Brave, Firefox etc. History, passwords and sessions are not deleted.",
        ),
        true,
        None,
        t,
    );

    let mut t = Vec::new();
    if let Some(l) = &local {
        t.push(Target::Dir(l.join(r"npm-cache\_cacache")));
        t.push(Target::Dir(l.join(r"pip\cache")));
        t.push(Target::Dir(l.join(r"NuGet\v3-cache")));
        t.push(Target::Dir(l.join("go-build")));
        t.push(Target::Dir(l.join(r"Yarn\Cache")));
    }
    add(
        "dev",
        lang::t("Geliştirici paket önbellekleri", "Developer package caches"),
        lang::t(
            "npm, pip, NuGet, Yarn, Go derleme önbellekleri. Sonraki kurulumda yeniden indirilir (ilk derleme yavaşlar).",
            "npm, pip, NuGet, Yarn and Go build caches. Downloaded again on the next install (the first build is slower).",
        ),
        false,
        None,
        t,
    );

    add(
        "recycle",
        lang::t("Geri Dönüşüm Kutusu", "Recycle Bin"),
        lang::t(
            "Sildiğiniz dosyalar kalıcı olarak silinir; geri alınamaz.",
            "The files you deleted are removed permanently; this cannot be undone.",
        ),
        false,
        None,
        vec![Target::RecycleBin],
    );

    v
}

/// (ad, toplam, bos) — sabit diskler.
pub fn drives() -> Vec<(String, u64, u64)> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let mut v: Vec<(String, u64, u64)> = disks
        .iter()
        .filter(|d| d.total_space() > 0)
        .map(|d| {
            (
                d.mount_point().to_string_lossy().to_string(),
                d.total_space(),
                d.available_space(),
            )
        })
        .collect();
    v.sort();
    v.dedup_by(|a, b| a.0 == b.0);
    v
}
