//! Kurulum programi olmadan kurulum. Yayimlanan surum tek bir HFixWin11.exe dosyasidir. Nereden
//! calistirilirsa calistirilsin kendini %LOCALAPPDATA%\HFixWin11 klasorune kopyalar, masaustune ve
//! Baslat menusune kisayol koyar ve o kopyayi baslatir. Oradan calisirken Windows'un "Yuklu
//! uygulamalar" listesinde bir kayit tutar; listedeki Kaldir dugmesi "HFixWin11.exe --kaldir"
//! komutunu calistirir.

use crate::{lang, reg, tray, update, win};
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::ptr::null_mut;

const DIR_NAME: &str = "HFixWin11";
const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\HFixWin11";
/// Kisayol sistem dilinde adlandirilir; diger dildeki eski kisayol silinir.
const NAMES: [&str; 2] = ["HFix Windows 11 Araçları", "HFix Windows 11 Tools"];
/// Eski surumun (Fan Bekcisi) program klasorunden tasinan dosyalar.
const DATA_FILES: [&str; 4] = ["ayarlar.txt", "ayar-yedek.txt", "servis-yedek.txt", "guvenilir.txt"];

pub fn install_dir() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join(DIR_NAME))
}

fn installed_exe() -> Option<PathBuf> {
    Some(install_dir()?.join(update::ASSET_EXE))
}

fn same_path(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy())
}

/// Bu surec kurulu kopya mi?
pub fn is_installed_copy() -> bool {
    match (std::env::current_exe(), installed_exe()) {
        (Ok(a), Some(b)) => same_path(&a, &b),
        _ => false,
    }
}

fn version_file(dir: &Path) -> PathBuf {
    dir.join("surum.txt")
}

fn installed_version(dir: &Path) -> String {
    std::fs::read_to_string(version_file(dir))
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

fn same_content(a: &Path, b: &Path) -> bool {
    match (std::fs::read(a), std::fs::read(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

fn copy_dir(from: &Path, to: &Path) {
    let _ = std::fs::create_dir_all(to);
    for e in std::fs::read_dir(from).into_iter().flatten().flatten() {
        let (src, dst) = (e.path(), to.join(e.file_name()));
        if src.is_dir() {
            copy_dir(&src, &dst);
        } else {
            let _ = std::fs::copy(&src, &dst);
        }
    }
}

/// Ilk calisan adim. Kurulum klasorunun disindaysa bu kopyayi kurar (kurulu olan ayni ya da daha
/// yeni degilse), kisayollari olusturur ve kurulu kopyayi baslatir. true donerse bu surec isini
/// devretmistir ve cikmalidir.
pub fn self_install(args: &[String]) -> bool {
    let (Ok(me), Some(dir), Some(target)) = (std::env::current_exe(), install_dir(), installed_exe()) else {
        return false;
    };
    if same_path(&me, &target) || std::fs::create_dir_all(&dir).is_err() {
        return false;
    }
    let order = update::compare(&installed_version(&dir), update::VERSION);
    let newer = order == std::cmp::Ordering::Less;
    if !target.exists() || newer || (order == std::cmp::Ordering::Equal && !same_content(&me, &target)) {
        // Calisan kopya degistirilmek uzere olan kurulu kopya olabilir.
        tray::quit_existing();
        let tmp = dir.join(format!("{}.new", update::ASSET_EXE));
        if std::fs::copy(&me, &tmp).is_err() {
            return false;
        }
        if update::replace_file(&tmp, &target).is_err() {
            let _ = std::fs::remove_file(&tmp);
            return false; // buradan calis
        }
        let _ = std::fs::write(version_file(&dir), update::VERSION);
    }
    // Tasinabilir kullanilan eski kopyanin ayarlari ve yedekleri kaybolmasin.
    if let Some(old) = me.parent() {
        for f in DATA_FILES {
            if old.join(f).is_file() && !dir.join(f).exists() {
                let _ = std::fs::copy(old.join(f), dir.join(f));
            }
        }
        if old.join("karantina").is_dir() && !dir.join("karantina").exists() {
            copy_dir(&old.join("karantina"), &dir.join("karantina"));
        }
    }
    create_shortcuts();
    std::process::Command::new(&target).args(&args[1..]).spawn().is_ok()
}

/// Kurulu kopyanin her acilistaki bakimi.
pub fn upkeep() {
    update::cleanup_old();
    let (Some(dir), Ok(exe)) = (install_dir(), std::env::current_exe()) else {
        return;
    };
    if !is_installed_copy() {
        return;
    }
    if installed_version(&dir) != update::VERSION {
        let _ = std::fs::write(version_file(&dir), update::VERSION);
    }
    write_icon();
    register_uninstall(&exe);
}

fn icon_path() -> Option<PathBuf> {
    Some(install_dir()?.join("hfix.ico"))
}

fn write_icon() {
    if let Some(p) = icon_path() {
        let data = crate::icon::ico();
        if std::fs::read(&p).ok().as_deref() != Some(&data[..]) {
            let _ = std::fs::write(p, data);
        }
    }
}

fn register_uninstall(exe: &Path) {
    let Some(k) = reg::Key::create(reg::HKCU, UNINSTALL_KEY) else {
        return;
    };
    let exe_s = exe.to_string_lossy();
    k.set_str("DisplayName", shortcut_name());
    k.set_str("DisplayVersion", update::VERSION);
    k.set_str("Publisher", "HukukFix");
    if let Some(i) = icon_path() {
        k.set_str("DisplayIcon", &i.to_string_lossy());
    }
    if let Some(d) = exe.parent() {
        k.set_str("InstallLocation", &d.to_string_lossy());
    }
    k.set_str("UninstallString", &format!("\"{exe_s}\" --kaldir"));
    k.set_str("QuietUninstallString", &format!("\"{exe_s}\" --kaldir --evet"));
    k.set_str("URLInfoAbout", &update::project_page());
    k.set_u32("NoModify", 1);
    k.set_u32("NoRepair", 1);
    if let Ok(m) = std::fs::metadata(exe) {
        k.set_u32("EstimatedSize", (m.len() / 1024) as u32);
    }
}

// ------------------------------------------------- kisayollar

fn shortcut_name() -> &'static str {
    NAMES[!lang::system_turkish() as usize]
}

#[repr(C)]
struct Guid(u32, u16, u16, [u8; 8]);

const OLE: [u8; 8] = [0xC0, 0, 0, 0, 0, 0, 0, 0x46];
const CLSID_SHELL_LINK: Guid = Guid(0x0002_1401, 0, 0, OLE);
const IID_SHELL_LINK_W: Guid = Guid(0x0002_14F9, 0, 0, OLE);
const IID_PERSIST_FILE: Guid = Guid(0x0000_010B, 0, 0, OLE);
const FOLDERID_DESKTOP: Guid = Guid(0xB4BF_CC3A, 0xDB2C, 0x424C, [0xB0, 0x29, 0x7F, 0xE9, 0x9A, 0x87, 0xC6, 0x41]);
const FOLDERID_PROGRAMS: Guid = Guid(0xA77F_5D77, 0x2E2B, 0x44C3, [0xA6, 0xA2, 0xAB, 0xA6, 0x01, 0x05, 0x4A, 0x51]);

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn known_folder(id: &Guid) -> Option<PathBuf> {
    unsafe {
        let f: unsafe extern "system" fn(*const Guid, u32, *mut c_void, *mut *mut u16) -> i32 =
            std::mem::transmute(win::dll_fn("shell32.dll", b"SHGetKnownFolderPath\0")?);
        let mut p: *mut u16 = null_mut();
        if f(id, 0, null_mut(), &mut p) != 0 || p.is_null() {
            return None;
        }
        let len = (0..).take_while(|&i| *p.add(i) != 0).count();
        Some(PathBuf::from(String::from_utf16_lossy(std::slice::from_raw_parts(p, len))))
    }
}

/// Masaustu ve Baslat menusunun Programlar klasoru. Test: FB_KISAYOL baska bir klasore yonlendirir.
fn shortcut_dirs() -> Vec<PathBuf> {
    if let Some(d) = std::env::var_os("FB_KISAYOL") {
        return vec![PathBuf::from(d)];
    }
    [&FOLDERID_DESKTOP, &FOLDERID_PROGRAMS]
        .into_iter()
        .filter_map(known_folder)
        .collect()
}

/// Kurulu kopya icin masaustune ve Baslat menusune kisayol koyar. Olusturulan kisayol sayisini doner.
pub fn create_shortcuts() -> usize {
    let Some(target) = installed_exe() else {
        return 0;
    };
    write_icon();
    let name = shortcut_name();
    let mut made = 0;
    for dir in shortcut_dirs() {
        for n in NAMES {
            if n != name {
                let _ = std::fs::remove_file(dir.join(format!("{n}.lnk")));
            }
        }
        made += write_shortcut(&dir.join(format!("{name}.lnk")), &target, name) as usize;
    }
    made
}

fn remove_shortcuts() {
    for dir in shortcut_dirs() {
        for n in NAMES {
            let _ = std::fs::remove_file(dir.join(format!("{n}.lnk")));
        }
    }
}

unsafe fn com(obj: *mut c_void, method: usize, a: usize, b: usize) -> i32 {
    let vtbl = *(obj as *const *const usize);
    let f: unsafe extern "system" fn(*mut c_void, usize, usize) -> i32 = std::mem::transmute(*vtbl.add(method));
    f(obj, a, b)
}

/// Windows kabugu (IShellLink) ile .lnk dosyasi olusturur; COM kendi is parcaciginda acilir ki
/// pencerenin kullandigi ortamla karismasin.
fn write_shortcut(lnk: &Path, target: &Path, desc: &str) -> bool {
    let (lnk, target, desc) = (lnk.to_path_buf(), target.to_path_buf(), desc.to_string());
    std::thread::spawn(move || unsafe {
        let load = |name: &[u8]| win::dll_fn("ole32.dll", name);
        let (Some(init), Some(create), Some(uninit)) =
            (load(b"CoInitializeEx\0"), load(b"CoCreateInstance\0"), load(b"CoUninitialize\0"))
        else {
            return false;
        };
        let init: unsafe extern "system" fn(*mut c_void, u32) -> i32 = std::mem::transmute(init);
        let create: unsafe extern "system" fn(*const Guid, *mut c_void, u32, *const Guid, *mut *mut c_void) -> i32 =
            std::mem::transmute(create);
        let uninit: unsafe extern "system" fn() = std::mem::transmute(uninit);
        if init(null_mut(), 2) < 0 {
            return false;
        }
        let mut ok = false;
        let mut link: *mut c_void = null_mut();
        if create(&CLSID_SHELL_LINK, null_mut(), 1, &IID_SHELL_LINK_W, &mut link) == 0 && !link.is_null() {
            let t = wide(&target.to_string_lossy());
            let d = wide(&target.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default());
            let desc = wide(&desc);
            com(link, 20, t.as_ptr() as usize, 0); // SetPath
            com(link, 9, d.as_ptr() as usize, 0); // SetWorkingDirectory
            com(link, 7, desc.as_ptr() as usize, 0); // SetDescription
            // Simge: kurulum klasorundeki .ico (exe'ye kaynak gomulmedigi icin).
            let icon = icon_path().filter(|p| p.exists()).unwrap_or(target.clone());
            let icon = wide(&icon.to_string_lossy());
            com(link, 17, icon.as_ptr() as usize, 0); // SetIconLocation
            let mut file: *mut c_void = null_mut();
            if com(link, 0, &IID_PERSIST_FILE as *const Guid as usize, &mut file as *mut _ as usize) == 0
                && !file.is_null()
            {
                let l = wide(&lnk.to_string_lossy());
                ok = com(file, 6, l.as_ptr() as usize, 1) == 0; // IPersistFile::Save
                com(file, 2, 0, 0); // Release
            }
            com(link, 2, 0, 0);
        }
        uninit();
        ok
    })
    .join()
    .unwrap_or(false)
}

// ------------------------------------------------- kaldirma

#[link(name = "user32")]
extern "system" {
    fn MessageBoxW(hwnd: *mut c_void, text: *const u16, title: *const u16, flags: u32) -> i32;
}

fn message(text: &str, flags: u32) -> i32 {
    unsafe { MessageBoxW(null_mut(), wide(text).as_ptr(), wide(shortcut_name()).as_ptr(), flags) }
}

/// Programin bu bilgisayara koydugu her seyi kaldirir. Uygulanmis Windows ayarlari oldugu gibi
/// kalir; onlar programdan "Geri al" ile geri alinir.
pub fn uninstall(quiet: bool) {
    let tr = lang::system_turkish();
    if !quiet {
        let q = if tr {
            "HFix Windows 11 Araçları kaldırılsın mı?\n\nProgram, ayarları, yedek dosyaları ve karantinadaki dosyalar silinir. Uyguladığınız Windows ayarları olduğu gibi kalır; geri almak istiyorsanız önce programdan \"Geri al\" ile geri alın."
        } else {
            "Remove HFix Windows 11 Tools?\n\nThe program, its settings, backup files and quarantined files are deleted. The Windows settings you applied stay as they are; to revert them, use \"Undo\" in the program first."
        };
        // MB_YESNO | MB_ICONQUESTION | MB_DEFBUTTON2; 6 = Evet
        if message(q, 0x4 | 0x20 | 0x100) != 6 {
            return;
        }
    }
    tray::quit_existing();
    remove_shortcuts();
    win::autostart_set(false);
    reg::Key::delete_tree(reg::HKCU, UNINSTALL_KEY);
    if let (Some(dir), Ok(me)) = (install_dir(), std::env::current_exe()) {
        for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let p = e.path();
            if same_path(&p, &me) {
                continue;
            }
            let _ = if p.is_dir() { std::fs::remove_dir_all(&p) } else { std::fs::remove_file(&p) };
        }
        if me.starts_with(&dir) {
            // Calisan program kendini silemez: cmd.exe hemen ardindan siler.
            use std::os::windows::process::CommandExt;
            let _ = std::process::Command::new("cmd.exe")
                .raw_arg(format!(
                    "/d /c ping -n 3 127.0.0.1 >nul & del /f /q \"{}\" & rmdir \"{}\"",
                    me.display(),
                    dir.display()
                ))
                .creation_flags(0x0800_0000)
                .spawn();
        }
    }
    if !quiet {
        message(
            if tr { "HFix Windows 11 Araçları kaldırıldı." } else { "HFix Windows 11 Tools has been removed." },
            0x40,
        );
    }
}

// ------------------------------------------------- programin kendini tanimasi

/// Bu programin kendisi mi? Kendi klasorundeki ya da kurulum klasorundeki dosyalar ve baska bir
/// yerde duran birebir ayni kopyasi (ornegin indirilen exe) imzasiz / supheli sayilmaz. Yalnizca
/// ada bakilmaz: ayni adi tasiyan baska bir dosya bu yolla gizlenemez.
pub fn is_self(p: &str) -> bool {
    use std::collections::HashMap;
    use std::sync::Mutex;
    let p = p.trim().trim_matches('"');
    if p.is_empty() {
        return false;
    }
    let low = p.to_lowercase();
    let inside = |d: Option<PathBuf>| {
        d.is_some_and(|d| {
            let d = d.to_string_lossy().to_lowercase();
            !d.is_empty() && low.starts_with(&format!("{}\\", d.trim_end_matches('\\')))
        })
    };
    if inside(crate::cfg::dir()) || inside(install_dir()) {
        return true;
    }
    // Icerik karsilastirmasi pahalidir; sonuc dosya basina (boyut ve degisme zamaniyla) saklanir.
    static CACHE: Mutex<Option<HashMap<String, (u64, Option<std::time::SystemTime>, bool)>>> = Mutex::new(None);
    let (Ok(me), Ok(meta)) = (std::env::current_exe(), std::fs::metadata(p)) else {
        return false;
    };
    if std::fs::metadata(&me).map(|m| m.len()).ok() != Some(meta.len()) {
        return false;
    }
    let stamp = (meta.len(), meta.modified().ok());
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let map = cache.get_or_insert_with(HashMap::new);
    if let Some((len, time, same)) = map.get(&low) {
        if (*len, *time) == stamp {
            return *same;
        }
    }
    let same = same_content(&me, Path::new(p));
    map.insert(low, (stamp.0, stamp.1, same));
    same
}