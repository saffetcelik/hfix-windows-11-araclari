//! Sistem tepsisi simgesi: pencere kapatilinca program burada calismaya devam eder.
#![allow(non_snake_case)]

use crate::mon::Cmd;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::mpsc::Sender;

type H = isize;
type WndProc = unsafe extern "system" fn(H, u32, usize, isize) -> isize;

const CLASS: &str = "HFixWin11Tray";
/// Onceki adla (Fan Bekcisi) calisan surumun tepsi penceresi.
const OLD_CLASS: &str = "FanBekcisiTray";
/// Ana pencerenin basligi; pencere bununla bulunur, bu yuzden acilista bir kez belirlenir.
static TITLE: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();

pub fn window_title() -> &'static str {
    TITLE.get_or_init(crate::lang::app_name)
}
const WM_TRAY: u32 = 0x8001;
/// Ikinci kez calistirilan kopya, acik olan pencereyi bununla one getirir.
const WM_SHOW: u32 = 0x8002;
/// Yeni surumu kuran kopya, calisan kopyadan bununla cikmasini ister.
const WM_EXIT: u32 = 0x8003;

#[repr(C)]
struct WndClass {
    style: u32,
    proc: WndProc,
    cls_extra: i32,
    wnd_extra: i32,
    inst: H,
    icon: H,
    cursor: H,
    bg: H,
    menu: *const u16,
    name: *const u16,
}
#[repr(C)]
struct Msg {
    hwnd: H,
    msg: u32,
    wp: usize,
    lp: isize,
    time: u32,
    pt: [i32; 2],
    private: u32,
}
#[repr(C)]
struct Nid {
    cb: u32,
    hwnd: H,
    id: u32,
    flags: u32,
    cbmsg: u32,
    icon: H,
    tip: [u16; 128],
    state: u32,
    mask: u32,
    info: [u16; 256],
    ver: u32,
    title: [u16; 64],
    iflags: u32,
    guid: [u32; 4],
    balloon: H,
}

#[link(name = "user32")]
extern "system" {
    fn RegisterClassW(c: *const WndClass) -> u16;
    fn CreateWindowExW(
        ex: u32,
        cls: *const u16,
        name: *const u16,
        style: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        parent: H,
        menu: H,
        inst: H,
        param: *mut u8,
    ) -> H;
    fn DefWindowProcW(h: H, m: u32, w: usize, l: isize) -> isize;
    fn GetMessageW(m: *mut Msg, h: H, min: u32, max: u32) -> i32;
    fn TranslateMessage(m: *const Msg) -> i32;
    fn DispatchMessageW(m: *const Msg) -> isize;
    fn PostMessageW(h: H, m: u32, w: usize, l: isize) -> i32;
    fn FindWindowW(cls: *const u16, name: *const u16) -> H;
    fn ShowWindow(h: H, cmd: i32) -> i32;
    fn SetForegroundWindow(h: H) -> i32;
    fn IsIconic(h: H) -> i32;
    fn CreatePopupMenu() -> H;
    fn AppendMenuW(menu: H, flags: u32, id: usize, text: *const u16) -> i32;
    fn TrackPopupMenu(menu: H, flags: u32, x: i32, y: i32, r: i32, hwnd: H, rect: *const u8) -> i32;
    fn DestroyMenu(menu: H) -> i32;
    fn GetCursorPos(p: *mut [i32; 2]) -> i32;
    fn CreateIcon(inst: H, w: i32, h: i32, planes: u8, bpp: u8, and: *const u8, xor: *const u8) -> H;
    fn GetWindowThreadProcessId(h: H, pid: *mut u32) -> u32;
    fn ChangeWindowMessageFilterEx(h: H, msg: u32, action: u32, info: *mut u8) -> i32;
}
#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(name: *const u16) -> H;
}
#[link(name = "shell32")]
extern "system" {
    fn Shell_NotifyIconW(msg: u32, data: *const Nid) -> i32;
}

static TRAY_HWND: AtomicIsize = AtomicIsize::new(0);
static mut TX: Option<Sender<Cmd>> = None;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn main_hwnd() -> H {
    unsafe {
        let h = FindWindowW(null(), wide(window_title()).as_ptr());
        if h == 0 {
            return 0;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(h, &mut pid);
        if pid == crate::win::own_pid() {
            h
        } else {
            0
        }
    }
}

/// Ana pencereyi gizler (program tepside calismayi surdurur).
pub fn hide_main() {
    let h = main_hwnd();
    if h != 0 {
        unsafe {
            ShowWindow(h, 0);
        }
    }
}

pub fn show_main() {
    let h = main_hwnd();
    if h != 0 {
        unsafe {
            ShowWindow(h, if IsIconic(h) != 0 { 9 } else { 5 });
            SetForegroundWindow(h);
        }
    }
}

/// Program zaten aciksa penceresini one getirir ve true doner.
pub fn signal_existing() -> bool {
    unsafe {
        let h = FindWindowW(wide(CLASS).as_ptr(), null());
        h != 0 && PostMessageW(h, WM_SHOW, 0, 0) != 0
    }
}

/// Calisan kopya (eski adli surum dahil) varsa cikmasini ister ve kapanmasini bekler.
pub fn quit_existing() {
    unsafe {
        for (class, msg) in [(CLASS, WM_EXIT), (OLD_CLASS, WM_EXIT)] {
            let h = FindWindowW(wide(class).as_ptr(), null());
            if h == 0 {
                continue;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(h, &mut pid);
            if pid != 0 && pid != crate::win::own_pid() && PostMessageW(h, msg, 0, 0) != 0 {
                crate::win::wait_pid(pid, 8000);
            }
        }
    }
}

fn nid(hwnd: H) -> Nid {
    let mut n: Nid = unsafe { std::mem::zeroed() };
    n.cb = std::mem::size_of::<Nid>() as u32;
    n.hwnd = hwnd;
    n.id = 1;
    n
}

/// Tepsi simgesini kaldirir ve programdan cikar.
pub fn quit() -> ! {
    let h = TRAY_HWND.load(Ordering::Relaxed);
    if h != 0 {
        unsafe {
            Shell_NotifyIconW(2, &nid(h));
        }
    }
    std::process::exit(0)
}

/// 32x32 program simgesi (BGRA).
fn icon() -> H {
    let mut px = crate::icon::rgba(32);
    for p in px.chunks_exact_mut(4) {
        p.swap(0, 2);
    }
    let and = vec![0u8; 32 * 32 / 8];
    unsafe { CreateIcon(GetModuleHandleW(null()), 32, 32, 1, 32, and.as_ptr(), px.as_ptr()) }
}

unsafe extern "system" fn wndproc(h: H, m: u32, w: usize, l: isize) -> isize {
    match m {
        WM_SHOW => show_main(),
        WM_EXIT => quit(),
        WM_TRAY => match l as u32 & 0xFFFF {
            0x202 | 0x203 => show_main(),
            0x205 => {
                let menu = CreatePopupMenu();
                AppendMenuW(menu, 0, 1, wide(crate::lang::t("Pencereyi aç", "Open window")).as_ptr());
                AppendMenuW(menu, 0, 2, wide(crate::lang::t("RAM'i temizle", "Clean RAM")).as_ptr());
                AppendMenuW(menu, 0x800, 0, null());
                AppendMenuW(menu, 0, 3, wide(crate::lang::t("Çıkış", "Exit")).as_ptr());
                let mut pt = [0i32; 2];
                GetCursorPos(&mut pt);
                SetForegroundWindow(h);
                let cmd = TrackPopupMenu(menu, 0x100 | 0x2, pt[0], pt[1], 0, h, null());
                DestroyMenu(menu);
                match cmd {
                    1 => show_main(),
                    2 => {
                        #[allow(static_mut_refs)]
                        if let Some(tx) = TX.as_ref() {
                            let _ = tx.send(Cmd::CleanRam);
                        }
                    }
                    3 => quit(),
                    _ => {}
                }
            }
            _ => {}
        },
        _ => return DefWindowProcW(h, m, w, l),
    }
    0
}

/// Tepsi simgesini kendi is parcaciginda olusturur.
pub fn start(tx: Sender<Cmd>) {
    std::thread::spawn(move || unsafe {
        TX = Some(tx);
        let inst = GetModuleHandleW(null());
        let cls = wide(CLASS);
        let wc = WndClass {
            style: 0,
            proc: wndproc,
            cls_extra: 0,
            wnd_extra: 0,
            inst,
            icon: 0,
            cursor: 0,
            bg: 0,
            menu: null(),
            name: cls.as_ptr(),
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(0, cls.as_ptr(), cls.as_ptr(), 0, 0, 0, 0, 0, 0, 0, inst, null_mut());
        if hwnd == 0 {
            return;
        }
        // Yonetici olmayan ikinci kopyanin "pencereyi goster" iletisine izin ver.
        ChangeWindowMessageFilterEx(hwnd, WM_SHOW, 1, null_mut());
        ChangeWindowMessageFilterEx(hwnd, WM_EXIT, 1, null_mut());
        TRAY_HWND.store(hwnd, Ordering::Relaxed);
        let mut n = nid(hwnd);
        n.flags = 0x1 | 0x2 | 0x4;
        n.cbmsg = WM_TRAY;
        n.icon = icon();
        for (i, c) in crate::lang::app_name().encode_utf16().take(127).enumerate() {
            n.tip[i] = c;
        }
        Shell_NotifyIconW(0, &n);
        let mut msg: Msg = std::mem::zeroed();
        while GetMessageW(&mut msg, 0, 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
}
