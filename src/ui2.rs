//! Bellek, Servisler ve Zamanlanmis Gorevler gorunumleri.

use crate::mon::{self, Cmd, RamReport, Snapshot};
use crate::sig::Risk;
use crate::svc::{self, Group, Service};
use crate::tasks::{self, Task};
use crate::tr;
use crate::{card, cfg, chart, fb, heat, pal, primary, risk_color, sort_header, stack_bar, win, App, Pal};
use eframe::egui::{self, Color32, RichText};
use egui_extras::{Column, TableBuilder};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Default)]
struct SvcState {
    list: Vec<Service>,
    busy: bool,
    loaded: bool,
    status: String,
    backup: usize,
}

#[derive(Default)]
struct TaskState {
    list: Vec<Task>,
    busy: bool,
    loaded: bool,
}

pub struct State {
    mem_pending: Option<(u64, Instant)>,
    svc: Arc<Mutex<SvcState>>,
    svc_sel: HashSet<String>,
    svc_filter: String,
    svc_core: bool,
    svc_risky: bool,
    svc_confirm: Option<bool>,
    tasks: Arc<Mutex<TaskState>>,
    task_filter: String,
    task_ms: bool,
    task_risky: bool,
    task_sort: (usize, bool),
}

impl Default for State {
    fn default() -> Self {
        Self {
            mem_pending: None,
            svc: Arc::default(),
            svc_sel: HashSet::new(),
            svc_filter: String::new(),
            svc_core: false,
            svc_risky: false,
            svc_confirm: None,
            tasks: Arc::default(),
            task_filter: String::new(),
            task_ms: false,
            task_risky: false,
            task_sort: (2, true),
        }
    }
}

fn used_color(c: Pal, pct: f32) -> Color32 {
    heat(c, pct, 70.0, 88.0)
}

fn legend(ui: &mut egui::Ui, col: Color32, text: String) {
    ui.colored_label(col, "■");
    ui.label(text);
    ui.add_space(8.0);
}

impl App {
    // --------------------------------------------------------------- Bellek

    pub(crate) fn ram_clean(&mut self, seq: u64) {
        if self.ui2.mem_pending.is_none() {
            self.ui2.mem_pending = Some((seq, Instant::now()));
            self.send(Cmd::CleanRam);
        }
    }

    pub(crate) fn mem_tab(&mut self, ui: &mut egui::Ui, s: &Snapshot) {
        let c = pal(ui);
        if let Some((seq, t)) = self.ui2.mem_pending {
            if s.ram_seq != seq || t.elapsed().as_secs() > 30 {
                self.ui2.mem_pending = None;
            }
        }
        let busy = self.ui2.mem_pending.is_some();
        let cache_col = c.blue.gamma_multiply(0.75);
        let free_col = ui.visuals().widgets.noninteractive.bg_stroke.color;
        let total = s.mem_total.max(1) as f32;
        let standby = s.mem_avail.saturating_sub(s.mem_free);
        let used_pct = s.mem_used as f32 * 100.0 / total;

        egui::ScrollArea::vertical().show(ui, |ui| {
            let full = ui.available_width() - 28.0;
            ui.add_space(6.0);

            // ---- su anki durum
            card(ui).show(ui, |ui| {
                ui.set_width(full);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(tr!("Bellek kullanımı", "Memory usage")).strong());
                        ui.label(
                            RichText::new(tr!("%{:.0}", "{:.0}%", used_pct))
                                .size(34.0)
                                .strong()
                                .color(used_color(c, used_pct)),
                        );
                        ui.label(format!(
                            "{} / {}",
                            fb(s.mem_used as f64),
                            fb(s.mem_total as f64)
                        ));
                    });
                    ui.add_space(16.0);
                    ui.vertical(|ui| {
                        let w = (full - 470.0).clamp(260.0, 620.0);
                        stack_bar(
                            ui,
                            w,
                            26.0,
                            &[
                                (s.mem_used as f32 / total, used_color(c, used_pct)),
                                (standby as f32 / total, cache_col),
                                (s.mem_free as f32 / total, free_col),
                            ],
                        );
                        ui.horizontal(|ui| {
                            legend(ui, used_color(c, used_pct), tr!("Kullanılan {}", "Used {}", fb(s.mem_used as f64)));
                            legend(ui, cache_col, tr!("Önbellek (bekleme) {}", "Standby cache {}", fb(standby as f64)));
                            legend(ui, free_col, tr!("Boş {}", "Free {}", fb(s.mem_free as f64)));
                        });
                        ui.label(
                            RichText::new(tr!(
                                "Disk belleği (pagefile): {} / {}  ·  Önbellek, Windows'un gerektiğinde hemen geri verdiği bellektir.",
                                "Page file: {} / {}  ·  The standby cache is memory that Windows gives back immediately when needed.",
                                fb(s.swap_used as f64),
                                fb(s.swap_total as f64)
                            ))
                            .weak(),
                        );
                    });
                    ui.add_space(16.0);
                    ui.vertical(|ui| {
                        if busy {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(RichText::new(tr!("Temizleniyor…", "Cleaning…")).strong());
                            });
                        } else if ui
                            .add(primary(tr!("   RAM'i şimdi temizle   ", "   Clean RAM now   ")).min_size(egui::vec2(190.0, 40.0)))
                            .clicked()
                        {
                            self.ram_clean(s.ram_seq);
                        }
                        ui.label(RichText::new(tr!("VMware süreçlerine dokunulmaz.", "VMware processes are not touched.")).weak());
                    });
                });
            });

            ui.add_space(8.0);
            // ---- son temizlik
            card(ui).show(ui, |ui| {
                ui.set_width(full);
                match s.ram.front() {
                    None => {
                        ui.label(RichText::new(tr!("Son temizlik", "Last cleanup")).strong());
                        ui.label(tr!(
                            "Henüz temizlik yapılmadı. \"RAM'i şimdi temizle\" düğmesine bastığınızda önceki ve sonraki durum burada karşılaştırılır.",
                            "No cleanup yet. When you press \"Clean RAM now\", the before and after states are compared here."
                        ));
                    }
                    Some(r) => ram_report(ui, c, r, full, cache_col, free_col),
                }
            });

            ui.add_space(8.0);
            ui.horizontal_top(|ui| {
                let half = (full - 36.0) / 2.0;
                card(ui).show(ui, |ui| { ui.vertical(|ui| {
                    ui.set_width(half);
                    ui.label(RichText::new(tr!("Son 3 dakika — bellek kullanımı (%)", "Last 3 minutes — memory usage (%)")).strong());
                    chart(ui, &s.mem_hist, used_color(c, used_pct), half, 96.0);
                    ui.add_space(6.0);
                    ui.label(RichText::new(tr!("Otomatik temizlik", "Automatic cleanup")).strong());
                    let mut a = self.cfg.auto;
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut a.ram_on, tr!("Kullanım şu oranı geçince temizle:", "Clean when usage exceeds:"));
                        ui.add(egui::Slider::new(&mut a.ram_pct, 50..=95).suffix(" %"));
                    });
                    if a != self.cfg.auto {
                        self.cfg.auto = a;
                        cfg::save(&self.cfg);
                        self.send(Cmd::Auto(a));
                    }
                    if s.ram.len() > 1 {
                        ui.add_space(6.0);
                        ui.label(RichText::new(tr!("Temizlik geçmişi", "Cleanup history")).strong());
                        egui::Grid::new("ramhist").num_columns(4).spacing([18.0, 4.0]).show(ui, |ui| {
                            for r in s.ram.iter().take(8) {
                                let gain = r.avail_after.saturating_sub(r.avail_before);
                                ui.label(&r.time);
                                ui.colored_label(c.green, format!("+{}", fb(gain as f64)));
                                ui.label(tr!(
                                    "{} » {} boş",
                                    "{} » {} free",
                                    fb(r.avail_before as f64),
                                    fb(r.avail_after as f64)
                                ));
                                ui.label(RichText::new(if r.auto { tr!("otomatik", "automatic") } else { tr!("elle", "manual") }).weak());
                                ui.end_row();
                            }
                        });
                    }
                }); });
                card(ui).show(ui, |ui| { ui.vertical(|ui| {
                    ui.set_width(half);
                    ui.label(RichText::new(tr!("En çok bellek kullanan programlar", "Programs using the most memory")).strong());
                    let mut agg: HashMap<&str, (u64, Vec<u32>)> = HashMap::new();
                    for p in &s.procs {
                        let e = agg.entry(p.name.as_str()).or_default();
                        e.0 += p.mem;
                        e.1.push(p.pid);
                    }
                    let mut v: Vec<_> = agg.into_iter().collect();
                    v.sort_by(|a, b| b.1 .0.cmp(&a.1 .0));
                    let max = v.first().map_or(1, |x| x.1 .0).max(1) as f32;
                    egui::Grid::new("memtop").num_columns(4).spacing([10.0, 5.0]).show(ui, |ui| {
                        for (name, (mem, pids)) in v.into_iter().take(12) {
                            ui.label(name.chars().take(24).collect::<String>());
                            let col = if mon::is_vm(name) { c.blue } else { crate::accent() };
                            stack_bar(ui, (half - 330.0).max(80.0), 12.0, &[(mem as f32 / max, col)]);
                            ui.label(fb(mem as f64));
                            if mon::is_vm(name) {
                                ui.label(RichText::new(tr!("korunuyor", "protected")).weak());
                            } else if ui.small_button(tr!("Boşalt", "Trim")).clicked() {
                                self.send(Cmd::Trim(pids));
                            }
                            ui.end_row();
                        }
                    });
                }); });
            });
            ui.add_space(10.0);
        });
    }

    // ------------------------------------------------------------- Servisler

    fn svc_scan(&self, ctx: &egui::Context) {
        let (st, ctx) = (self.ui2.svc.clone(), ctx.clone());
        if let Ok(mut s) = st.lock() {
            if s.busy {
                return;
            }
            s.busy = true;
            s.status = tr!("Servisler okunuyor, imzalar denetleniyor…", "Reading services, checking signatures…").into();
        }
        std::thread::spawn(move || {
            let list = svc::scan();
            let backup = svc::backup().len();
            if let Ok(mut s) = st.lock() {
                s.list = list;
                s.backup = backup;
                s.busy = false;
                s.loaded = true;
                s.status.clear();
            }
            ctx.request_repaint();
        });
    }

    fn svc_job(&self, ctx: &egui::Context, targets: Vec<Service>, disable: bool, restore: bool) {
        let (st, ctx, tx) = (self.ui2.svc.clone(), ctx.clone(), self.tx.clone());
        if let Ok(mut s) = st.lock() {
            if s.busy {
                return;
            }
            s.busy = true;
        }
        std::thread::spawn(move || {
            if restore {
                if let Ok(mut s) = st.lock() {
                    s.status = tr!("İlk ayarlar geri yükleniyor…", "Restoring original settings…").into();
                }
                ctx.request_repaint();
                let (ok, all) = svc::restore_all();
                let _ = tx.send(Cmd::Note(tr!(
                    "Servisler: {}/{} servis ilk ayarına döndürüldü.",
                    "Services: {}/{} services restored to their original settings.",
                    ok,
                    all
                )));
            } else {
                let mut ok = 0;
                for (i, t) in targets.iter().enumerate() {
                    if let Ok(mut s) = st.lock() {
                        s.status = tr!("{}/{}: {} durduruluyor…", "{}/{}: stopping {}…", i + 1, targets.len(), t.display);
                    }
                    ctx.request_repaint();
                    if svc::turn_off(t, disable) {
                        ok += 1;
                    }
                }
                let _ = tx.send(Cmd::Note(tr!(
                    "Servisler: {}/{} servis durduruldu ve {} ({}).{}",
                    "Services: {}/{} services stopped and {} ({}).{}",
                    ok,
                    targets.len(),
                    if disable {
                        tr!("devre dışı bırakıldı", "disabled")
                    } else {
                        tr!("elle başlatmaya alındı", "set to manual start")
                    },
                    targets.iter().map(|t| t.name.as_str()).collect::<Vec<_>>().join(", "),
                    if ok < targets.len() {
                        tr!(
                            " Kalanlar için yönetici modu gerekir ya da servis korumalı.",
                            " The rest need administrator mode or the service is protected."
                        )
                    } else {
                        ""
                    }
                )));
            }
            let list = svc::scan();
            let backup = svc::backup().len();
            if let Ok(mut s) = st.lock() {
                s.list = list;
                s.backup = backup;
                s.busy = false;
                s.status.clear();
            }
            ctx.request_repaint();
        });
    }

    pub(crate) fn services_tab(&mut self, ui: &mut egui::Ui, s: &Snapshot) {
        let c = pal(ui);
        let (list, busy, loaded, status, backup) = {
            let g = self.ui2.svc.lock().unwrap();
            (g.list.clone(), g.busy, g.loaded, g.status.clone(), g.backup)
        };
        if !loaded && !busy {
            self.svc_scan(ui.ctx());
        }
        let mem: HashMap<u32, u64> = s.procs.iter().map(|p| (p.pid, p.mem)).collect();
        let selectable = |x: &Service| !x.group.locked();
        self.ui2
            .svc_sel
            .retain(|n| list.iter().any(|x| &x.name == n && selectable(x)));

        let running = list.iter().filter(|x| x.running).count();
        let safe_run: Vec<&Service> = list
            .iter()
            .filter(|x| x.group == Group::Safe && (x.running || x.start == 2))
            .collect();
        let safe_mem: u64 = safe_run
            .iter()
            .filter(|x| x.running && x.pid != 0)
            .map(|x| x.pid)
            .collect::<HashSet<_>>()
            .iter()
            .filter_map(|p| mem.get(p))
            .sum();
        let risky = list.iter().filter(|x| x.risk >= Risk::Unsigned).count();

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            for (title, value, col) in [
                (tr!("Çalışan servis", "Running services"), format!("{running} / {}", list.len()), None),
                (
                    tr!("Kapatılabilir, şu an açık", "Can be turned off, currently on"),
                    tr!("{} servis · {}", "{} services · {}", safe_run.len(), fb(safe_mem as f64)),
                    Some(c.green),
                ),
                (
                    tr!("İmzasız / şüpheli dosya", "Unsigned / suspicious files"),
                    risky.to_string(),
                    (risky > 0).then_some(c.orange),
                ),
                (tr!("Bu programla değiştirilen", "Changed by this program"), backup.to_string(), None),
            ] {
                card(ui).show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.set_width(200.0);
                        ui.label(RichText::new(title).weak());
                        let mut t = RichText::new(value).size(17.0).strong();
                        if let Some(col) = col {
                            t = t.color(col);
                        }
                        ui.label(t);
                    });
                });
            }
        });
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(!busy, egui::Button::new(tr!("Önerilenleri seç", "Select recommended")))
                .on_hover_text(tr!(
                    "\"Gereksiz\" grubundaki açık servislerin tümünü işaretler",
                    "Selects all services in the \"Unneeded\" group that are currently on"
                ))
                .clicked()
            {
                self.ui2.svc_sel = safe_run.iter().map(|x| x.name.clone()).collect();
                self.ui2.svc_confirm = None;
            }
            let n = self.ui2.svc_sel.len();
            if ui
                .add_enabled(
                    !busy && n > 0,
                    primary(&tr!("  Seçilen {} servisi durdur  ", "  Stop {} selected services  ", n)),
                )
                .on_hover_text(tr!(
                    "Durdurur ve başlangıç türünü \"Elle\" yapar: Windows gerçekten gerek duyarsa yeniden başlatabilir",
                    "Stops them and sets the startup type to \"Manual\": Windows can start them again if it really needs to"
                ))
                .clicked()
            {
                self.ui2.svc_confirm = Some(false);
            }
            if ui
                .add_enabled(!busy && n > 0, egui::Button::new(tr!("Durdur ve devre dışı bırak", "Stop and disable")))
                .on_hover_text(tr!(
                    "Servis hiçbir koşulda başlamaz; yalnızca emin olduklarınız için",
                    "The service will not start under any circumstances; only for the ones you are sure about"
                ))
                .clicked()
            {
                self.ui2.svc_confirm = Some(true);
            }
            if n > 0 && ui.button(tr!("Seçimi temizle", "Clear selection")).clicked() {
                self.ui2.svc_sel.clear();
                self.ui2.svc_confirm = None;
            }
            if ui
                .add_enabled(
                    !busy && backup > 0,
                    egui::Button::new(tr!("Değişiklikleri geri al ({})", "Undo changes ({})", backup)),
                )
                .on_hover_text(tr!(
                    "Bu programla kapattığınız tüm servisleri ilk ayarına döndürür",
                    "Restores every service you turned off with this program to its original setting"
                ))
                .clicked()
            {
                self.svc_job(ui.ctx(), Vec::new(), false, true);
            }
            if ui.add_enabled(!busy, egui::Button::new(tr!("Yenile", "Refresh"))).clicked() {
                self.svc_scan(ui.ctx());
            }
        });
        if let Some(disable) = self.ui2.svc_confirm {
            let n = self.ui2.svc_sel.len();
            ui.horizontal(|ui| {
                ui.colored_label(
                    c.orange,
                    tr!(
                        "{} servis durdurulacak ve {}. İlk ayarlar kaydedilir, \"Değişiklikleri geri al\" ile dönebilirsiniz.",
                        "{} services will be stopped and {}. The original settings are saved; you can go back with \"Undo changes\".",
                        n,
                        if disable {
                            tr!("devre dışı bırakılacak", "disabled")
                        } else {
                            tr!("elle başlatmaya alınacak", "set to manual start")
                        }
                    ),
                );
                if ui.button(RichText::new(tr!("Onayla", "Confirm")).strong()).clicked() {
                    let targets: Vec<Service> = list
                        .iter()
                        .filter(|x| self.ui2.svc_sel.contains(&x.name) && selectable(x))
                        .cloned()
                        .collect();
                    self.ui2.svc_sel.clear();
                    self.ui2.svc_confirm = None;
                    self.svc_job(ui.ctx(), targets, disable, false);
                }
                if ui.button(tr!("Vazgeç", "Cancel")).clicked() {
                    self.ui2.svc_confirm = None;
                }
            });
        }
        ui.horizontal(|ui| {
            ui.label(tr!("Ara:", "Search:"));
            ui.add(egui::TextEdit::singleline(&mut self.ui2.svc_filter).desired_width(160.0));
            ui.checkbox(&mut self.ui2.svc_risky, tr!("Yalnızca imzasız / şüpheli", "Only unsigned / suspicious"));
            ui.checkbox(
                &mut self.ui2.svc_core,
                tr!("Windows çekirdek servislerini de göster", "Also show Windows core system services"),
            );
            if busy {
                ui.spinner();
                ui.label(&status);
            } else if !s.admin {
                ui.colored_label(
                    c.orange,
                    tr!(
                        "Sınırlı mod: servisleri değiştirmek için yönetici gerekir.",
                        "Limited mode: changing services requires administrator rights."
                    ),
                );
            }
        });
        ui.add_space(2.0);

        // Gruplu satir listesi.
        enum Line {
            Head(Group, usize, usize),
            Svc(usize),
        }
        let f = self.ui2.svc_filter.to_lowercase();
        // Grup ici siralama: 0 calisanlar ustte (varsayilan), 1 duranlar ustte, 2 ada gore.
        let sort_id = egui::Id::new("svc_sort");
        let svc_sort: u8 = ui.ctx().data(|d| d.get_temp(sort_id)).unwrap_or(0);
        // Acik gruplar (bit maskesi). Ilk yuklemede yalnizca en cok calisan servisi olan grup aciktir;
        // arama ya da suzgec varken hepsi acik gosterilir.
        let open_id = egui::Id::new("svc_open");
        let mut open_mask: Option<u8> = ui.ctx().data(|d| d.get_temp(open_id));
        if open_mask.is_none() && !list.is_empty() {
            let best = Group::ALL
                .iter()
                .enumerate()
                .filter(|(_, g)| **g != Group::Core || self.ui2.svc_core)
                .max_by_key(|(_, g)| list.iter().filter(|x| x.group == **g && x.running).count())
                .map_or(0, |(i, _)| i);
            open_mask = Some(1u8 << best);
            ui.ctx().data_mut(|d| d.insert_temp(open_id, 1u8 << best));
        }
        let open_mask = open_mask.unwrap_or(0);
        let all_open = !f.is_empty() || self.ui2.svc_risky;
        let is_open = |g: Group| all_open || open_mask & (1u8 << Group::ALL.iter().position(|x| *x == g).unwrap_or(0)) != 0;
        let mut lines: Vec<Line> = Vec::new();
        for g in Group::ALL {
            if g == Group::Core && !self.ui2.svc_core && !self.ui2.svc_risky && f.is_empty() {
                continue;
            }
            let idx: Vec<usize> = list
                .iter()
                .enumerate()
                .filter(|(_, x)| {
                    x.group == g
                        && (!self.ui2.svc_risky || x.risk >= Risk::Unsigned)
                        && (f.is_empty()
                            || x.display.to_lowercase().contains(&f)
                            || x.name.to_lowercase().contains(&f)
                            || x.publisher.to_lowercase().contains(&f)
                            || x.file.to_lowercase().contains(&f))
                })
                .map(|(i, _)| i)
                .collect();
            if idx.is_empty() {
                continue;
            }
            let mut idx = idx;
            idx.sort_by(|a, b| {
                let (x, y) = (&list[*a], &list[*b]);
                let o = match svc_sort {
                    0 => y.running.cmp(&x.running),
                    1 => x.running.cmp(&y.running),
                    _ => std::cmp::Ordering::Equal,
                };
                o.then_with(|| x.display.to_lowercase().cmp(&y.display.to_lowercase()))
            });
            let run = idx.iter().filter(|i| list[**i].running).count();
            lines.push(Line::Head(g, idx.len(), run));
            if is_open(g) {
                lines.extend(idx.into_iter().map(Line::Svc));
            }
        }

        let sel = self.ui2.svc_sel.clone();
        let mut flips: Vec<(String, bool)> = Vec::new();
        let mut group_flip: Option<(Group, bool)> = None;
        let mut group_toggle: Option<Group> = None;
        let heights: Vec<f32> = lines
            .iter()
            .map(|l| if matches!(l, Line::Head(..)) { 34.0 } else { 23.0 })
            .collect();
        TableBuilder::new(ui)
            .striped(true)
            .sense(egui::Sense::click())
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::initial(30.0))
            .column(Column::initial(210.0).clip(true))
            .column(Column::initial(74.0))
            .column(Column::initial(104.0).clip(true))
            .column(Column::initial(70.0))
            .column(Column::initial(170.0).clip(true))
            .column(Column::remainder().clip(true))
            .header(24.0, |mut h| {
                for (i, t) in [
                    "",
                    tr!("Servis", "Service"),
                    tr!("Durum", "Status"),
                    tr!("Başlangıç", "Startup"),
                    tr!("Bellek", "Memory"),
                    tr!("İmza / yayıncı", "Signature / publisher"),
                    tr!("Ne işe yarar", "What it does"),
                ]
                .into_iter()
                .enumerate()
                {
                    h.col(|ui| {
                        // Servis ve Durum basliklari grup ici siralamayi degistirir.
                        let arrow = match (i, svc_sort) {
                            (1, 2) => " ⬆",
                            (2, 0) => " ⬇",
                            (2, 1) => " ⬆",
                            _ => "",
                        };
                        if i == 1 || i == 2 {
                            if ui
                                .add(egui::Button::new(RichText::new(format!("{t}{arrow}")).strong()).frame(false))
                                .on_hover_text(tr!("Sıralamak için tıklayın", "Click to sort"))
                                .clicked()
                            {
                                let next: u8 = match (i, svc_sort) {
                                    (1, _) => 2,
                                    (_, 0) => 1,
                                    _ => 0,
                                };
                                ui.ctx().data_mut(|d| d.insert_temp(sort_id, next));
                            }
                        } else {
                            ui.label(RichText::new(t).strong());
                        }
                    });
                }
            })
            .body(|body| {
                body.heterogeneous_rows(heights.into_iter(), |mut row| match &lines[row.index()] {
                    Line::Head(g, n, run) => {
                        let g = *g;
                        let col = match g {
                            Group::Safe => c.green,
                            Group::Optional => c.blue,
                            Group::Remote => c.orange,
                            _ => ui_text(row.index()),
                        };
                        // Grup basligi: renkli serit; tiklaninca grup acilir / kapanir.
                        let band = |ui: &mut egui::Ui| {
                            let base = if col == Color32::PLACEHOLDER { ui.visuals().text_color() } else { col };
                            ui.painter().rect_filled(
                                ui.max_rect().expand2(egui::vec2(6.0, 0.0)).shrink2(egui::vec2(0.0, 2.0)),
                                0.0,
                                base.gamma_multiply(0.16),
                            );
                        };
                        row.col(|ui| {
                            band(ui);
                            if !g.locked() {
                                let members: Vec<&Service> =
                                    list.iter().filter(|x| x.group == g).collect();
                                let mut all = members.iter().all(|x| sel.contains(&x.name));
                                if ui
                                    .add_enabled(!busy, egui::Checkbox::new(&mut all, ""))
                                    .on_hover_text(tr!(
                                        "Gruptaki tüm servisleri seç / bırak",
                                        "Select / deselect all services in the group"
                                    ))
                                    .changed()
                                {
                                    group_flip = Some((g, all));
                                }
                            }
                        });
                        row.col(|ui| {
                            band(ui);
                            let mut t = RichText::new(format!("{}  {}", if is_open(g) { "−" } else { "+" }, g.title()))
                                .strong()
                                .size(15.0);
                            if col != Color32::PLACEHOLDER {
                                t = t.color(col);
                            }
                            ui.label(t);
                        });
                        row.col(|ui| {
                            band(ui);
                            ui.label(RichText::new(tr!("{} açık", "{} running", run)).strong());
                        });
                        row.col(|ui| {
                            band(ui);
                            ui.label(tr!("{} servis", "{} services", n));
                        });
                        row.col(band);
                        row.col(band);
                        row.col(|ui| {
                            band(ui);
                            ui.label(
                                RichText::new(if is_open(g) {
                                    tr!("Daraltmak için tıklayın", "Click to collapse")
                                } else {
                                    tr!("Servisleri görmek için tıklayın", "Click to show the services")
                                })
                                .weak(),
                            );
                        });
                        if row.response().clicked() {
                            group_toggle = Some(g);
                        }
                    }
                    Line::Svc(i) => {
                        let x = &list[*i];
                        let rc = risk_color(c, x.risk);
                        row.col(|ui| {
                            if selectable(x) {
                                let mut on = sel.contains(&x.name);
                                if ui.add_enabled(!busy, egui::Checkbox::new(&mut on, "")).changed() {
                                    flips.push((x.name.clone(), on));
                                }
                            }
                        });
                        row.col(|ui| {
                            let mut t = RichText::new(&x.display);
                            if let Some(col) = rc {
                                t = t.color(col).strong();
                            } else if !x.running {
                                t = t.weak();
                            }
                            ui.label(t).on_hover_text(tr!(
                                "Servis adı: {}\nDosya: {}\n\n{}",
                                "Service name: {}\nFile: {}\n\n{}",
                                x.name,
                                x.file,
                                x.desc
                            ));
                        });
                        row.col(|ui| {
                            if x.running {
                                ui.colored_label(c.green, tr!("Çalışıyor", "Running"));
                            } else {
                                ui.label(RichText::new(tr!("Durdu", "Stopped")).weak());
                            }
                        });
                        row.col(|ui| {
                            let t = RichText::new(x.start_text());
                            ui.label(if x.start == 2 { t } else { t.weak() });
                        });
                        row.col(|ui| {
                            if let Some(m) = mem.get(&x.pid).filter(|_| x.running && x.pid != 0) {
                                ui.label(fb(*m as f64))
                                    .on_hover_text(tr!(
                                        "Servisin çalıştığı sürecin belleği (aynı süreci birden çok servis paylaşabilir)",
                                        "Memory of the process the service runs in (several services can share the same process)"
                                    ));
                            }
                        });
                        row.col(|ui| {
                            let mut t = RichText::new(&x.publisher);
                            if let Some(col) = rc {
                                t = t.color(col);
                            }
                            ui.label(t).on_hover_text(&x.file);
                        });
                        row.col(|ui| {
                            if x.note.is_empty() {
                                ui.label(RichText::new(&x.desc).weak()).on_hover_text(&x.desc);
                            } else {
                                ui.label(x.note).on_hover_text(&x.desc);
                            }
                        });
                    }
                });
            });
        for (n, on) in flips {
            if on {
                self.ui2.svc_sel.insert(n);
            } else {
                self.ui2.svc_sel.remove(&n);
            }
            self.ui2.svc_confirm = None;
        }
        if let Some(g) = group_toggle {
            let bit = 1u8 << Group::ALL.iter().position(|x| *x == g).unwrap_or(0);
            ui.ctx().data_mut(|d| d.insert_temp(open_id, open_mask ^ bit));
        }
        if let Some((g, on)) = group_flip {
            for x in list.iter().filter(|x| x.group == g) {
                if on {
                    self.ui2.svc_sel.insert(x.name.clone());
                } else {
                    self.ui2.svc_sel.remove(&x.name);
                }
            }
            self.ui2.svc_confirm = None;
        }
    }

    // --------------------------------------------------- Zamanlanmis gorevler

    fn task_scan(&self, ctx: &egui::Context) {
        let (st, ctx) = (self.ui2.tasks.clone(), ctx.clone());
        if let Ok(mut s) = st.lock() {
            if s.busy {
                return;
            }
            s.busy = true;
        }
        std::thread::spawn(move || {
            let list = tasks::scan();
            if let Ok(mut s) = st.lock() {
                s.list = list;
                s.busy = false;
                s.loaded = true;
            }
            ctx.request_repaint();
        });
    }

    pub(crate) fn tasks_view(&mut self, ui: &mut egui::Ui) {
        let c = pal(ui);
        let (list, busy, loaded) = {
            let g = self.ui2.tasks.lock().unwrap();
            (g.list.clone(), g.busy, g.loaded)
        };
        if !loaded && !busy {
            self.task_scan(ui.ctx());
        }
        let f = self.ui2.task_filter.to_lowercase();
        let mut shown: Vec<usize> = list
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                (self.ui2.task_ms || !t.microsoft || t.risk >= Risk::Unsigned)
                    && (!self.ui2.task_risky || t.risk >= Risk::Unsigned)
                    && (f.is_empty()
                        || t.path.to_lowercase().contains(&f)
                        || t.command.to_lowercase().contains(&f)
                        || t.trust.to_lowercase().contains(&f)
                        || t.author.to_lowercase().contains(&f))
            })
            .map(|(i, _)| i)
            .collect();
        let (col, desc) = self.ui2.task_sort;
        shown.sort_by(|&a, &b| {
            let (a, b) = (&list[a], &list[b]);
            let o = match col {
                0 => a.enabled.cmp(&b.enabled),
                1 => b.name.to_lowercase().cmp(&a.name.to_lowercase()),
                2 => a
                    .risk
                    .cmp(&b.risk)
                    .then_with(|| b.trust.to_lowercase().cmp(&a.trust.to_lowercase())),
                3 => b.triggers.cmp(&a.triggers),
                _ => b.author.to_lowercase().cmp(&a.author.to_lowercase()),
            };
            if desc {
                o.reverse()
            } else {
                o
            }
        });

        let third = list.iter().filter(|t| !t.microsoft).count();
        let red = list.iter().filter(|t| t.risk == Risk::Suspicious).count();
        let orange = list.iter().filter(|t| t.risk == Risk::Unsigned).count();
        ui.horizontal(|ui| {
            if ui.add_enabled(!busy, egui::Button::new(tr!("Yenile", "Refresh"))).clicked() {
                self.task_scan(ui.ctx());
            }
            ui.label(tr!("Ara:", "Search:"));
            ui.add(egui::TextEdit::singleline(&mut self.ui2.task_filter).desired_width(160.0));
            ui.checkbox(&mut self.ui2.task_risky, tr!("Yalnızca riskli", "Only risky"));
            ui.checkbox(
                &mut self.ui2.task_ms,
                tr!("Windows'un kendi görevlerini de göster", "Also show Windows' own tasks"),
            );
            if ui.button(tr!("Görev Zamanlayıcı'yı aç", "Open Task Scheduler")).clicked() {
                win::shell_open("taskschd.msc", "");
            }
            if busy {
                ui.spinner();
                ui.label(tr!("Görevler okunuyor, imzalar denetleniyor…", "Reading tasks, checking signatures…"));
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(tr!(
                "{} görev · {} tanesi Windows dışı",
                "{} tasks · {} not from Windows",
                list.len(),
                third
            ));
            if red > 0 {
                ui.colored_label(c.red, tr!("· {} şüpheli", "· {} suspicious", red));
            }
            if orange > 0 {
                ui.colored_label(
                    c.orange,
                    tr!("· {} imzasız / betik çalıştıran", "· {} unsigned / running a script", orange),
                );
            }
            ui.label(
                RichText::new(tr!(
                    "· Kırmızı: gizli, kodlanmış ya da internetten indiren komut; imzasız dosyanın riskli konumu. Turuncu: imzasız dosya ya da betik. Kutuyu kaldırmak görevi silmez, devre dışı bırakır.",
                    "· Red: hidden, encoded or internet-downloading command; unsigned file in a risky location. Orange: unsigned file or script. Unchecking the box does not delete the task, it disables it."
                ))
                .weak(),
            );
        });
        if loaded && list.len() < 20 {
            ui.colored_label(
                c.orange,
                tr!(
                    "Görevlerin çoğu yalnızca yönetici modunda okunabilir; program yönetici olarak açılmamış.",
                    "Most tasks can only be read in administrator mode; the program was not started as administrator."
                ),
            );
        }
        ui.add_space(4.0);

        let mut sort = self.ui2.task_sort;
        let mut toggle: Option<(usize, bool)> = None;
        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::initial(46.0))
            .column(Column::initial(180.0).clip(true))
            .column(Column::initial(175.0).clip(true))
            .column(Column::initial(150.0).clip(true))
            .column(Column::initial(100.0).clip(true))
            .column(Column::initial(56.0))
            .column(Column::remainder().clip(true))
            .header(24.0, |mut h| {
                for (i, (t, d)) in [
                    (tr!("Açık", "On"), true),
                    (tr!("Görev", "Task"), true),
                    (tr!("Güven / yayıncı", "Trust / publisher"), true),
                    (tr!("Ne zaman çalışır", "When it runs"), true),
                    (tr!("Oluşturan", "Author"), true),
                ]
                .into_iter()
                .enumerate()
                {
                    h.col(|ui| sort_header(ui, t, i, d, &mut sort));
                }
                h.col(|_| {});
                h.col(|ui| {
                    ui.label(RichText::new(tr!("Çalıştırdığı komut", "Command it runs")).strong());
                });
            })
            .body(|body| {
                body.rows(22.0, shown.len(), |mut row| {
                    let i = shown[row.index()];
                    let t = &list[i];
                    let rc = risk_color(c, t.risk);
                    row.col(|ui| {
                        let mut en = t.enabled;
                        if ui.checkbox(&mut en, "").changed() {
                            toggle = Some((i, en));
                        }
                    });
                    row.col(|ui| {
                        let mut x = RichText::new(&t.name);
                        if let Some(col) = rc {
                            x = x.color(col).strong();
                        }
                        if !t.enabled {
                            x = x.weak();
                        }
                        ui.label(x).on_hover_text(format!(
                            "{}{}{}",
                            t.path,
                            if t.highest {
                                tr!("\nEn yüksek yetkiyle çalışır", "\nRuns with highest privileges")
                            } else {
                                ""
                            },
                            if t.hidden { tr!("\nGizli görev", "\nHidden task") } else { "" }
                        ));
                    });
                    row.col(|ui| {
                        let mut x = RichText::new(&t.trust);
                        if let Some(col) = rc {
                            x = x.color(col);
                        }
                        ui.label(x);
                    });
                    row.col(|ui| {
                        ui.label(&t.triggers).on_hover_text(&t.triggers);
                    });
                    row.col(|ui| {
                        ui.label(RichText::new(&t.author).weak());
                    });
                    row.col(|ui| {
                        if t.exe.contains('\\') && ui.small_button(tr!("Konum", "Folder")).clicked() {
                            win::open_location(&t.exe);
                        }
                    });
                    row.col(|ui| {
                        ui.label(RichText::new(&t.command).weak())
                            .on_hover_text(&t.command);
                    });
                });
            });
        self.ui2.task_sort = sort;

        if let Some((i, en)) = toggle {
            let t = &list[i];
            if tasks::set_enabled(t, en) {
                if let Ok(mut g) = self.ui2.tasks.lock() {
                    if let Some(x) = g.list.get_mut(i) {
                        x.enabled = en;
                    }
                }
                self.send(Cmd::Note(tr!(
                    "Zamanlanmış görev {}: {}.",
                    "Scheduled task {}: {}.",
                    if en { tr!("etkinleştirildi", "enabled") } else { tr!("devre dışı bırakıldı", "disabled") },
                    t.path
                )));
            } else {
                self.send(Cmd::Note(tr!(
                    "Zamanlanmış görev değiştirilemedi: {} (yönetici modu gerekir ya da görev korumalı).",
                    "Could not change scheduled task: {} (administrator mode is required or the task is protected).",
                    t.path
                )));
            }
        }
    }
}

/// Varsayilan metin rengi icin yer tutucu.
fn ui_text(_: usize) -> Color32 {
    Color32::PLACEHOLDER
}

fn ram_report(ui: &mut egui::Ui, c: Pal, r: &RamReport, full: f32, cache_col: Color32, free_col: Color32) {
    let total = r.total.max(1) as f32;
    let gain = r.avail_after.saturating_sub(r.avail_before);
    let used_b = r.total.saturating_sub(r.avail_before);
    let used_a = r.total.saturating_sub(r.avail_after);
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr!("Son temizlik — {}", "Last cleanup — {}", r.time)).strong());
        if r.auto {
            ui.label(RichText::new(tr!("(otomatik)", "(automatic)")).weak());
        }
    });
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("+{}", fb(gain as f64)))
                .size(28.0)
                .strong()
                .color(c.green),
        );
        ui.vertical(|ui| {
            ui.label(tr!("kullanılabilir bellek kazanıldı", "of available memory gained"));
            ui.label(
                RichText::new(tr!(
                    "{} sürecin belleği boşaltıldı{}",
                    "Memory of {} processes trimmed{}",
                    r.trimmed,
                    if r.standby {
                        tr!(" · bekleme önbelleği temizlendi", " · standby cache cleaned")
                    } else {
                        tr!(
                            " · bekleme önbelleği için yönetici modu gerekir",
                            " · the standby cache requires administrator mode"
                        )
                    }
                ))
                .weak(),
            );
        });
    });
    ui.add_space(4.0);
    let w = (full - 420.0).clamp(240.0, 700.0);
    egui::Grid::new("ramba").num_columns(3).spacing([12.0, 6.0]).show(ui, |ui| {
        for (label, used, avail, free) in [
            (tr!("Önce", "Before"), used_b, r.avail_before, r.free_before),
            (tr!("Sonra", "After"), used_a, r.avail_after, r.free_after),
        ] {
            let pct = used as f32 * 100.0 / total;
            ui.label(RichText::new(label).strong());
            stack_bar(
                ui,
                w,
                22.0,
                &[
                    (used as f32 / total, heat(c, pct, 70.0, 88.0)),
                    (avail.saturating_sub(free) as f32 / total, cache_col),
                    (free as f32 / total, free_col),
                ],
            );
            ui.label(tr!(
                "%{:.0} kullanımda · {} kullanılan · {} boş",
                "{:.0}% in use · {} used · {} free",
                pct,
                fb(used as f64),
                fb(free as f64)
            ));
            ui.end_row();
        }
    });
    if !r.top.is_empty() {
        ui.add_space(6.0);
        ui.label(RichText::new(tr!("En çok bellek bırakan programlar", "Programs that released the most memory")).strong());
        let max = r.top.iter().map(|x| x.1).max().unwrap_or(1).max(1) as f32;
        let bw = (full - 560.0).clamp(160.0, 520.0);
        egui::Grid::new("ramtop").num_columns(4).spacing([12.0, 4.0]).show(ui, |ui| {
            for (name, b, a) in r.top.iter().take(8) {
                ui.label(name);
                // Gri: onceki kullanim; uzerine renkli: sonraki kullanim.
                let (rect, _) = ui.allocate_exact_size(egui::vec2(bw, 12.0), egui::Sense::hover());
                let p = ui.painter();
                p.rect_filled(rect, 3.0, ui.visuals().extreme_bg_color);
                let mut r1 = rect;
                r1.set_width(rect.width() * (*b as f32 / max));
                p.rect_filled(r1, 3.0, free_col);
                let mut r2 = rect;
                r2.set_width(rect.width() * (*a as f32 / max));
                p.rect_filled(r2, 3.0, crate::accent());
                ui.label(format!("{} » {}", fb(*b as f64), fb(*a as f64)));
                ui.colored_label(c.green, format!("−{}", fb((b - a) as f64)));
                ui.end_row();
            }
        });
    }
    ui.add_space(4.0);
    ui.label(
        RichText::new(tr!(
            "Boşaltılan bellek programlardan silinmez; kullanılmayan sayfalar disk belleğine ve önbelleğe taşınır. Programlar ihtiyaç duydukça bir kısmını geri alır.",
            "Trimmed memory is not deleted from programs; unused pages are moved to the page file and the standby cache. Programs take some of it back as they need it."
        ))
        .weak(),
    );
}
