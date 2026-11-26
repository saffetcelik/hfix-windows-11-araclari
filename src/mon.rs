//! Arka plan toplayici: surecler, CPU, RAM, ag, guc plani ve otomatik korumalar.

use crate::sig::{self, Risk};
use crate::lang::t;
use crate::startup;
use crate::tr;
use crate::win::{self, NetMon, PowerMode, PowerState};
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};
use sysinfo::{Networks, ProcessRefreshKind, System, UpdateKind};

pub const PERIOD: Duration = Duration::from_millis(2000);
const HIST: usize = 90;

/// Sonlandirilmasina / kisilmasina izin verilmeyen sistem surecleri.
const CRITICAL: &[&str] = &[
    "system",
    "system idle process",
    "registry",
    "memory compression",
    "secure system",
    "smss",
    "csrss",
    "wininit",
    "winlogon",
    "services",
    "lsass",
    "lsaiso",
    "fontdrvhost",
    "dwm",
    "svchost",
    "audiodg",
    "explorer",
    "msmpeng",
    "fanbekcisi",
    "hfixwin11",
];

pub fn is_critical(name: &str) -> bool {
    let n = name.to_lowercase();
    CRITICAL.contains(&n.as_str())
}

/// Sanal makine surecleri: RAM'i bosaltilmaz, otomatik kisilmaz.
pub fn is_vm(name: &str) -> bool {
    let n = name.to_lowercase();
    n.starts_with("vmware") || n.starts_with("vmplayer") || n == "vmnat" || n == "mkssandbox"
}

#[derive(Clone)]
pub struct Proc {
    pub pid: u32,
    pub name: String,
    pub exe: String,
    pub cpu: f32,
    pub avg: f32,
    pub mem: u64,
    pub disk: f64,
    pub down: f64,
    pub up: f64,
    pub conns: u32,
    pub throttled: bool,
    pub risk: Risk,
    pub trust: String,
}

#[derive(Clone, Copy, PartialEq)]
pub struct AutoCfg {
    pub ram_on: bool,
    pub ram_pct: u32,
    pub thr_on: bool,
    pub thr_pct: u32,
}

impl Default for AutoCfg {
    fn default() -> Self {
        Self {
            ram_on: false,
            ram_pct: 85,
            thr_on: false,
            thr_pct: 25,
        }
    }
}

#[derive(Clone, Default)]
pub struct Snapshot {
    pub procs: Vec<Proc>,
    pub cpu: f32,
    pub cores: Vec<f32>,
    pub freq: u64,
    pub mem_total: u64,
    pub mem_used: u64,
    pub mem_avail: u64,
    pub swap_total: u64,
    pub swap_used: u64,
    pub net_name: String,
    pub net_down: f64,
    pub net_up: f64,
    pub gpu: String,
    pub power: PowerState,
    pub cpu_hist: VecDeque<f32>,
    pub mem_hist: VecDeque<f32>,
    pub log: VecDeque<String>,
    pub admin: bool,
    pub unsigned: usize,
    pub suspicious: usize,
    pub mem_free: u64,
    pub ram: VecDeque<RamReport>,
    pub ram_seq: u64,
}

/// Bir RAM temizliginin oncesi / sonrasi.
#[derive(Clone, Default)]
pub struct RamReport {
    pub time: String,
    pub auto: bool,
    pub total: u64,
    pub avail_before: u64,
    pub avail_after: u64,
    pub free_before: u64,
    pub free_after: u64,
    pub trimmed: usize,
    pub standby: bool,
    /// (ad, once, sonra) — en cok bellek birakan programlar.
    pub top: Vec<(String, u64, u64)>,
}

pub enum Cmd {
    CleanRam,
    Trim(Vec<u32>),
    Kill(Vec<u32>),
    Throttle(Vec<u32>, bool),
    Priority(Vec<u32>, bool),
    Power(PowerMode),
    PowerPlan(String, String),
    FlushDns,
    Auto(AutoCfg),
    Rules(Vec<String>),
    Note(String),
    RefreshPower,
}

pub struct Collector {
    sys: System,
    nets: Networks,
    netmon: NetMon,
    ncpu: f32,
    ema: HashMap<u32, f32>,
    hot: HashMap<u32, u32>,
    throttled: HashSet<u32>,
    rules: HashSet<String>,
    rule_done: HashSet<u32>,
    cycle: u64,
    last: Instant,
    last_auto_clean: Option<Instant>,
    gpu: String,
    power: PowerState,
    power_dirty: bool,
    cpu_hist: VecDeque<f32>,
    mem_hist: VecDeque<f32>,
    log: VecDeque<String>,
    ram: VecDeque<RamReport>,
    ram_seq: u64,
    auto: AutoCfg,
    admin: bool,
    own: u32,
}

fn refresh_kind() -> ProcessRefreshKind {
    ProcessRefreshKind::new()
        .with_cpu()
        .with_memory()
        .with_disk_usage()
        .with_exe(UpdateKind::OnlyIfNotSet)
}

impl Collector {
    pub fn new(admin: bool, auto: AutoCfg, rules: &[String]) -> Self {
        if admin {
            win::enable_privileges();
        }
        let mut sys = System::new();
        sys.refresh_cpu();
        sys.refresh_memory();
        let ncpu = sys.cpus().len().max(1) as f32;
        let mut c = Self {
            sys,
            nets: Networks::new_with_refreshed_list(),
            netmon: NetMon::default(),
            ncpu,
            ema: HashMap::new(),
            hot: HashMap::new(),
            throttled: HashSet::new(),
            rules: rules.iter().map(|r| r.to_lowercase()).collect(),
            rule_done: HashSet::new(),
            cycle: 0,
            last: Instant::now(),
            last_auto_clean: None,
            gpu: String::new(),
            power: PowerState::default(),
            power_dirty: true,
            cpu_hist: VecDeque::new(),
            mem_hist: VecDeque::new(),
            log: VecDeque::new(),
            ram: VecDeque::new(),
            ram_seq: 0,
            auto,
            admin,
            own: win::own_pid(),
        };
        c.note(if admin {
            t(
                "Başladı (yönetici modu: tüm özellikler açık).",
                "Started (administrator mode: all features available).",
            )
            .into()
        } else {
            t(
                "Başladı (sınırlı mod: RAM temizliği, ağ bayt sayaçları ve sistem ayarları için yönetici gerekir).",
                "Started (limited mode: RAM cleaning, network byte counters and system settings need administrator).",
            )
            .into()
        });
        c
    }

    fn note(&mut self, msg: String) {
        self.log.push_front(format!("{}  {}", win::clock(), msg));
        self.log.truncate(300);
    }

    pub fn log(&self) -> VecDeque<String> {
        self.log.clone()
    }

    fn name_of(&self, pid: u32) -> String {
        self.sys
            .process(sysinfo::Pid::from_u32(pid))
            .map(|p| clean_name(p.name()))
            .unwrap_or_default()
    }

    fn exe_of(&self, pid: u32) -> Option<String> {
        self.sys
            .process(sysinfo::Pid::from_u32(pid))
            .and_then(|p| p.exe())
            .map(|p| p.to_string_lossy().to_lowercase())
    }

    pub fn ram(&self) -> (VecDeque<RamReport>, u64) {
        (self.ram.clone(), self.ram_seq)
    }

    fn mem_by_name(&mut self) -> HashMap<String, u64> {
        self.sys
            .refresh_processes_specifics(ProcessRefreshKind::new().with_memory());
        let mut m: HashMap<String, u64> = HashMap::new();
        for p in self.sys.processes().values() {
            *m.entry(clean_name(p.name())).or_default() += p.memory();
        }
        m
    }

    fn clean_ram(&mut self, why: &str, auto: bool) {
        self.sys.refresh_memory();
        let before = self.sys.available_memory();
        let free_before = before.saturating_sub(win::mem_cache());
        let by_before = self.mem_by_name();
        let pids: Vec<(u32, String)> = self
            .sys
            .processes()
            .iter()
            .map(|(pid, p)| (pid.as_u32(), clean_name(p.name())))
            .collect();
        let mut n = 0;
        for (pid, name) in pids {
            if pid == self.own || pid < 8 || is_vm(&name) {
                continue;
            }
            if win::trim_process(pid) {
                n += 1;
            }
        }
        let standby = self.admin && win::purge_standby();
        std::thread::sleep(Duration::from_millis(500));
        self.sys.refresh_memory();
        let after = self.sys.available_memory();
        let by_after = self.mem_by_name();
        let mut top: Vec<(String, u64, u64)> = by_before
            .into_iter()
            .map(|(k, b)| {
                let a = by_after.get(&k).copied().unwrap_or(0);
                (k, b, a)
            })
            .filter(|x| x.1 > x.2)
            .collect();
        top.sort_by(|a, b| (b.1 - b.2).cmp(&(a.1 - a.2)));
        top.truncate(10);
        self.ram.push_front(RamReport {
            time: win::clock(),
            auto,
            total: self.sys.total_memory(),
            avail_before: before,
            avail_after: after,
            free_before,
            free_after: after.saturating_sub(win::mem_cache()),
            trimmed: n,
            standby,
            top,
        });
        self.ram.truncate(12);
        self.ram_seq += 1;
        let freed = after.saturating_sub(before) as f64 / 1_048_576.0;
        self.note(tr!(
            "{}: {} sürecin belleği boşaltıldı{}; {:.0} MB kazanıldı (boş: {:.1} GB).",
            "{}: memory of {} process(es) freed{}; {:.0} MB gained (available: {:.1} GB).",
            why,
            n,
            if standby {
                t(", bekleme önbelleği temizlendi", ", standby cache cleared")
            } else {
                ""
            },
            freed,
            after as f64 / 1_073_741_824.0
        ));
    }

    fn kill(&mut self, pids: Vec<u32>) {
        let name = pids.first().map(|p| self.name_of(*p)).unwrap_or_default();
        if name.is_empty() {
            return;
        }
        if is_critical(&name) {
            self.note(tr!(
                "{} bir sistem süreci, sonlandırılmadı.",
                "{} is a system process; it was not ended.",
                name
            ));
            return;
        }
        // Surec bir Windows servisine aitse once servisi durdur; yoksa
        // servis yoneticisi onu hemen yeniden baslatir.
        let exes: HashSet<String> = pids.iter().filter_map(|p| self.exe_of(*p)).collect();
        let mut stopped = Vec::new();
        if self.admin && !exes.is_empty() {
            for s in startup::services(true) {
                if exes.contains(&s.exe.to_lowercase()) && win::run_status("sc", &["stop", &s.id])
                {
                    stopped.push(s.name);
                }
            }
        }
        if !stopped.is_empty() {
            std::thread::sleep(Duration::from_millis(1500));
        }
        for p in &pids {
            win::kill(*p);
        }
        std::thread::sleep(Duration::from_millis(1200));
        self.sys.refresh_processes_specifics(refresh_kind());
        let gone = pids
            .iter()
            .filter(|p| self.sys.process(sysinfo::Pid::from_u32(**p)).is_none())
            .count();
        let back = self
            .sys
            .processes()
            .iter()
            .filter(|(pid, p)| {
                !pids.contains(&pid.as_u32()) && clean_name(p.name()).eq_ignore_ascii_case(&name)
            })
            .count();
        let mut msg = tr!(
            "{}: {}/{} süreç sonlandırıldı",
            "{}: {}/{} process(es) ended",
            name,
            gone,
            pids.len()
        );
        if !stopped.is_empty() {
            msg += &tr!(" (servis durduruldu: {})", " (service stopped: {})", stopped.join(", "));
        }
        if gone < pids.len() {
            msg += if self.admin {
                t("; kalanlar korumalı süreç", "; the rest are protected processes")
            } else {
                t(
                    "; kalanlar için yönetici modu gerekir",
                    "; the rest need administrator mode",
                )
            };
        }
        if back > 0 {
            msg += &tr!(
                ". {} yeni kopya yeniden başladı — bir servis ya da başlangıç görevi onu geri açıyor; kalıcı kapatmak için Başlangıç sekmesinden devre dışı bırakın",
                ". {} new instance(s) started again — a service or startup task is reopening it; to stop it for good, disable it on the Startup page",
                back
            );
        } else if !stopped.is_empty() {
            msg += t(
                ". Servis hâlâ otomatik; yeniden başlatmada tekrar açılır (Başlangıç sekmesinden kapatılabilir)",
                ". The service is still automatic; it will start again after a restart (it can be turned off on the Startup page)",
            );
        }
        msg.push('.');
        self.note(msg);
    }

    pub fn handle(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::CleanRam => self.clean_ram(t("RAM temizliği", "RAM cleaning"), false),
            Cmd::Trim(pids) => {
                let name = pids.first().map(|p| self.name_of(*p)).unwrap_or_default();
                let n = pids.iter().filter(|p| win::trim_process(**p)).count();
                self.note(tr!(
                    "{}: {} sürecin belleği boşaltıldı.",
                    "{}: memory of {} process(es) freed.",
                    name,
                    n
                ));
            }
            Cmd::Kill(pids) => self.kill(pids),
            Cmd::Throttle(pids, on) => {
                let name = pids.first().map(|p| self.name_of(*p)).unwrap_or_default();
                let mut n = 0;
                for p in &pids {
                    if win::throttle(*p, on) {
                        n += 1;
                        if on {
                            self.throttled.insert(*p);
                        } else {
                            self.throttled.remove(p);
                        }
                    }
                }
                self.note(tr!(
                    "{}: {}/{} süreç {}.",
                    "{}: {}/{} process(es) {}.",
                    name,
                    n,
                    pids.len(),
                    if on {
                        t("kısıldı (verimlilik modu)", "throttled (efficiency mode)")
                    } else {
                        t("normal hıza döndürüldü", "returned to normal speed")
                    }
                ));
            }
            Cmd::Priority(pids, high) => {
                let name = pids.first().map(|p| self.name_of(*p)).unwrap_or_default();
                let class = if high { 0x80 } else { 0x20 };
                let n = pids.iter().filter(|p| win::set_priority(**p, class)).count();
                for p in &pids {
                    self.throttled.remove(p);
                }
                self.note(tr!(
                    "{}: {}/{} sürecin önceliği {} yapıldı.",
                    "{}: priority of {}/{} process(es) set to {}.",
                    name,
                    n,
                    pids.len(),
                    if high { t("yüksek", "high") } else { "normal" }
                ));
            }
            Cmd::Power(mode) => {
                let ok = win::power_apply(mode);
                self.power_dirty = true;
                self.note(if ok {
                    tr!("Güç modu uygulandı: {}.", "Power mode applied: {}.", mode.title())
                } else {
                    tr!(
                        "Güç modu uygulanamadı: {} (yönetici modu gerekir).",
                        "Could not apply power mode: {} (administrator mode needed).",
                        mode.title()
                    )
                });
            }
            Cmd::PowerPlan(guid, name) => {
                let ok = win::power_activate(&guid);
                self.power_dirty = true;
                self.note(if ok {
                    tr!("Güç planı etkinleştirildi: {}.", "Power plan activated: {}.", name)
                } else {
                    tr!("Güç planı etkinleştirilemedi: {}.", "Could not activate power plan: {}.", name)
                });
            }
            Cmd::FlushDns => {
                win::run_hidden("ipconfig", &["/flushdns"]);
                self.note(t("DNS önbelleği temizlendi.", "DNS cache cleared.").into());
            }
            Cmd::Auto(cfg) => self.auto = cfg,
            Cmd::Rules(r) => {
                self.rules = r.iter().map(|r| r.to_lowercase()).collect();
                self.rule_done.clear();
            }
            Cmd::Note(s) => self.note(s),
            Cmd::RefreshPower => self.power_dirty = true,
        }
    }

    pub fn collect(&mut self) -> Snapshot {
        let dt = self.last.elapsed().as_secs_f64().max(0.2);
        self.last = Instant::now();
        self.cycle += 1;

        self.sys.refresh_cpu();
        self.sys.refresh_memory();
        self.sys.refresh_processes_specifics(refresh_kind());
        self.nets.refresh();
        if self.cycle % 30 == 0 {
            self.nets.refresh_list();
        }
        let net = self.netmon.sample(self.admin);

        // En yogun ag bagdastiricisi (sanal bagdastiricilarla cift sayimi onler).
        let (mut net_name, mut net_down, mut net_up) = (String::new(), 0.0, 0.0);
        let mut best = -1.0;
        for (name, d) in &self.nets {
            let (rx, tx) = (d.received() as f64 / dt, d.transmitted() as f64 / dt);
            if rx + tx > best {
                best = rx + tx;
                net_name = name.clone();
                net_down = rx;
                net_up = tx;
            }
        }

        let mut procs = Vec::with_capacity(self.sys.processes().len());
        let mut alive = HashSet::new();
        let (mut unsigned, mut suspicious) = (HashSet::new(), HashSet::new());
        for (pid, p) in self.sys.processes() {
            let pid = pid.as_u32();
            alive.insert(pid);
            let cpu = p.cpu_usage() / self.ncpu;
            let e = self.ema.entry(pid).or_insert(cpu);
            *e = *e * 0.93 + cpu * 0.07;
            let du = p.disk_usage();
            let nu = net.get(&pid).copied().unwrap_or_default();
            let name = clean_name(p.name());
            let exe = p
                .exe()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_default();
            let (risk, trust) = if pid == self.own {
                (Risk::Ok, t("Bu program", "This program").to_string())
            } else {
                let s = sig::lookup(&exe);
                sig::assess(&name, &exe, s.as_ref())
            };
            match risk {
                Risk::Unsigned => {
                    unsigned.insert(exe.to_lowercase());
                }
                Risk::Suspicious => {
                    suspicious.insert(exe.to_lowercase());
                }
                _ => {}
            }
            procs.push(Proc {
                pid,
                name,
                exe,
                cpu,
                avg: *e,
                mem: p.memory(),
                disk: (du.read_bytes + du.written_bytes) as f64 / dt,
                down: nu.down as f64 / dt,
                up: nu.up as f64 / dt,
                conns: nu.conns,
                throttled: self.throttled.contains(&pid),
                risk,
                trust,
            });
        }
        self.ema.retain(|k, _| alive.contains(k));
        self.hot.retain(|k, _| alive.contains(k));
        self.throttled.retain(|k| alive.contains(k));
        self.rule_done.retain(|k| alive.contains(k));

        // Kalici kurallar: "her zaman kis" listesindeki surecler.
        if !self.rules.is_empty() {
            for p in procs.iter_mut() {
                if !self.rule_done.contains(&p.pid) && self.rules.contains(&p.name.to_lowercase())
                {
                    self.rule_done.insert(p.pid);
                    if win::throttle(p.pid, true) {
                        self.throttled.insert(p.pid);
                        p.throttled = true;
                    }
                }
            }
        }

        // Otomatik kisma: esigi 30 sn boyunca asan arka plan surecleri.
        if self.auto.thr_on {
            let thr = self.auto.thr_pct as f32;
            let mut todo = Vec::new();
            for p in &procs {
                if p.cpu > thr
                    && !p.throttled
                    && p.pid != self.own
                    && !is_critical(&p.name)
                    && !is_vm(&p.name)
                {
                    let c = self.hot.entry(p.pid).or_insert(0);
                    *c += 1;
                    if *c >= 15 {
                        todo.push((p.pid, p.name.clone(), p.cpu));
                    }
                } else {
                    self.hot.remove(&p.pid);
                }
            }
            for (pid, name, cpu) in todo {
                self.hot.remove(&pid);
                if win::throttle(pid, true) {
                    self.throttled.insert(pid);
                    self.note(tr!(
                        "Otomatik: {} (PID {}) 30 sn boyunca %{:.0} CPU kullandı, kısıldı.",
                        "Automatic: {} (PID {}) used {:.0}% CPU for 30 s and was throttled.",
                        name,
                        pid,
                        cpu
                    ));
                }
            }
        }

        let total = self.sys.total_memory();
        let avail = self.sys.available_memory();
        let used = total.saturating_sub(avail);
        let used_pct = used as f32 * 100.0 / total.max(1) as f32;

        // Otomatik RAM temizligi (5 dakikada en fazla bir kez).
        if self.auto.ram_on
            && used_pct >= self.auto.ram_pct as f32
            && self
                .last_auto_clean
                .map_or(true, |t| t.elapsed() > Duration::from_secs(300))
        {
            self.last_auto_clean = Some(Instant::now());
            self.clean_ram(t("Otomatik RAM temizliği", "Automatic RAM cleaning"), true);
        }

        if self.cycle % 4 == 1 {
            self.gpu = read_gpu();
        }
        if self.power_dirty || self.cycle % 15 == 1 {
            self.power = win::power_read();
            self.power_dirty = false;
        }

        let cpu = self.sys.global_cpu_info().cpu_usage();
        push(&mut self.cpu_hist, cpu);
        push(&mut self.mem_hist, used_pct);

        Snapshot {
            procs,
            cpu,
            cores: self.sys.cpus().iter().map(|c| c.cpu_usage()).collect(),
            freq: self.sys.cpus().first().map(|c| c.frequency()).unwrap_or(0),
            mem_total: total,
            mem_used: used,
            mem_avail: avail,
            swap_total: self.sys.total_swap(),
            swap_used: self.sys.used_swap(),
            net_name,
            net_down,
            net_up,
            gpu: self.gpu.clone(),
            power: self.power.clone(),
            cpu_hist: self.cpu_hist.clone(),
            mem_hist: self.mem_hist.clone(),
            log: self.log.clone(),
            admin: self.admin,
            unsigned: unsigned.len(),
            suspicious: suspicious.len(),
            mem_free: avail.saturating_sub(win::mem_cache()),
            ram: self.ram.clone(),
            ram_seq: self.ram_seq,
        }
    }
}

fn push(h: &mut VecDeque<f32>, v: f32) {
    h.push_back(v);
    while h.len() > HIST {
        h.pop_front();
    }
}

fn clean_name(n: &str) -> String {
    n.strip_suffix(".exe")
        .or_else(|| n.strip_suffix(".EXE"))
        .unwrap_or(n)
        .to_string()
}

fn read_gpu() -> String {
    let Some(out) = win::run_hidden(
        "nvidia-smi",
        &[
            "--query-gpu=temperature.gpu,utilization.gpu,power.draw",
            "--format=csv,noheader,nounits",
        ],
    ) else {
        return String::new();
    };
    let v: Vec<&str> = out.trim().split(',').map(|s| s.trim()).collect();
    if v.len() < 3 {
        return String::new();
    }
    tr!("{} °C · %{} · {} W", "{} °C · {}% · {} W", v[0], v[1], v[2])
}
