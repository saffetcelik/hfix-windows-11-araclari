//! Windows API baglantilari (ek kutuphane kullanmadan, dogrudan FFI).
#![allow(non_snake_case, clippy::missing_safety_doc)]

use std::collections::HashMap;
use std::ffi::c_void;
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::ptr::{null, null_mut};

pub type Handle = *mut c_void;

#[repr(C)]
struct Luid {
    low: u32,
    high: i32,
}
#[repr(C)]
struct TokenPrivileges {
    count: u32,
    luid: Luid,
    attrs: u32,
}
#[repr(C)]
struct PowerThrottling {
    version: u32,
    control_mask: u32,
    state_mask: u32,
}

#[repr(C)]
struct PerfInfo {
    cb: u32,
    commit_total: usize,
    commit_limit: usize,
    commit_peak: usize,
    phys_total: usize,
    phys_avail: usize,
    system_cache: usize,
    kernel_total: usize,
    kernel_paged: usize,
    kernel_nonpaged: usize,
    page_size: usize,
    handles: u32,
    processes: u32,
    threads: u32,
}

/// Windows dosya onbelleginin (bekleme listesi) yaklasik boyutu, bayt.
pub fn mem_cache() -> u64 {
    unsafe {
        let mut pi: PerfInfo = std::mem::zeroed();
        pi.cb = std::mem::size_of::<PerfInfo>() as u32;
        if K32GetPerformanceInfo(&mut pi, pi.cb) == 0 {
            return 0;
        }
        pi.system_cache as u64 * pi.page_size as u64
    }
}

#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
    fn CloseHandle(h: Handle) -> i32;
    fn GetCurrentProcess() -> Handle;
    fn GetCurrentProcessId() -> u32;
    fn K32EmptyWorkingSet(h: Handle) -> i32;
    fn SetPriorityClass(h: Handle, class: u32) -> i32;
    fn SetProcessInformation(h: Handle, class: i32, info: *const c_void, size: u32) -> i32;
    fn TerminateProcess(h: Handle, code: u32) -> i32;
    fn GetLocalTime(st: *mut [u16; 8]);
    fn K32GetPerformanceInfo(info: *mut PerfInfo, cb: u32) -> i32;
    fn MultiByteToWideChar(cp: u32, flags: u32, s: *const u8, n: i32, out: *mut u16, cap: i32) -> i32;
}
#[link(name = "advapi32")]
extern "system" {
    fn OpenProcessToken(h: Handle, access: u32, token: *mut Handle) -> i32;
    fn LookupPrivilegeValueW(sys: *const u16, name: *const u16, luid: *mut Luid) -> i32;
    fn AdjustTokenPrivileges(
        token: Handle,
        disable_all: i32,
        new: *const TokenPrivileges,
        len: u32,
        prev: *mut c_void,
        ret_len: *mut u32,
    ) -> i32;
}
#[link(name = "ntdll")]
extern "system" {
    fn NtSetSystemInformation(class: i32, info: *mut c_void, len: u32) -> i32;
}
#[link(name = "shell32")]
extern "system" {
    fn IsUserAnAdmin() -> i32;
    fn ShellExecuteW(
        hwnd: Handle,
        op: *const u16,
        file: *const u16,
        params: *const u16,
        dir: *const u16,
        show: i32,
    ) -> isize;
}
#[link(name = "iphlpapi")]
extern "system" {
    fn GetExtendedTcpTable(
        table: *mut c_void,
        size: *mut u32,
        order: i32,
        af: u32,
        class: i32,
        reserved: u32,
    ) -> u32;
    fn SetPerTcpConnectionEStats(
        row: *const Row4,
        kind: i32,
        rw: *const u8,
        rw_version: u32,
        rw_size: u32,
        offset: u32,
    ) -> u32;
    fn GetPerTcpConnectionEStats(
        row: *const Row4,
        kind: i32,
        rw: *mut u8,
        rw_version: u32,
        rw_size: u32,
        ros: *mut u8,
        ros_version: u32,
        ros_size: u32,
        rod: *mut u8,
        rod_version: u32,
        rod_size: u32,
    ) -> u32;
    fn SetPerTcp6ConnectionEStats(
        row: *const Row6,
        kind: i32,
        rw: *const u8,
        rw_version: u32,
        rw_size: u32,
        offset: u32,
    ) -> u32;
    fn GetPerTcp6ConnectionEStats(
        row: *const Row6,
        kind: i32,
        rw: *mut u8,
        rw_version: u32,
        rw_size: u32,
        ros: *mut u8,
        ros_version: u32,
        ros_size: u32,
        rod: *mut u8,
        rod_version: u32,
        rod_size: u32,
    ) -> u32;
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn is_admin() -> bool {
    unsafe { IsUserAnAdmin() != 0 }
}

pub fn own_pid() -> u32 {
    unsafe { GetCurrentProcessId() }
}

/// Programi yonetici olarak yeniden baslatir. UAC onaylanirsa true doner.
pub fn relaunch_as_admin(args: &[String]) -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let exe = wide(&exe.to_string_lossy());
    let op = wide("runas");
    let params: Vec<String> = args.iter().map(|a| format!("\"{}\"", a.replace('"', ""))).collect();
    let params = wide(&params.join(" "));
    unsafe { ShellExecuteW(null_mut(), op.as_ptr(), exe.as_ptr(), params.as_ptr(), null(), 1) > 32 }
}

pub fn clock() -> String {
    let mut st = [0u16; 8];
    unsafe { GetLocalTime(&mut st) };
    format!("{:02}:{:02}:{:02}", st[4], st[5], st[6])
}

pub fn enable_privileges() {
    unsafe {
        let mut token: Handle = null_mut();
        if OpenProcessToken(GetCurrentProcess(), 0x20 | 0x8, &mut token) == 0 {
            return;
        }
        for name in [
            "SeDebugPrivilege",
            "SeProfileSingleProcessPrivilege",
            "SeIncreaseQuotaPrivilege",
        ] {
            let w = wide(name);
            let mut luid = Luid { low: 0, high: 0 };
            if LookupPrivilegeValueW(null(), w.as_ptr(), &mut luid) != 0 {
                let tp = TokenPrivileges {
                    count: 1,
                    luid,
                    attrs: 2,
                };
                AdjustTokenPrivileges(token, 0, &tp, 0, null_mut(), null_mut());
            }
        }
        CloseHandle(token);
    }
}

/// Surecin calisma kumesini (RAM) bosaltir.
pub fn trim_process(pid: u32) -> bool {
    unsafe {
        let h = OpenProcess(0x0100 | 0x1000, 0, pid);
        if h.is_null() {
            return false;
        }
        let ok = K32EmptyWorkingSet(h) != 0;
        CloseHandle(h);
        ok
    }
}

/// Bekleme (standby) ve degistirilmis sayfa listelerini bosaltir. Yonetici gerekir.
pub fn purge_standby() -> bool {
    unsafe {
        let mut flush: i32 = 3; // MemoryFlushModifiedList
        NtSetSystemInformation(80, &mut flush as *mut i32 as *mut c_void, 4);
        let mut purge: i32 = 4; // MemoryPurgeStandbyList
        NtSetSystemInformation(80, &mut purge as *mut i32 as *mut c_void, 4) >= 0
    }
}

/// on=true: bosta onceligi + Windows verimlilik modu (EcoQoS). on=false: normale dondur.
pub fn throttle(pid: u32, on: bool) -> bool {
    unsafe {
        let h = OpenProcess(0x0200 | 0x1000, 0, pid);
        if h.is_null() {
            return false;
        }
        let ok = SetPriorityClass(h, if on { 0x40 } else { 0x20 }) != 0;
        let st = PowerThrottling {
            version: 1,
            control_mask: 1,
            state_mask: if on { 1 } else { 0 },
        };
        SetProcessInformation(h, 4, &st as *const _ as *const c_void, 12);
        CloseHandle(h);
        ok
    }
}

pub fn kill(pid: u32) -> bool {
    unsafe {
        let h = OpenProcess(0x0001, 0, pid);
        if h.is_null() {
            return false;
        }
        let ok = TerminateProcess(h, 1) != 0;
        CloseHandle(h);
        ok
    }
}

/// Komutu pencere acmadan calistirir, ciktisini doner.
pub fn run_hidden(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd)
        .args(args)
        .creation_flags(0x0800_0000)
        .output()
        .ok()?;
    Some(decode(&out.stdout))
}

/// Konsol ciktisini metne cevirir: UTF-8 degilse konsol (OEM) kod sayfasindan.
fn decode(b: &[u8]) -> String {
    if let Ok(s) = std::str::from_utf8(b) {
        return s.to_string();
    }
    unsafe {
        let n = MultiByteToWideChar(1, 0, b.as_ptr(), b.len() as i32, null_mut(), 0);
        if n <= 0 {
            return String::from_utf8_lossy(b).into_owned();
        }
        let mut w = vec![0u16; n as usize];
        MultiByteToWideChar(1, 0, b.as_ptr(), b.len() as i32, w.as_mut_ptr(), n);
        String::from_utf16_lossy(&w)
    }
}

/// Komutu pencere acmadan calistirir; (cikis kodu, cikti) doner.
pub fn run_code(cmd: &str, args: &[&str]) -> Option<(i32, String)> {
    let out = Command::new(cmd)
        .args(args)
        .creation_flags(0x0800_0000)
        .output()
        .ok()?;
    Some((out.status.code().unwrap_or(-1), decode(&out.stdout)))
}

/// Komutu pencere acmadan calistirir; basariyla bittiyse true.
pub fn run_status(cmd: &str, args: &[&str]) -> bool {
    Command::new(cmd)
        .args(args)
        .creation_flags(0x0800_0000)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Programi/klasoru/ayar sayfasini kabuk uzerinden acar.
pub fn shell_open(target: &str, params: &str) {
    let op = wide("open");
    let t = wide(target);
    let p = wide(params);
    unsafe {
        ShellExecuteW(null_mut(), op.as_ptr(), t.as_ptr(), p.as_ptr(), null(), 1);
    }
}

/// Dosyayi Gezgin'de secili olarak gosterir.
pub fn open_location(path: &str) {
    if !path.is_empty() {
        shell_open("explorer.exe", &format!("/select,\"{path}\""));
    }
}

pub fn set_priority(pid: u32, class: u32) -> bool {
    unsafe {
        let h = OpenProcess(0x0200 | 0x1000, 0, pid);
        if h.is_null() {
            return false;
        }
        let ok = SetPriorityClass(h, class) != 0;
        CloseHandle(h);
        ok
    }
}

// ---------------------------------------------------------------- guc plani

const SUB_PROC: &str = "54533251-82be-4824-96c1-47b60b740d00";
const P_MIN: &str = "893dee8e-2bef-41e0-89c6-b55d0929964c";
const P_MAX: &str = "bc5038f7-23e0-4960-96da-33abaf5935ec";
const P_EPP: &str = "36687f9e-e3a5-4dbf-b1dc-15eb381c6863";
const P_BOOST: &str = "be337238-0d82-4146-a960-4f3749d470c7";
const P_COOL: &str = "94d3a615-a899-4ac5-ae2b-e4d8f634367f";
const P_PARK: &str = "0cc5b647-c1df-4637-891a-dec35c318583";
const BALANCED: &str = "381b4222-f694-41f0-9685-ff5bb260df2e";
/// Programin kendi guc plani (kullanicinin planlarina dokunulmaz).
pub const FB_PLAN: &str = "fa9bec15-5a11-4d0e-9e55-0fa9bec15001";

fn power_value(setting: &str) -> Option<(u32, u32)> {
    let out = run_hidden("powercfg", &["/qh", "SCHEME_CURRENT", SUB_PROC, setting])?;
    let vals: Vec<u32> = out
        .lines()
        .filter_map(|l| {
            let v = l.rsplit(':').next()?.trim();
            let hex = v.strip_prefix("0x")?;
            u32::from_str_radix(hex, 16).ok()
        })
        .collect();
    if vals.len() < 2 {
        return None;
    }
    Some((vals[vals.len() - 2], vals[vals.len() - 1]))
}

#[derive(Clone, Default)]
pub struct Plan {
    pub guid: String,
    pub name: String,
    pub active: bool,
}

#[derive(Clone, Default)]
pub struct PowerState {
    pub plan: String,
    pub plan_guid: String,
    /// (priz, pil)
    pub min: Option<(u32, u32)>,
    pub max: Option<(u32, u32)>,
    pub epp: Option<(u32, u32)>,
    pub boost: Option<(u32, u32)>,
    pub plans: Vec<Plan>,
}

fn plans() -> Vec<Plan> {
    let Some(out) = run_hidden("powercfg", &["/list"]) else {
        return Vec::new();
    };
    out.lines()
        .filter_map(|l| {
            let a = l.find('(')?;
            let b = l.rfind(')')?;
            let guid = l[..a].split_whitespace().last()?.to_string();
            if guid.len() != 36 {
                return None;
            }
            Some(Plan {
                guid,
                name: l[a + 1..b].to_string(),
                active: l[b..].contains('*'),
            })
        })
        .collect()
}

pub fn power_read() -> PowerState {
    let plans = plans();
    let active = plans.iter().find(|p| p.active).cloned().unwrap_or_default();
    PowerState {
        plan: active.name,
        plan_guid: active.guid,
        min: power_value(P_MIN),
        max: power_value(P_MAX),
        epp: power_value(P_EPP),
        boost: power_value(P_BOOST),
        plans,
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PowerMode {
    Quiet,
    Smart,
    Performance,
    Max,
}

impl PowerMode {
    /// Etkin plan adi bu moda mi ait? Plan hangi dilde adlandirilmis olursa olsun tanir.
    pub fn names_plan(self, plan: &str) -> bool {
        let (tr, en) = match self {
            PowerMode::Quiet => ("Sessiz", "Quiet"),
            PowerMode::Smart => ("Akıllı Denge", "Smart Balance"),
            PowerMode::Performance => ("Serin Performans", "Cool Performance"),
            PowerMode::Max => ("Tam Güç", "Full Power"),
        };
        plan.ends_with(tr) || plan.ends_with(en)
    }

    pub fn title(self) -> &'static str {
        match self {
            PowerMode::Quiet => crate::lang::t("Sessiz", "Quiet"),
            PowerMode::Smart => crate::lang::t("Akıllı Denge", "Smart Balance"),
            PowerMode::Performance => crate::lang::t("Serin Performans", "Cool Performance"),
            PowerMode::Max => crate::lang::t("Tam Güç", "Full Power"),
        }
    }
    /// (alt %, ust %, EPP, turbo modu, en az etkin cekirdek %)
    fn values(self) -> (u32, u32, u32, u32, u32) {
        match self {
            PowerMode::Quiet => (5, 99, 70, 0, 25),
            PowerMode::Smart => (5, 100, 35, 3, 50),
            PowerMode::Performance => (5, 100, 10, 2, 100),
            PowerMode::Max => (100, 100, 0, 2, 100),
        }
    }
}

/// "HFix" planini (yoksa Dengeli'den kopyalayarak) olusturur, modu yazar ve etkinlestirir.
pub fn power_apply(mode: PowerMode) -> bool {
    if !plans().iter().any(|p| p.guid.eq_ignore_ascii_case(FB_PLAN)) {
        if !run_status("powercfg", &["/duplicatescheme", BALANCED, FB_PLAN])
            && !run_status("powercfg", &["/duplicatescheme", "SCHEME_CURRENT", FB_PLAN])
        {
            return false;
        }
    }
    let name = format!("HFix - {}", mode.title());
    run_status(
        "powercfg",
        &[
            "/changename",
            FB_PLAN,
            &name,
            crate::lang::t(
                "HFix Windows 11 Araçları tarafından yönetilen işlemci planı",
                "CPU plan managed by HFix Windows 11 Tools",
            ),
        ],
    );
    let (min, max, epp, boost, park) = mode.values();
    let dc_epp = (epp + 25).min(100);
    let set = |verb: &str, setting: &str, v: u32| {
        run_status(
            "powercfg",
            &[verb, FB_PLAN, SUB_PROC, setting, &v.to_string()],
        );
    };
    set("/setacvalueindex", P_MIN, min);
    set("/setacvalueindex", P_MAX, max);
    set("/setacvalueindex", P_EPP, epp);
    set("/setacvalueindex", P_BOOST, boost);
    set("/setacvalueindex", P_PARK, park);
    set("/setacvalueindex", P_COOL, 1);
    set("/setdcvalueindex", P_MIN, 5);
    set("/setdcvalueindex", P_MAX, max);
    set("/setdcvalueindex", P_EPP, dc_epp);
    set("/setdcvalueindex", P_BOOST, if boost == 0 { 0 } else { 3 });
    set("/setdcvalueindex", P_PARK, park.min(50));
    run_status("powercfg", &["/setactive", FB_PLAN])
}

pub fn power_activate(guid: &str) -> bool {
    run_status("powercfg", &["/setactive", guid])
}

// ------------------------------------------------- Windows ile baslatma

const TASK: &str = "HFixWin11";
/// Onceki adla (Fan Bekcisi) olusturulmus gorev.
const OLD_TASK: &str = "FanBekcisi";

/// Eski adli baslangic gorevi varsa siler ve yeni adla (bu exe icin) yeniden olusturur.
pub fn autostart_migrate() {
    if run_status("schtasks", &["/query", "/tn", OLD_TASK]) {
        run_status("schtasks", &["/delete", "/tn", OLD_TASK, "/f"]);
        autostart_set(true);
    }
}

pub fn autostart_enabled() -> bool {
    run_status("schtasks", &["/query", "/tn", TASK])
}

/// Oturum acilisinda yonetici olarak (UAC sormadan) kucultulmus baslatir.
pub fn autostart_set(on: bool) -> bool {
    if !on {
        return run_status("schtasks", &["/delete", "/tn", TASK, "/f"]);
    }
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    let user = format!(
        "{}\\{}",
        std::env::var("USERDOMAIN").unwrap_or_default(),
        std::env::var("USERNAME").unwrap_or_default()
    );
    // Pilde de calissin, 72 saat sinirina takilmasin diye gorev XML ile tanimlanir.
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo><Description>{desc}</Description></RegistrationInfo>
  <Triggers><LogonTrigger><Enabled>true</Enabled><UserId>{user}</UserId></LogonTrigger></Triggers>
  <Principals><Principal id="Author"><UserId>{user}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>HighestAvailable</RunLevel></Principal></Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Enabled>true</Enabled>
  </Settings>
  <Actions Context="Author"><Exec><Command>{exe}</Command><Arguments>--kucuk</Arguments></Exec></Actions>
</Task>"#,
        desc = crate::lang::t(
            "HFix Windows 11 Araclari'ni oturum acilisinda baslatir.",
            "Starts HFix Windows 11 Tools at sign-in."
        ),
        user = esc(&user),
        exe = esc(&exe.to_string_lossy())
    );
    let file = std::env::temp_dir().join("hfixwin11_gorev.xml");
    let mut bytes = vec![0xFF, 0xFE];
    for u in xml.encode_utf16() {
        bytes.extend_from_slice(&u.to_le_bytes());
    }
    if std::fs::write(&file, bytes).is_err() {
        return false;
    }
    let ok = run_status(
        "schtasks",
        &[
            "/create",
            "/tn",
            TASK,
            "/xml",
            &file.to_string_lossy(),
            "/f",
        ],
    );
    let _ = std::fs::remove_file(&file);
    ok
}

// ------------------------------------------------- surec basina ag trafigi

#[repr(C)]
#[derive(Clone, Copy)]
struct Row4Owner {
    state: u32,
    la: u32,
    lp: u32,
    ra: u32,
    rp: u32,
    pid: u32,
}
#[repr(C)]
pub struct Row4 {
    state: u32,
    la: u32,
    lp: u32,
    ra: u32,
    rp: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct Row6Owner {
    la: [u8; 16],
    lscope: u32,
    lp: u32,
    ra: [u8; 16],
    rscope: u32,
    rp: u32,
    state: u32,
    pid: u32,
}
#[repr(C)]
pub struct Row6 {
    state: u32,
    la: [u8; 16],
    lscope: u32,
    lp: u32,
    ra: [u8; 16],
    rscope: u32,
    rp: u32,
}
#[repr(C)]
#[derive(Default)]
struct DataRod {
    bytes_out: u64,
    segs_out: u64,
    bytes_in: u64,
    segs_in: u64,
    t_segs_out: u64,
    t_segs_in: u64,
    soft_errors: u32,
    soft_error_reason: u32,
    snd_una: u32,
    snd_nxt: u32,
    snd_max: u32,
    thru_acked: u64,
    rcv_nxt: u32,
    thru_received: u64,
}

#[derive(Hash, PartialEq, Eq, Clone)]
struct ConnKey {
    la: [u8; 16],
    lp: u32,
    ra: [u8; 16],
    rp: u32,
}

#[derive(Default, Clone, Copy)]
pub struct NetUse {
    pub down: u64,
    pub up: u64,
    pub conns: u32,
}

#[derive(Default)]
pub struct NetMon {
    last: HashMap<ConnKey, (u64, u64)>,
}

unsafe fn tcp_table(af: u32) -> Vec<u8> {
    let mut size = 0u32;
    GetExtendedTcpTable(null_mut(), &mut size, 0, af, 5, 0);
    let mut buf = vec![0u8; size as usize + 8192];
    size = buf.len() as u32;
    if GetExtendedTcpTable(buf.as_mut_ptr() as *mut c_void, &mut size, 0, af, 5, 0) != 0 {
        buf.clear();
    }
    buf
}

const ROD_SIZE: u32 = std::mem::size_of::<DataRod>() as u32;

impl NetMon {
    /// Son cagridan bu yana surec basina TCP bayt farklarini doner.
    /// Bayt sayaclari yonetici yetkisi ister; yoksa sadece baglanti sayisi dolar.
    pub fn sample(&mut self, admin: bool) -> HashMap<u32, NetUse> {
        let mut out: HashMap<u32, NetUse> = HashMap::new();
        let mut now: HashMap<ConnKey, (u64, u64)> = HashMap::new();
        let enable = [1u8];
        unsafe {
            // IPv4
            let buf = tcp_table(2);
            if buf.len() >= 4 {
                let n = std::ptr::read_unaligned(buf.as_ptr() as *const u32) as usize;
                let sz = std::mem::size_of::<Row4Owner>();
                for i in 0..n {
                    let off = 4 + i * sz;
                    if off + sz > buf.len() {
                        break;
                    }
                    let r: Row4Owner = std::ptr::read_unaligned(buf.as_ptr().add(off) as *const _);
                    if r.state != 5 || (r.ra & 0xff) == 127 {
                        continue;
                    }
                    let e = out.entry(r.pid).or_default();
                    e.conns += 1;
                    if !admin {
                        continue;
                    }
                    let mut key = ConnKey {
                        la: [0; 16],
                        lp: r.lp,
                        ra: [0; 16],
                        rp: r.rp,
                    };
                    key.la[..4].copy_from_slice(&r.la.to_ne_bytes());
                    key.ra[..4].copy_from_slice(&r.ra.to_ne_bytes());
                    let row = Row4 {
                        state: r.state,
                        la: r.la,
                        lp: r.lp,
                        ra: r.ra,
                        rp: r.rp,
                    };
                    let prev = self.last.get(&key).copied();
                    if prev.is_none() {
                        SetPerTcpConnectionEStats(&row, 1, enable.as_ptr(), 0, 1, 0);
                    }
                    let mut rod = DataRod::default();
                    let rc = GetPerTcpConnectionEStats(
                        &row,
                        1,
                        null_mut(),
                        0,
                        0,
                        null_mut(),
                        0,
                        0,
                        &mut rod as *mut _ as *mut u8,
                        0,
                        ROD_SIZE,
                    );
                    if rc == 0 {
                        if let Some((pi, po)) = prev {
                            e.down += rod.bytes_in.saturating_sub(pi);
                            e.up += rod.bytes_out.saturating_sub(po);
                        }
                        now.insert(key, (rod.bytes_in, rod.bytes_out));
                    }
                }
            }
            // IPv6
            let buf = tcp_table(23);
            if buf.len() >= 4 {
                let n = std::ptr::read_unaligned(buf.as_ptr() as *const u32) as usize;
                let sz = std::mem::size_of::<Row6Owner>();
                let mut loopback = [0u8; 16];
                loopback[15] = 1;
                for i in 0..n {
                    let off = 4 + i * sz;
                    if off + sz > buf.len() {
                        break;
                    }
                    let r: Row6Owner = std::ptr::read_unaligned(buf.as_ptr().add(off) as *const _);
                    if r.state != 5 || r.ra == loopback {
                        continue;
                    }
                    let e = out.entry(r.pid).or_default();
                    e.conns += 1;
                    if !admin {
                        continue;
                    }
                    let key = ConnKey {
                        la: r.la,
                        lp: r.lp | 0x8000_0000,
                        ra: r.ra,
                        rp: r.rp,
                    };
                    let row = Row6 {
                        state: r.state,
                        la: r.la,
                        lscope: r.lscope,
                        lp: r.lp,
                        ra: r.ra,
                        rscope: r.rscope,
                        rp: r.rp,
                    };
                    let prev = self.last.get(&key).copied();
                    if prev.is_none() {
                        SetPerTcp6ConnectionEStats(&row, 1, enable.as_ptr(), 0, 1, 0);
                    }
                    let mut rod = DataRod::default();
                    let rc = GetPerTcp6ConnectionEStats(
                        &row,
                        1,
                        null_mut(),
                        0,
                        0,
                        null_mut(),
                        0,
                        0,
                        &mut rod as *mut _ as *mut u8,
                        0,
                        ROD_SIZE,
                    );
                    if rc == 0 {
                        if let Some((pi, po)) = prev {
                            e.down += rod.bytes_in.saturating_sub(pi);
                            e.up += rod.bytes_out.saturating_sub(po);
                        }
                        now.insert(key, (rod.bytes_in, rod.bytes_out));
                    }
                }
            }
        }
        self.last = now;
        out
    }
}

/// FB_DEBUG ortam degiskeni bir dosya yolu ise asama kayitlarini oraya ekler.
#[allow(dead_code)]
pub fn stage(msg: &str) {
    if let Ok(p) = std::env::var("FB_DEBUG") {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
            let _ = writeln!(f, "{msg}");
        }
    }
}
#[link(name = "kernel32")]
extern "system" {
    fn CreateMutexW(attrs: *const c_void, owner: i32, name: *const u16) -> Handle;
    fn GetLastError() -> u32;
}

/// Program zaten aciksa false doner (ikinci kopya acilmaz).
pub fn single_instance() -> bool {
    let name = wide("Local\\HFixWin11TekKopya");
    unsafe {
        let h = CreateMutexW(null(), 0, name.as_ptr());
        !(h.is_null() || GetLastError() == 183)
    }
}

// ------------------------------------------------- yardimcilar (guncelleme ve kurulum)

#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryW(name: *const u16) -> Handle;
    fn GetProcAddress(h: Handle, name: *const u8) -> Handle;
    fn WaitForSingleObject(h: Handle, ms: u32) -> u32;
}

/// Bir sistem DLL'indeki islevin adresi. `name` sifir ile bitmelidir (b"Ad\0").
pub fn dll_fn(dll: &str, name: &[u8]) -> Option<Handle> {
    debug_assert!(name.ends_with(&[0]));
    unsafe {
        let h = LoadLibraryW(wide(dll).as_ptr());
        if h.is_null() {
            return None;
        }
        let f = GetProcAddress(h, name.as_ptr());
        (!f.is_null()).then_some(f)
    }
}

/// Isletim sisteminin guvenli rastgele sayi uretecinden `buf`i doldurur.
pub fn random(buf: &mut [u8]) -> bool {
    unsafe {
        let Some(f) = dll_fn("bcrypt.dll", b"BCryptGenRandom\0") else {
            return false;
        };
        let f: unsafe extern "system" fn(Handle, *mut u8, u32, u32) -> i32 = std::mem::transmute(f);
        // 2: BCRYPT_USE_SYSTEM_PREFERRED_RNG
        f(null_mut(), buf.as_mut_ptr(), buf.len() as u32, 2) == 0
    }
}

/// `pid` numarali surec kapanana kadar (en cok `ms` milisaniye) bekler.
pub fn wait_pid(pid: u32, ms: u32) {
    unsafe {
        let h = OpenProcess(0x0010_0000, 0, pid); // SYNCHRONIZE
        if !h.is_null() {
            WaitForSingleObject(h, ms);
            CloseHandle(h);
        }
    }
}