#![windows_subsystem = "windows"]
//! HFix Windows 11 Araçları — süreç, ağ, RAM, güç, başlangıç, disk, hızlandırma ve risk taraması.

mod apps;
mod cfg;
mod clean;
mod icon;
mod install;
mod lang;
mod mon;
mod reg;
mod risk;
mod tray;
mod tweak;
mod ui4;
mod ui3;
mod sig;
mod startup;
mod svc;
mod tasks;
mod ui;
mod ui2;
mod ui5;
mod update;
mod win;

use eframe::egui::{self, Color32, RichText};
use egui_extras::{Column, TableBuilder};
use mon::{Cmd, Collector, Snapshot, PERIOD};
use sig::Risk;
use std::collections::{HashMap, VecDeque};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
pub struct Pal {
    pub red: Color32,
    pub orange: Color32,
    pub green: Color32,
    pub blue: Color32,
}

pub fn pal(ui: &egui::Ui) -> Pal {
    if ui.visuals().dark_mode {
        Pal {
            red: Color32::from_rgb(242, 104, 92),
            orange: Color32::from_rgb(240, 165, 70),
            green: Color32::from_rgb(95, 205, 125),
            blue: Color32::from_rgb(105, 172, 245),
        }
    } else {
        Pal {
            red: Color32::from_rgb(192, 28, 28),
            orange: Color32::from_rgb(170, 90, 0),
            green: Color32::from_rgb(18, 122, 52),
            blue: Color32::from_rgb(20, 88, 190),
        }
    }
}

pub fn risk_color(p: Pal, r: Risk) -> Option<Color32> {
    match r {
        Risk::Suspicious => Some(p.red),
        Risk::Unsigned => Some(p.orange),
        _ => None,
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let has = |flag: &str| args.iter().any(|a| a == flag);

    // Surum yayimlarken kullanilan imza komutlari (pencere acmaz, cikis koduyla sonuc verir).
    match (args.get(1).map(String::as_str), args.len()) {
        (Some("--anahtar-uret"), 3) => std::process::exit(update::cmd_keygen(&args[2])),
        (Some("--imzala"), 5) => std::process::exit(update::cmd_sign(&args[2], &args[3], &args[4])),
        (Some("--imza-dogrula"), 4) => std::process::exit(update::cmd_verify(&args[2], &args[3])),
        _ => {}
    }

    // Test modu: olcum alip dosyaya yazar, pencere acmaz, hicbir seyi degistirmez.
    if let Some(i) = args.iter().position(|a| a == "--dump") {
        let path = args.get(i + 1).cloned().unwrap_or("dump.txt".into());
        dump(&path);
        return;
    }

    // Test goruntusu alinirken tek kopya denetimi ve tepsi simgesi atlanir.
    let testing = has("--goruntu");
    // Tasinabilir: kendini kurmaz, kisayol olusturmaz, guncelleme denetlemez.
    let portable = testing || has("--tasinabilir");
    // Guncellemeden sonra: onceki surumun kapanmasini bekle.
    if let Some(pid) = args.iter().find_map(|a| a.strip_prefix("--after=")?.parse::<u32>().ok()) {
        win::wait_pid(pid, 15_000);
    }
    if has("--kaldir") {
        // Baslangic gorevini silebilmek icin yonetici olarak.
        if win::is_admin() || !win::relaunch_as_admin(&args[1..]) {
            install::uninstall(has("--evet"));
        }
        return;
    }
    // Indirildigi yerden calistirildiysa: kendini kur, kisayollari olustur, kurulu kopyayi baslat.
    if !portable && install::self_install(&args) {
        return;
    }
    let config = cfg::load();
    lang::set(&config.lang);
    // Program zaten aciksa (tepside olsa bile) penceresini one getir.
    if !testing && tray::signal_existing() {
        return;
    }
    // Onceki oturumda indirilmis guncelleme varsa pencere acilmadan kur.
    if !portable && update::apply_at_start(&args) {
        return;
    }
    if !win::is_admin() && !has("--normal") && win::relaunch_as_admin(&args[1..]) {
        return;
    }
    if !testing && !win::single_instance() {
        return;
    }

    let admin = win::is_admin();
    if !portable {
        install::upkeep();
        if admin {
            std::thread::spawn(win::autostart_migrate);
        }
    }
    let minimized = args.iter().any(|a| a == "--kucuk");
    let start_tab = args
        .iter()
        .position(|a| a == "--tab")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse::<usize>().ok())
        .and_then(|i| Tab::ALL.get(i).copied())
        .unwrap_or(Tab::Procs);
    // Test: --goruntu <dosya.bmp> pencerenin goruntusunu kaydedip kapanir.
    let shot = args
        .iter()
        .position(|a| a == "--goruntu")
        .and_then(|i| args.get(i + 1))
        .cloned();
    let shot_clean = args.iter().any(|a| a == "--temizle");
    let shared = Arc::new(Mutex::new(Snapshot::default()));
    let (tx, rx) = mpsc::channel::<Cmd>();

    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(tray::window_title())
            .with_icon(egui::IconData {
                rgba: icon::rgba(64),
                width: 64,
                height: 64,
            })
            .with_inner_size([1240.0, 780.0])
            .with_min_inner_size([900.0, 520.0]),
        ..Default::default()
    };
    let sh = shared.clone();
    let _ = eframe::run_native(
        "HFixWin11",
        opts,
        Box::new(move |cc| {
            cc.egui_ctx
                .style_mut(|s| s.interaction.selectable_labels = false);
            let ctx = cc.egui_ctx.clone();
            let (auto, rules) = (config.auto, config.rules.clone());
            std::thread::spawn(move || worker(sh, rx, ctx, admin, auto, rules));
            apply_theme(&cc.egui_ctx, config.theme);
            if !testing {
                tray::start(tx.clone());
            }
            if !portable {
                let ctx = cc.egui_ctx.clone();
                update::start(config.auto_update, move || ctx.request_repaint());
            }
            let mut app = App::new(shared, tx, config, minimized);
            app.tab = start_tab;
            app.shot = shot.clone();
            app.shot_clean = shot_clean;
            Ok(Box::new(app))
        }),
    );
}

fn worker(
    shared: Arc<Mutex<Snapshot>>,
    rx: mpsc::Receiver<Cmd>,
    ctx: egui::Context,
    admin: bool,
    auto: mon::AutoCfg,
    rules: Vec<String>,
) {
    let mut col = Collector::new(admin, auto, &rules);
    let mut next = Instant::now();
    loop {
        let wait = next.saturating_duration_since(Instant::now());
        match rx.recv_timeout(wait) {
            Ok(cmd) => {
                col.handle(cmd);
                if let Ok(mut s) = shared.lock() {
                    s.log = col.log();
                    let (ram, seq) = col.ram();
                    s.ram = ram;
                    s.ram_seq = seq;
                }
                ctx.request_repaint();
                // Sonucu hemen goster.
                next = next.min(Instant::now() + Duration::from_millis(300));
                continue;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
        next = Instant::now() + PERIOD;
        let snap = col.collect();
        if let Ok(mut s) = shared.lock() {
            *s = snap;
        }
        ctx.request_repaint();
    }
}

fn dump(path: &str) {
    let mut col = Collector::new(win::is_admin(), mon::AutoCfg::default(), &[]);
    col.collect();
    std::thread::sleep(PERIOD);
    col.collect();
    std::thread::sleep(Duration::from_millis(6000));
    let s = col.collect();
    let mut out = format!(
        "admin={} cpu={:.1}% freq={}MHz ram={}/{} net[{}] in={} out={} gpu=[{}]\nplan={} [{}] min={:?} max={:?} epp={:?} boost={:?}\n",
        s.admin,
        s.cpu,
        s.freq,
        fb(s.mem_used as f64),
        fb(s.mem_total as f64),
        s.net_name,
        rate(s.net_down),
        rate(s.net_up),
        s.gpu,
        s.power.plan,
        s.power.plan_guid,
        s.power.min,
        s.power.max,
        s.power.epp,
        s.power.boost
    );
    for p in &s.power.plans {
        out += &format!("  plan {} {} {}\n", p.guid, p.name, p.active);
    }
    out += &format!(
        "surec {} imzasiz {} supheli {}\n--- imza\n",
        s.procs.len(),
        s.unsigned,
        s.suspicious
    );
    let mut seen = std::collections::HashSet::new();
    let (mut ok, mut unk) = (0, 0);
    for p in &s.procs {
        match p.risk {
            Risk::Ok => ok += 1,
            Risk::Unknown => unk += 1,
            _ => {
                if seen.insert(p.exe.clone()) {
                    out += &format!("{:?} {} | {} | {}\n", p.risk, p.name, p.trust, p.exe);
                }
            }
        }
    }
    out += &format!("guvenilir {ok} bilinmeyen {unk}\n");
    for p in s.procs.iter().filter(|p| p.risk == Risk::Ok).take(8) {
        out += &format!("  ok {} | {}\n", p.name, p.trust);
    }
    out += "--- baslangic\n";
    for it in startup::scan() {
        out += &format!(
            "{} [{}] {} {} | {:?} {} | {}\n",
            if it.enabled { "+" } else { "-" },
            it.source,
            it.name,
            it.state,
            it.risk,
            it.publisher,
            it.exe
        );
    }
    let progs = apps::list();
    out += &format!("--- programlar: {}\n", progs.len());
    for p in progs.iter().take(5) {
        out += &format!("{} | {} | {} | {}\n", p.name, p.publisher, p.version, fb(p.size as f64));
    }
    out += "--- disk\n";
    for (n, t, f) in clean::drives() {
        out += &format!("{n} toplam {} bos {}\n", fb(t as f64), fb(f as f64));
    }
    for mut c in clean::categories() {
        clean::scan(&mut c);
        out += &format!("{:<12} {:>10} {:>7} dosya  {}\n", c.id, fb(c.bytes as f64), c.files, c.name);
    }
    out += "--- servisler\n";
    for s in svc::scan() {
        out += &format!(
            "{} {:<9} {:<20} | {} | {:?} {} | {} | {}\n",
            if s.running { "*" } else { " " },
            s.group.title().chars().take(9).collect::<String>(),
            s.start_text(),
            s.display,
            s.risk,
            s.publisher,
            s.file,
            s.desc.chars().take(40).collect::<String>()
        );
    }
    out += "--- gorevler\n";
    let tl = match std::env::var("FB_TASKS") {
        Ok(d) => tasks::scan_dir(std::path::Path::new(&d)),
        Err(_) => tasks::scan(),
    };
    for t in tl {
        out += &format!(
            "{} {:?} {} | {} | {} | {} | {}\n",
            if t.enabled { "+" } else { "-" },
            t.risk,
            t.path,
            t.trust,
            t.triggers,
            t.author,
            t.command
        );
    }
    let _ = std::fs::write(path, out);
}

pub fn fb(b: f64) -> String {
    if b >= 1_073_741_824.0 {
        format!("{:.1} GB", b / 1_073_741_824.0)
    } else if b >= 1_048_576.0 {
        format!("{:.1} MB", b / 1_048_576.0)
    } else if b >= 1024.0 {
        format!("{:.0} KB", b / 1024.0)
    } else {
        format!("{:.0} B", b)
    }
}

pub fn rate(b: f64) -> String {
    if b < 1.0 {
        String::new()
    } else {
        format!("{}/s", fb(b))
    }
}

#[derive(PartialEq, Clone, Copy)]
pub enum Tab {
    Procs,
    Net,
    Mem,
    Startup,
    Services,
    Risk,
    Disk,
    Programs,
    Power,
    Log,
    Boost,
    Settings,
    About,
}

impl Tab {
    /// `--tab N` test secenegi bu siraya gore sayar.
    const ALL: [Tab; 13] = [
        Tab::Procs,
        Tab::Net,
        Tab::Mem,
        Tab::Startup,
        Tab::Services,
        Tab::Risk,
        Tab::Disk,
        Tab::Programs,
        Tab::Power,
        Tab::Log,
        Tab::Boost,
        Tab::Settings,
        Tab::About,
    ];

    /// Yan menu: (bolum basligi TR, EN, sekmeler).
    const NAV: [(&'static str, &'static str, &'static [Tab]); 5] = [
        ("İZLEME", "MONITOR", &[Tab::Procs, Tab::Net, Tab::Mem]),
        ("HIZLANDIRMA", "SPEED UP", &[Tab::Boost, Tab::Power, Tab::Startup, Tab::Services]),
        ("TEMİZLİK", "CLEAN UP", &[Tab::Disk, Tab::Programs]),
        ("GÜVENLİK", "SECURITY", &[Tab::Risk, Tab::Log]),
        ("PROGRAM", "PROGRAM", &[Tab::Settings, Tab::About]),
    ];

    fn title(self) -> &'static str {
        match self {
            Tab::Procs => lang::t("Süreçler", "Processes"),
            Tab::Net => lang::t("Ağ", "Network"),
            Tab::Mem => lang::t("Bellek", "Memory"),
            Tab::Startup => lang::t("Başlangıç ve Görevler", "Startup and Tasks"),
            Tab::Services => lang::t("Servisler", "Services"),
            Tab::Risk => lang::t("Risk Analizi", "Risk Analysis"),
            Tab::Disk => lang::t("Disk Temizliği", "Disk Cleanup"),
            Tab::Programs => lang::t("Programlar", "Programs"),
            Tab::Power => lang::t("Güç Modu", "Power Mode"),
            Tab::Log => lang::t("İşlem Kaydı", "Activity Log"),
            Tab::Boost => lang::t("Windows Hızlandırma", "Windows Speed-Up"),
            Tab::Settings => lang::t("Ayarlar", "Settings"),
            Tab::About => lang::t("Hakkında", "About"),
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Tab::Procs => lang::t(
                "Çalışan programlar; işlemci, bellek, disk ve ağ kullanımı",
                "Running programs; CPU, memory, disk and network use",
            ),
            Tab::Net => lang::t("İnterneti kullanan programlar", "Programs using the internet"),
            Tab::Mem => lang::t("RAM kullanımı ve temizliği", "RAM use and cleaning"),
            Tab::Boost => lang::t(
                "Windows ayarlarıyla hızlandırma ve gereksiz uygulamalar",
                "Speeding up with Windows settings, and unneeded apps",
            ),
            Tab::Power => lang::t(
                "İşlemci güç modu, otomatik kısma ve program ayarları",
                "CPU power mode, automatic throttling and program options",
            ),
            Tab::Startup => lang::t(
                "Windows açılırken başlayan programlar ve zamanlanmış görevler",
                "Programs that start with Windows, and scheduled tasks",
            ),
            Tab::Services => lang::t("Arka plan servisleri", "Background services"),
            Tab::Disk => lang::t("Gereksiz dosyaları silerek yer açma", "Freeing space by deleting unneeded files"),
            Tab::Programs => lang::t(
                "Yüklü programları kaldırma ve Windows araçları",
                "Uninstalling programs, and Windows tools",
            ),
            Tab::Risk => lang::t(
                "Şüpheli program, dosya ve ayarların taranması",
                "Scanning for suspicious programs, files and settings",
            ),
            Tab::Log => lang::t("Programın yaptığı işlemlerin kaydı", "A record of what the program did"),
            Tab::Settings => lang::t("Tema, dil, başlangıç ve güncelleme tercihleri", "Theme, language, startup and update preferences"),
            Tab::About => lang::t("Sürüm bilgisi ve güncelleme", "Version information and updates"),
        }
    }
}

#[derive(PartialEq, Clone, Copy)]
enum SortCol {
    Name,
    Count,
    Trust,
    Cpu,
    Avg,
    Mem,
    Disk,
    Down,
    Up,
    Conns,
}

#[derive(Clone)]
struct Row {
    key: String,
    name: String,
    exe: String,
    pids: Vec<u32>,
    cpu: f32,
    avg: f32,
    mem: u64,
    disk: f64,
    down: f64,
    up: f64,
    conns: u32,
    throttled: bool,
    risk: Risk,
    trust: String,
}

struct App {
    shared: Arc<Mutex<Snapshot>>,
    tx: mpsc::Sender<Cmd>,
    cfg: cfg::Cfg,
    tab: Tab,
    sort: SortCol,
    desc: bool,
    filter: String,
    only_risky: bool,
    sel: Option<String>,
    confirm_kill: Option<String>,
    first: bool,
    minimized: bool,
    ui: ui::State,
    ui2: ui2::State,
    ui3: ui3::State,
    ui4: ui4::State,
    ui5: ui5::State,
    frames: u32,
    shot: Option<String>,
    shot_clean: bool,
    started: Instant,
    shot_asked: bool,
}

impl App {
    fn new(
        shared: Arc<Mutex<Snapshot>>,
        tx: mpsc::Sender<Cmd>,
        cfg: cfg::Cfg,
        minimized: bool,
    ) -> Self {
        Self {
            shared,
            tx,
            cfg,
            tab: Tab::Procs,
            sort: SortCol::Cpu,
            desc: true,
            filter: String::new(),
            only_risky: false,
            sel: None,
            confirm_kill: None,
            first: true,
            minimized,
            ui: ui::State::new(),
            ui2: ui2::State::default(),
            ui3: ui3::State::default(),
            ui4: ui4::State::default(),
            ui5: ui5::State::default(),
            frames: 0,
            shot: None,
            shot_clean: false,
            started: Instant::now(),
            shot_asked: false,
        }
    }

    fn send(&self, c: Cmd) {
        let _ = self.tx.send(c);
    }

    fn rows(&self, s: &Snapshot, net_only: bool) -> Vec<Row> {
        let f = self.filter.to_lowercase();
        let mut rows: Vec<Row> = Vec::new();
        let mut idx: HashMap<String, usize> = HashMap::new();
        for p in &s.procs {
            if !f.is_empty()
                && !p.name.to_lowercase().contains(&f)
                && !p.trust.to_lowercase().contains(&f)
                && p.pid.to_string() != f
            {
                continue;
            }
            if net_only && p.conns == 0 {
                continue;
            }
            if self.only_risky && p.risk < Risk::Unsigned {
                continue;
            }
            if self.cfg.group {
                if let Some(&i) = idx.get(&p.name) {
                    let r = &mut rows[i];
                    r.pids.push(p.pid);
                    r.cpu += p.cpu;
                    r.avg += p.avg;
                    r.mem += p.mem;
                    r.disk += p.disk;
                    r.down += p.down;
                    r.up += p.up;
                    r.conns += p.conns;
                    r.throttled |= p.throttled;
                    if p.risk > r.risk {
                        r.risk = p.risk;
                        r.trust = p.trust.clone();
                        r.exe = p.exe.clone();
                    }
                    continue;
                }
                idx.insert(p.name.clone(), rows.len());
            }
            rows.push(Row {
                key: if self.cfg.group {
                    p.name.clone()
                } else {
                    format!("{}#{}", p.name, p.pid)
                },
                name: p.name.clone(),
                exe: p.exe.clone(),
                pids: vec![p.pid],
                cpu: p.cpu,
                avg: p.avg,
                mem: p.mem,
                disk: p.disk,
                down: p.down,
                up: p.up,
                conns: p.conns,
                throttled: p.throttled,
                risk: p.risk,
                trust: p.trust.clone(),
            });
        }
        let d = self.desc;
        rows.sort_by(|a, b| {
            let o = match self.sort {
                SortCol::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                SortCol::Count => {
                    if self.cfg.group {
                        a.pids.len().cmp(&b.pids.len())
                    } else {
                        a.pids[0].cmp(&b.pids[0])
                    }
                }
                SortCol::Trust => a
                    .risk
                    .cmp(&b.risk)
                    .then_with(|| b.trust.to_lowercase().cmp(&a.trust.to_lowercase())),
                SortCol::Cpu => a.cpu.total_cmp(&b.cpu),
                SortCol::Avg => a.avg.total_cmp(&b.avg),
                SortCol::Mem => a.mem.cmp(&b.mem),
                SortCol::Disk => a.disk.total_cmp(&b.disk),
                SortCol::Down => a.down.total_cmp(&b.down),
                SortCol::Up => a.up.total_cmp(&b.up),
                SortCol::Conns => a.conns.cmp(&b.conns),
            };
            if d {
                o.reverse()
            } else {
                o
            }
        });
        rows
    }

    fn top_bar(&mut self, ui: &mut egui::Ui, s: &Snapshot) {
        let c = pal(ui);
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let mem_pct = s.mem_used as f32 / s.mem_total.max(1) as f32;
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(tr!("İşlemci  %{:.0}   {} MHz", "CPU  {:.0}%   {} MHz", s.cpu, s.freq)).strong(),
                );
                spark(ui, &s.cpu_hist, heat(c, s.cpu, 40.0, 75.0));
            });
            ui.separator();
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(tr!(
                        "Bellek  {} / {}   (kullanılabilir {})",
                        "Memory  {} / {}   (available {})",
                        fb(s.mem_used as f64),
                        fb(s.mem_total as f64),
                        fb(s.mem_avail as f64)
                    ))
                    .strong(),
                );
                spark(ui, &s.mem_hist, heat(c, mem_pct * 100.0, 70.0, 88.0));
            })
            .response
            .on_hover_text(tr!(
                "Disk belleği (pagefile): {} / {}",
                "Page file: {} / {}",
                fb(s.swap_used as f64),
                fb(s.swap_total as f64)
            ));
            ui.separator();
            ui.vertical(|ui| {
                ui.label(RichText::new(lang::t("Ağ", "Network")).strong());
                ui.label(tr!("İndirme  {}", "Download  {}", zero(rate(s.net_down))));
                ui.label(tr!("Yükleme  {}", "Upload  {}", zero(rate(s.net_up))));
            })
            .response
            .on_hover_text(tr!("Bağdaştırıcı: {}", "Adapter: {}", s.net_name));
            ui.separator();
            ui.vertical(|ui| {
                ui.label(RichText::new(lang::t("Ekran kartı", "Graphics card")).strong());
                ui.label(if s.gpu.is_empty() { "—" } else { &s.gpu });
            });
            ui.separator();
            ui.vertical(|ui| {
                ui.label(RichText::new(tr!("Güç: {}", "Power: {}", s.power.plan)).strong());
                match (s.power.min, s.power.max) {
                    (Some((mn, _)), Some((mx, _))) => {
                        if mn >= 90 {
                            ui.colored_label(
                                c.red,
                                tr!(
                                    "İşlemci alt sınırı %{} — fan bu yüzden hızlı",
                                    "CPU minimum is {}% — that is why the fan runs fast",
                                    mn
                                ),
                            );
                        } else {
                            ui.colored_label(
                                c.green,
                                tr!("İşlemci alt %{} · üst %{}", "CPU min {}% · max {}%", mn, mx),
                            );
                        }
                    }
                    _ => {
                        ui.label(lang::t("İşlemci sınırları okunamadı", "Could not read the CPU limits"));
                    }
                }
                if s.suspicious > 0 {
                    ui.colored_label(
                        c.red,
                        tr!("{} şüpheli program çalışıyor", "{} suspicious program(s) running", s.suspicious),
                    );
                } else if s.unsigned > 0 {
                    ui.colored_label(
                        c.orange,
                        tr!("{} imzasız program çalışıyor", "{} unsigned program(s) running", s.unsigned),
                    );
                }
                if !s.admin {
                    ui.colored_label(c.orange, lang::t("Sınırlı mod (yönetici değil)", "Limited mode (not administrator)"));
                }
            });
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(self.tab.title()).size(19.0).strong());
            ui.label(RichText::new(self.tab.hint()).weak());
        });
        if matches!(self.tab, Tab::Procs | Tab::Net) {
            ui.horizontal(|ui| {
                ui.label(lang::t("Ara:", "Search:"));
                ui.add(egui::TextEdit::singleline(&mut self.filter).desired_width(160.0));
                if ui
                    .checkbox(&mut self.cfg.group, lang::t("Aynı adlıları birleştir", "Merge same names"))
                    .changed()
                {
                    cfg::save(&self.cfg);
                }
                ui.checkbox(
                    &mut self.only_risky,
                    lang::t("Yalnızca imzasız / şüpheli", "Only unsigned / suspicious"),
                );
                ui.separator();
                ui.colored_label(c.red, lang::t("■ şüpheli", "■ suspicious"));
                ui.colored_label(c.orange, lang::t("■ imzasız", "■ unsigned"));
                ui.colored_label(c.blue, lang::t("■ sanal makine", "■ virtual machine"));
            });
        }
        ui.add_space(2.0);
    }

    /// Sol taraftaki gruplandirilmis menu.
    fn nav(&mut self, ui: &mut egui::Ui, s: &Snapshot) {
        let c = pal(ui);
        ui.spacing_mut().button_padding = egui::vec2(10.0, 7.0);
        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Min), |ui| {
            ui.add_space(10.0);
            ui.label(RichText::new("HFix").size(22.0).strong());
            ui.label(RichText::new(lang::t("Windows 11 Araçları", "Windows 11 Tools")).weak());
            let update_ready = update::info().phase == update::Phase::Ready;
            for (section_tr, section_en, tabs) in Tab::NAV {
                ui.add_space(10.0);
                ui.label(RichText::new(lang::t(section_tr, section_en)).small().weak());
                ui.add_space(2.0);
                for t in tabs {
                    let mut text = RichText::new(t.title()).size(15.0);
                    // Dikkat isteyen sayfalari renkle belirt.
                    if *t == Tab::Risk && s.suspicious > 0 {
                        text = text.color(c.red);
                    }
                    if *t == Tab::About && update_ready {
                        text = text.color(c.green);
                    }
                    if ui.selectable_label(self.tab == *t, text).on_hover_text(t.hint()).clicked() {
                        self.tab = *t;
                    }
                }
            }
            ui.add_space(16.0);
            ui.separator();
            ui.add_space(6.0);
            if ui
                .add(primary(lang::t("RAM'i temizle", "Clean RAM")))
                .on_hover_text(lang::t(
                    "Belleği temizler ve Bellek sayfasında önce / sonra durumunu gösterir",
                    "Cleans the memory and shows the before / after state on the Memory page",
                ))
                .clicked()
            {
                self.tab = Tab::Mem;
                self.ram_clean(s.ram_seq);
            }
            ui.add_space(6.0);
            if ui
                .button(lang::t("Programdan çık", "Exit program"))
                .on_hover_text(lang::t(
                    "Programı tümüyle kapatır. Pencerenin çarpısı yalnızca sistem tepsisine gizler.",
                    "Closes the program completely. The window's X button only hides it to the system tray.",
                ))
                .clicked()
            {
                tray::quit();
            }
        });
    }

    fn bottom_bar(&mut self, ui: &mut egui::Ui, s: &Snapshot, rows: &[Row]) {
        let c = pal(ui);
        ui.add_space(3.0);
        let sel = self
            .sel
            .as_ref()
            .and_then(|k| rows.iter().find(|r| &r.key == k))
            .cloned();
        ui.horizontal_wrapped(|ui| match &sel {
            None => {
                ui.label(lang::t("Bir satıra tıklayarak süreç seçin.", "Click a row to select a process."));
            }
            Some(r) => {
                let crit = mon::is_critical(&r.name);
                ui.label(
                    RichText::new(tr!("{} ({} süreç)", "{} ({} process(es))", r.name, r.pids.len())).strong(),
                );
                if crit {
                    ui.colored_label(c.orange, lang::t("sistem süreci — korunuyor", "system process — protected"));
                }
                if ui
                    .add_enabled(!crit, egui::Button::new(lang::t("Kıs", "Throttle")))
                    .on_hover_text(lang::t(
                        "Önceliği en düşüğe alır ve Windows verimlilik modunu açar: süreç çalışır ama işlemciyi ısıtmaz",
                        "Sets the lowest priority and turns on Windows efficiency mode: the process keeps running but does not heat up the CPU",
                    ))
                    .clicked()
                {
                    self.send(Cmd::Throttle(r.pids.clone(), true));
                }
                if r.throttled && ui.button(lang::t("Normal hız", "Normal speed")).clicked() {
                    self.send(Cmd::Throttle(r.pids.clone(), false));
                }
                if ui
                    .add_enabled(!crit, egui::Button::new(lang::t("Hızlandır", "Boost")))
                    .on_hover_text(lang::t(
                        "Önceliği yükseğe alır: bu program diğerlerinden önce işlemci alır",
                        "Sets a high priority: this program gets CPU time before the others",
                    ))
                    .clicked()
                {
                    self.send(Cmd::Priority(r.pids.clone(), true));
                }
                let low = r.name.to_lowercase();
                let mut always = self.cfg.rules.contains(&low);
                if ui
                    .add_enabled(
                        !crit,
                        egui::Checkbox::new(&mut always, lang::t("Her zaman kıs", "Always throttle")),
                    )
                    .on_hover_text(lang::t(
                        "Bu program her açıldığında otomatik kısılır (kalıcı kural)",
                        "This program is throttled automatically every time it starts (permanent rule)",
                    ))
                    .changed()
                {
                    if always {
                        self.cfg.rules.push(low);
                        self.send(Cmd::Throttle(r.pids.clone(), true));
                    } else {
                        self.cfg.rules.retain(|x| x != &low);
                    }
                    cfg::save(&self.cfg);
                    self.send(Cmd::Rules(self.cfg.rules.clone()));
                }
                if ui.button(lang::t("Belleğini boşalt", "Free its memory")).clicked() {
                    self.send(Cmd::Trim(r.pids.clone()));
                }
                if !r.exe.is_empty() && ui.button(lang::t("Konumu aç", "Open location")).clicked() {
                    win::open_location(&r.exe);
                }
                if self.confirm_kill.as_ref() == Some(&r.key) {
                    ui.colored_label(
                        c.red,
                        lang::t("Kaydedilmemiş veri kaybolur. Emin misiniz?", "Unsaved data will be lost. Are you sure?"),
                    );
                    if ui
                        .button(RichText::new(lang::t("Evet, sonlandır", "Yes, end it")).color(c.red))
                        .clicked()
                    {
                        self.send(Cmd::Kill(r.pids.clone()));
                        self.confirm_kill = None;
                    }
                    if ui.button(lang::t("Vazgeç", "Cancel")).clicked() {
                        self.confirm_kill = None;
                    }
                } else if ui
                    .add_enabled(!crit, egui::Button::new(lang::t("Sonlandır", "End")))
                    .on_hover_text(lang::t(
                        "Süreç bir Windows servisine aitse önce servis durdurulur",
                        "If the process belongs to a Windows service, the service is stopped first",
                    ))
                    .clicked()
                {
                    self.confirm_kill = Some(r.key.clone());
                }
            }
        });
        if let Some(r) = &sel {
            if !r.exe.is_empty() {
                let mut t = RichText::new(format!("{}   —   {}", r.exe, r.trust)).small();
                if let Some(col) = risk_color(c, r.risk) {
                    t = t.color(col);
                }
                ui.label(t);
            }
        }
        if let Some(l) = s.log.front() {
            ui.label(RichText::new(l).weak());
        }
        ui.add_space(3.0);
    }

    fn table(&mut self, ui: &mut egui::Ui, rows: &[Row], net: bool) {
        let c = pal(ui);
        let (mut sort, mut desc) = (self.sort, self.desc);
        let group = self.cfg.group;
        let sel = self.sel.clone();
        let mut clicked: Option<String> = None;

        let count = if group { lang::t("Adet", "Count") } else { "PID" };
        let cols: Vec<(&str, SortCol, f32)> = if net {
            vec![
                (lang::t("Süreç", "Process"), SortCol::Name, 230.0),
                (count, SortCol::Count, 60.0),
                (lang::t("İmza / yayıncı", "Signature / publisher"), SortCol::Trust, 220.0),
                (lang::t("İndirme", "Download"), SortCol::Down, 100.0),
                (lang::t("Yükleme", "Upload"), SortCol::Up, 100.0),
                (lang::t("Bağlantı", "Connections"), SortCol::Conns, 80.0),
                ("CPU %", SortCol::Cpu, 70.0),
                (lang::t("Bellek", "Memory"), SortCol::Mem, 90.0),
            ]
        } else {
            vec![
                (lang::t("Süreç", "Process"), SortCol::Name, 230.0),
                (count, SortCol::Count, 60.0),
                (lang::t("İmza / yayıncı", "Signature / publisher"), SortCol::Trust, 220.0),
                ("CPU %", SortCol::Cpu, 70.0),
                (lang::t("1 dk ort.", "1 min avg"), SortCol::Avg, 80.0),
                (lang::t("Bellek", "Memory"), SortCol::Mem, 90.0),
                ("Disk", SortCol::Disk, 90.0),
                (lang::t("İndirme", "Download"), SortCol::Down, 90.0),
                (lang::t("Yükleme", "Upload"), SortCol::Up, 90.0),
                (lang::t("Bağlantı", "Connections"), SortCol::Conns, 70.0),
            ]
        };

        let mut tb = TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .sense(egui::Sense::click())
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center));
        for (i, (_, _, w)) in cols.iter().enumerate() {
            tb = tb.column(if i + 1 == cols.len() {
                Column::remainder()
            } else {
                Column::initial(*w).at_least(40.0).clip(true)
            });
        }

        tb.header(24.0, |mut h| {
            for (label, col, _) in &cols {
                h.col(|ui| {
                    let t = if sort == *col {
                        format!("{label} {}", if desc { "⬇" } else { "⬆" })
                    } else {
                        label.to_string()
                    };
                    if ui
                        .add(egui::Button::new(RichText::new(t).strong()).frame(false))
                        .clicked()
                    {
                        if sort == *col {
                            desc = !desc;
                        } else {
                            sort = *col;
                            desc = *col != SortCol::Name;
                        }
                    }
                });
            }
        })
        .body(|body| {
            body.rows(21.0, rows.len(), |mut row| {
                let r = &rows[row.index()];
                row.set_selected(sel.as_ref() == Some(&r.key));
                let rc = risk_color(c, r.risk);
                for (_, col, _) in &cols {
                    row.col(|ui| match col {
                        SortCol::Name => {
                            let mut t = RichText::new(&r.name);
                            if let Some(col) = rc {
                                t = t.color(col).strong();
                            } else if mon::is_vm(&r.name) {
                                t = t.color(c.blue);
                            }
                            ui.label(t);
                            if r.throttled {
                                ui.colored_label(c.green, lang::t("kısıldı", "throttled"));
                            }
                        }
                        SortCol::Count => {
                            ui.label(if group {
                                r.pids.len().to_string()
                            } else {
                                r.pids[0].to_string()
                            });
                        }
                        SortCol::Trust => {
                            let mut t = RichText::new(&r.trust);
                            if let Some(col) = rc {
                                t = t.color(col);
                            }
                            ui.label(t).on_hover_text(&r.exe);
                        }
                        SortCol::Cpu => pct(ui, c, r.cpu),
                        SortCol::Avg => pct(ui, c, r.avg),
                        SortCol::Mem => {
                            ui.label(fb(r.mem as f64));
                        }
                        SortCol::Disk => {
                            ui.label(rate(r.disk));
                        }
                        SortCol::Down => {
                            ui.label(rate(r.down));
                        }
                        SortCol::Up => {
                            ui.label(rate(r.up));
                        }
                        SortCol::Conns => {
                            ui.label(if r.conns > 0 {
                                r.conns.to_string()
                            } else {
                                String::new()
                            });
                        }
                    });
                }
                if row.response().clicked() {
                    clicked = Some(r.key.clone());
                }
            });
        });

        self.sort = sort;
        self.desc = desc;
        if let Some(k) = clicked {
            self.confirm_kill = None;
            self.sel = if self.sel.as_ref() == Some(&k) {
                None
            } else {
                Some(k)
            };
        }
    }
}

pub fn apply_theme(ctx: &egui::Context, theme: cfg::Theme) {
    ctx.set_theme(match theme {
        cfg::Theme::System => egui::ThemePreference::System,
        cfg::Theme::Light => egui::ThemePreference::Light,
        cfg::Theme::Dark => egui::ThemePreference::Dark,
    });
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.first {
            self.first = false;
            apply_theme(ctx, self.cfg.theme);
        }
        // Windows acilisinda baslatildiysa dogrudan tepsiye gizlen.
        self.frames = self.frames.saturating_add(1);
        if self.minimized && self.frames == 3 {
            self.minimized = false;
            tray::hide_main();
        }
        // Carpi dugmesi programi kapatmaz, tepsiye gizler.
        if self.shot.is_none() && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            tray::hide_main();
        }
        let s = self.shared.lock().map(|s| s.clone()).unwrap_or_default();
        if let Some(path) = self.shot.clone() {
            ctx.request_repaint();
            let t = self.started.elapsed().as_secs();
            if self.shot_clean && t >= 5 {
                self.shot_clean = false;
                self.ram_clean(s.ram_seq);
            }
            if t >= std::env::var("FB_BEKLE").ok().and_then(|v| v.parse().ok()).unwrap_or(14) && !self.shot_asked {
                self.shot_asked = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
            }
            let img = ctx.input(|i| {
                i.raw.events.iter().find_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(img) = img {
                save_bmp(&path, &img);
                std::process::exit(0);
            }
        }
        let rows = match self.tab {
            Tab::Procs => self.rows(&s, false),
            Tab::Net => self.rows(&s, true),
            _ => Vec::new(),
        };

        egui::SidePanel::left("nav")
            .exact_width(204.0)
            .resizable(false)
            .show(ctx, |ui| self.nav(ui, &s));
        egui::TopBottomPanel::top("top").show(ctx, |ui| self.top_bar(ui, &s));
        if matches!(self.tab, Tab::Procs | Tab::Net) {
            egui::TopBottomPanel::bottom("bottom").show(ctx, |ui| self.bottom_bar(ui, &s, &rows));
        }
        egui::CentralPanel::default().show(ctx, |ui| match self.tab {
            Tab::Procs => self.table(ui, &rows, false),
            Tab::Net => {
                let c = pal(ui);
                if !s.admin {
                    ui.colored_label(
                        c.orange,
                        lang::t(
                            "Sınırlı modda yalnızca bağlantı sayısı görünür; bayt hızları için programı yönetici olarak açın.",
                            "In limited mode only the connection count is shown; run the program as administrator to see byte rates.",
                        ),
                    );
                }
                ui.label(
                    RichText::new(lang::t(
                        "Hızlar TCP trafiğidir (web, indirme, senkronizasyon). UDP/QUIC (bazı görüntülü görüşme ve oyunlar) burada görünmez; toplam hız üst çubuktadır.",
                        "Rates are TCP traffic (web, downloads, sync). UDP/QUIC (some video calls and games) is not shown here; the total rate is in the top bar.",
                    ))
                    .weak(),
                );
                self.table(ui, &rows, true);
            }
            Tab::Mem => self.mem_tab(ui, &s),
            Tab::Services => self.services_tab(ui, &s),
            Tab::Risk => self.risk_tab(ui, &s),
            Tab::Boost => self.boost_tab(ui, &s),
            Tab::Startup => self.startup_tab(ui),
            Tab::Disk => self.disk_tab(ui),
            Tab::Programs => self.programs_tab(ui),
            Tab::Power => self.power_tab(ui, &s),
            Tab::Settings => self.settings_tab(ui),
            Tab::About => self.about_tab(ui),
            Tab::Log => {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for l in &s.log {
                        ui.label(l);
                    }
                });
            }
        });
    }
}

fn save_bmp(path: &str, img: &egui::ColorImage) {
    let (w, h) = (img.size[0], img.size[1]);
    let row = (w * 3 + 3) & !3;
    let mut out = Vec::with_capacity(54 + row * h);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&((54 + row * h) as u32).to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&54u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&(h as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&24u16.to_le_bytes());
    out.extend_from_slice(&[0; 24]);
    for y in (0..h).rev() {
        for x in 0..w {
            let p = img.pixels[y * w + x];
            out.extend_from_slice(&[p.b(), p.g(), p.r()]);
        }
        out.resize(out.len() + row - w * 3, 0);
    }
    let _ = std::fs::write(path, out);
}

fn zero(s: String) -> String {
    if s.is_empty() {
        "0".into()
    } else {
        s
    }
}

/// Tiklaninca siralamayi degistiren tablo basligi.
pub fn sort_header(
    ui: &mut egui::Ui,
    label: &str,
    col: usize,
    first_desc: bool,
    sort: &mut (usize, bool),
) {
    let t = if sort.0 == col {
        format!("{label} {}", if sort.1 { "⬇" } else { "⬆" })
    } else {
        label.to_string()
    };
    if ui
        .add(egui::Button::new(RichText::new(t).strong()).frame(false))
        .on_hover_text(lang::t("Sıralamak için tıklayın", "Click to sort"))
        .clicked()
    {
        if sort.0 == col {
            sort.1 = !sort.1;
        } else {
            *sort = (col, first_desc);
        }
    }
}

/// Sekmelerde kullanilan cerceveli kart.
pub fn card(ui: &egui::Ui) -> egui::Frame {
    egui::Frame::group(ui.style())
        .fill(ui.visuals().faint_bg_color)
        .rounding(8.0)
        .inner_margin(12.0)
}

/// Parcalari (oran, renk) yan yana cizen yatay cubuk.
pub fn stack_bar(ui: &mut egui::Ui, w: f32, h: f32, parts: &[(f32, Color32)]) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 3.0, ui.visuals().extreme_bg_color);
    let mut x = rect.left();
    for (f, col) in parts {
        let ww = (rect.width() * f.clamp(0.0, 1.0)).min(rect.right() - x);
        if ww > 0.5 {
            p.rect_filled(
                egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(ww, h)),
                2.0,
                *col,
            );
        }
        x += ww;
    }
}

pub fn accent() -> Color32 {
    Color32::from_rgb(30, 118, 210)
}

/// Dolu renkli, belirgin ana eylem dugmesi.
pub fn primary(text: &str) -> egui::Button<'_> {
    egui::Button::new(RichText::new(text).strong().color(Color32::WHITE))
        .fill(accent())
        .min_size(egui::vec2(0.0, 32.0))
}

pub fn heat(c: Pal, v: f32, warn: f32, bad: f32) -> Color32 {
    if v >= bad {
        c.red
    } else if v >= warn {
        c.orange
    } else {
        c.green
    }
}

pub fn pct(ui: &mut egui::Ui, c: Pal, v: f32) {
    if v < 0.05 {
        ui.label("");
    } else if v >= 5.0 {
        ui.colored_label(heat(c, v, 5.0, 15.0), format!("{v:.1}"));
    } else {
        ui.label(format!("{v:.1}"));
    }
}

/// 0-100 araligindaki gecmisi kucuk bir cizgi grafik olarak cizer.
fn spark(ui: &mut egui::Ui, hist: &VecDeque<f32>, color: Color32) {
    chart(ui, hist, color, 190.0, 30.0);
}

pub fn chart(ui: &mut egui::Ui, hist: &VecDeque<f32>, color: Color32, w: f32, h: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 3.0, ui.visuals().extreme_bg_color);
    if hist.len() < 2 {
        return;
    }
    let n = 90.0_f32;
    let off = n - hist.len() as f32;
    let pts: Vec<egui::Pos2> = hist
        .iter()
        .enumerate()
        .map(|(i, v)| {
            egui::pos2(
                rect.left() + (off + i as f32) / (n - 1.0) * rect.width(),
                rect.bottom() - 1.0 - (v.clamp(0.0, 100.0) / 100.0) * (rect.height() - 2.0),
            )
        })
        .collect();
    p.add(egui::Shape::line(pts, egui::Stroke::new(1.5_f32, color)));
}
