//! Baslangic, Disk Temizligi, Programlar ve Guc sekmeleri.

use crate::apps::{self, Program};
use crate::clean::{self, Cat};
use crate::lang::t;
use crate::tr;
use crate::mon::{self, Cmd, Snapshot};
use crate::sig::Risk;
use crate::startup::{self, Item, Kind};
use crate::win::{self, PowerMode};
use crate::{cfg, fb, pal, pct, risk_color, App};
use eframe::egui::{self, RichText};
use egui_extras::{Column, TableBuilder};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn prog_key(p: &Program) -> String {
    format!("{}|{}", p.name, p.version)
}

#[derive(Default)]
struct StartupState {
    items: Vec<Item>,
    busy: bool,
    loaded: bool,
}

#[derive(Default)]
struct DiskState {
    cats: Vec<Cat>,
    drives: Vec<(String, u64, u64)>,
    busy: bool,
    scanned: bool,
    status: String,
    result: Option<String>,
    pending_total: u64,
    step: (usize, usize),
}

#[derive(Default)]
struct ProgState {
    list: Vec<Program>,
    busy: bool,
    loaded: bool,
    /// Kaldirma sihirbazi acilan programlar: (anahtar, ad).
    pending: Vec<(String, String)>,
    done: Option<String>,
    last: Option<Instant>,
}

pub struct State {
    startup: Arc<Mutex<StartupState>>,
    startup_filter: String,
    startup_sort: (usize, bool),
    view_tasks: bool,
    show_manual: bool,
    disk: Arc<Mutex<DiskState>>,
    progs: Arc<Mutex<ProgState>>,
    prog_filter: String,
    prog_by_size: bool,
    prog_confirm: Option<String>,
    autostart: Arc<Mutex<Option<bool>>>,
    autostart_asked: bool,
}

impl State {
    pub fn new() -> Self {
        Self {
            startup: Arc::default(),
            startup_filter: String::new(),
            startup_sort: (4, true),
            view_tasks: std::env::var("FB_TASKS").is_ok(),
            show_manual: false,
            disk: Arc::default(),
            progs: Arc::default(),
            prog_filter: String::new(),
            prog_by_size: false,
            prog_confirm: None,
            autostart: Arc::default(),
            autostart_asked: false,
        }
    }
}

fn windows_tools() -> [(&'static str, &'static str, &'static str); 14] {
    [
        (t("Program Ekle / Kaldır", "Add / Remove Programs"), "appwiz.cpl", ""),
        (t("Yüklü uygulamalar (Ayarlar)", "Installed apps (Settings)"), "ms-settings:appsfeatures", ""),
        (t("Başlangıç uygulamaları (Ayarlar)", "Startup apps (Settings)"), "ms-settings:startupapps", ""),
        (t("Depolama ayarları", "Storage settings"), "ms-settings:storagesense", ""),
        (t("Disk Temizleme", "Disk Cleanup"), "cleanmgr.exe", ""),
        ("Windows Update", "ms-settings:windowsupdate", ""),
        (t("Servisler", "Services"), "services.msc", ""),
        (t("Görev Zamanlayıcı", "Task Scheduler"), "taskschd.msc", ""),
        (t("Aygıt Yöneticisi", "Device Manager"), "devmgmt.msc", ""),
        (t("Kaynak İzleyicisi", "Resource Monitor"), "resmon.exe", ""),
        (t("Olay Görüntüleyici", "Event Viewer"), "eventvwr.msc", ""),
        (t("Sistem Bilgisi", "System Information"), "msinfo32.exe", ""),
        (t("Güç Seçenekleri", "Power Options"), "powercfg.cpl", ""),
        (t("Windows Güvenliği", "Windows Security"), "windowsdefender:", ""),
    ]
}

impl App {
    // ------------------------------------------------------------ Baslangic

    fn startup_scan(&self, ctx: &egui::Context) {
        let st = self.ui.startup.clone();
        let ctx = ctx.clone();
        if let Ok(mut s) = st.lock() {
            if s.busy {
                return;
            }
            s.busy = true;
        }
        std::thread::spawn(move || {
            let items = startup::scan();
            if let Ok(mut s) = st.lock() {
                s.items = items;
                s.busy = false;
                s.loaded = true;
            }
            ctx.request_repaint();
        });
    }

    pub(crate) fn startup_tab(&mut self, ui: &mut egui::Ui) {
        let c = pal(ui);
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut self.ui.view_tasks,
                false,
                RichText::new(t("  Başlangıçta açılanlar  ", "  Startup programs  ")).strong(),
            );
            ui.selectable_value(
                &mut self.ui.view_tasks,
                true,
                RichText::new(t(
                    "  Zamanlanmış görevler (tümü)  ",
                    "  Scheduled tasks (all)  ",
                ))
                .strong(),
            );
        });
        ui.separator();
        if self.ui.view_tasks {
            self.tasks_view(ui);
            return;
        }
        let (items, busy, loaded) = {
            let s = self.ui.startup.lock().unwrap();
            (s.items.clone(), s.busy, s.loaded)
        };
        if !loaded && !busy {
            self.startup_scan(ui.ctx());
        }

        let f = self.ui.startup_filter.to_lowercase();
        let mut shown: Vec<usize> = items
            .iter()
            .enumerate()
            .filter(|(_, it)| {
                (self.ui.show_manual || it.kind != Kind::Service || it.enabled)
                    && (f.is_empty()
                        || it.name.to_lowercase().contains(&f)
                        || it.command.to_lowercase().contains(&f)
                        || it.publisher.to_lowercase().contains(&f))
            })
            .map(|(i, _)| i)
            .collect();
        let (scol, sdesc) = self.ui.startup_sort;
        shown.sort_by(|&a, &b| {
            let (x, y) = (&items[a], &items[b]);
            let o = match scol {
                1 => y.name.to_lowercase().cmp(&x.name.to_lowercase()),
                2 => x
                    .risk
                    .cmp(&y.risk)
                    .then_with(|| y.publisher.to_lowercase().cmp(&x.publisher.to_lowercase())),
                3 => y.source.cmp(x.source),
                // Acik / Durum: etkin olanlar, icinde de riskli olanlar once.
                _ => x.enabled.cmp(&y.enabled).then_with(|| x.risk.cmp(&y.risk)),
            };
            let o = if sdesc { o.reverse() } else { o };
            o.then_with(|| x.name.to_lowercase().cmp(&y.name.to_lowercase()))
        });
        let mut sort = self.ui.startup_sort;

        ui.horizontal(|ui| {
            if ui
                .add_enabled(!busy, egui::Button::new(t("Yenile", "Refresh")))
                .clicked()
            {
                self.startup_scan(ui.ctx());
            }
            ui.label(t("Ara:", "Search:"));
            ui.add(egui::TextEdit::singleline(&mut self.ui.startup_filter).desired_width(160.0));
            ui.checkbox(
                &mut self.ui.show_manual,
                t(
                    "Otomatik olmayan servisleri de göster",
                    "Also show services that are not automatic",
                ),
            );
            if busy {
                ui.spinner();
                ui.label(t(
                    "Taranıyor, imzalar denetleniyor…",
                    "Scanning, checking signatures…",
                ));
            }
        });
        let on = items.iter().filter(|i| i.enabled).count();
        let bad = items
            .iter()
            .filter(|i| i.enabled && i.risk >= Risk::Unsigned)
            .count();
        ui.horizontal(|ui| {
            ui.label(tr!(
                "{} öğe · {} etkin",
                "{} items · {} enabled",
                items.len(),
                on
            ));
            if bad > 0 {
                ui.colored_label(
                    c.orange,
                    tr!(
                        "· {} etkin öğe imzasız / şüpheli",
                        "· {} enabled items unsigned / suspicious",
                        bad
                    ),
                );
            }
            ui.label(
                RichText::new(t(
                    "· Kapatılan öğe silinmez; kutuyu yeniden işaretleyerek geri açabilirsiniz. Windows'un kendi servis ve görevleri listelenmez.",
                    "· A disabled item is not deleted; tick the box again to re-enable it. Windows' own services and tasks are not listed.",
                ))
                .weak(),
            );
        });
        ui.add_space(4.0);

        let mut toggle: Option<(usize, bool)> = None;
        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::initial(46.0))
            .column(Column::initial(230.0).clip(true))
            .column(Column::initial(230.0).clip(true))
            .column(Column::initial(170.0).clip(true))
            .column(Column::initial(80.0))
            .column(Column::initial(60.0))
            .column(Column::remainder().clip(true))
            .header(24.0, |mut h| {
                let heads = [
                    t("Açık", "On"),
                    t("Ad", "Name"),
                    t("İmza / yayıncı", "Signature / publisher"),
                    t("Tür", "Type"),
                    t("Durum", "Status"),
                ];
                for (i, head) in heads.into_iter().enumerate() {
                    h.col(|ui| crate::sort_header(ui, head, i, true, &mut sort));
                }
                h.col(|_| {});
                h.col(|ui| {
                    ui.label(RichText::new(t("Komut", "Command")).strong());
                });
            })
            .body(|body| {
                body.rows(22.0, shown.len(), |mut row| {
                    let i = shown[row.index()];
                    let it = &items[i];
                    let rc = risk_color(c, it.risk);
                    row.col(|ui| {
                        let mut en = it.enabled;
                        if ui.checkbox(&mut en, "").changed() {
                            toggle = Some((i, en));
                        }
                    });
                    row.col(|ui| {
                        let mut rt = RichText::new(&it.name);
                        if let Some(col) = rc {
                            rt = rt.color(col).strong();
                        }
                        if !it.enabled {
                            rt = rt.weak();
                        }
                        ui.label(rt);
                    });
                    row.col(|ui| {
                        let mut rt = RichText::new(&it.publisher);
                        if let Some(col) = rc {
                            rt = rt.color(col);
                        }
                        ui.label(rt);
                    });
                    row.col(|ui| {
                        ui.label(it.source);
                    });
                    row.col(|ui| {
                        let txt = if !it.state.is_empty() {
                            it.state.as_str()
                        } else if it.enabled {
                            t("Etkin", "Enabled")
                        } else {
                            t("Kapalı", "Off")
                        };
                        if it.enabled {
                            ui.colored_label(c.green, txt);
                        } else {
                            ui.label(RichText::new(txt).weak());
                        }
                    });
                    row.col(|ui| {
                        if !it.exe.is_empty() && ui.small_button(t("Konum", "Location")).clicked() {
                            win::open_location(&it.exe);
                        }
                    });
                    row.col(|ui| {
                        ui.label(RichText::new(&it.command).weak())
                            .on_hover_text(&it.command);
                    });
                });
            });

        // Acik ve Durum ayni olcute gore siralar.
        if sort.0 == 0 {
            let old = self.ui.startup_sort;
            sort = (4, if old.0 == 4 { !old.1 } else { true });
        }
        self.ui.startup_sort = sort;
        if let Some((i, en)) = toggle {
            let it = &items[i];
            match startup::set_enabled(it, en) {
                Ok(()) => {
                    if let Ok(mut s) = self.ui.startup.lock() {
                        if let Some(x) = s.items.get_mut(i) {
                            x.enabled = en;
                            if x.kind == Kind::Service {
                                x.state = if en {
                                    t("Otomatik", "Automatic")
                                } else {
                                    t("Elle", "Manual")
                                }
                                .into();
                            }
                        }
                    }
                    self.send(Cmd::Note(tr!(
                        "Başlangıç: {} {}.",
                        "Startup: {} {}.",
                        it.name,
                        if en {
                            t("etkinleştirildi", "enabled")
                        } else {
                            t("kapatıldı", "disabled")
                        }
                    )));
                }
                Err(e) => self.send(Cmd::Note(tr!(
                    "Başlangıç: {} — {}.",
                    "Startup: {} — {}.",
                    it.name,
                    e
                ))),
            }
        }
    }

    // ------------------------------------------------------- Disk temizligi

    fn disk_job(&self, ctx: &egui::Context, do_clean: bool) {
        let st = self.ui.disk.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        if let Ok(mut s) = st.lock() {
            if s.busy {
                return;
            }
            s.busy = true;
            if s.cats.is_empty() {
                s.cats = clean::categories();
            }
        }
        std::thread::spawn(move || {
            let before = clean::drives();
            let n = st.lock().map(|s| s.cats.len()).unwrap_or(0);
            let (mut freed, mut skipped) = (0u64, 0u64);
            for i in 0..n {
                let Some(mut cat) = st.lock().ok().and_then(|mut s| {
                    s.step = (i, n);
                    s.status = format!(
                        "{}: {}",
                        if do_clean {
                            t("Temizleniyor", "Cleaning")
                        } else {
                            t("Taranıyor", "Scanning")
                        },
                        s.cats[i].name
                    );
                    Some(s.cats[i].clone())
                }) else {
                    break;
                };
                ctx.request_repaint();
                if do_clean {
                    if !cat.on {
                        continue;
                    }
                    freed += clean::clean(&mut cat);
                    skipped += cat.skipped;
                } else {
                    clean::scan(&mut cat);
                }
                if let Ok(mut s) = st.lock() {
                    let on = s.cats[i].on;
                    s.cats[i] = cat;
                    s.cats[i].on = on;
                }
            }
            let after = clean::drives();
            if let Ok(mut s) = st.lock() {
                s.busy = false;
                s.scanned = true;
                s.status.clear();
                s.drives = after.clone();
                if do_clean {
                    let sys = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
                    let find = |v: &[(String, u64, u64)]| {
                        v.iter()
                            .find(|d| d.0.to_uppercase().starts_with(&sys.to_uppercase()))
                            .map(|d| d.2)
                    };
                    let mut msg = tr!(
                        "Son temizlik: {} kazanıldı",
                        "Last cleanup: {} freed",
                        fb(freed as f64)
                    );
                    if let (Some(b), Some(a)) = (find(&before), find(&after)) {
                        msg += &tr!(
                            " · {} boş alan {} » {}",
                            " · {} free space {} » {}",
                            sys,
                            fb(b as f64),
                            fb(a as f64)
                        );
                    }
                    if skipped > 0 {
                        msg += &tr!(
                            " · {} dosya kullanımda olduğu için atlandı",
                            " · {} files skipped because they are in use",
                            skipped
                        );
                    }
                    s.result = Some(msg.clone());
                    s.pending_total += freed;
                    let _ = tx.send(Cmd::Note(tr!(
                        "Disk temizliği — {}.",
                        "Disk cleanup — {}.",
                        msg
                    )));
                }
            }
            ctx.request_repaint();
        });
    }

    pub(crate) fn disk_tab(&mut self, ui: &mut egui::Ui) {
        let c = pal(ui);
        let (cats, drives, busy, scanned, status, result, step) = {
            let mut s = self.ui.disk.lock().unwrap();
            if s.pending_total > 0 {
                self.cfg.cleaned += s.pending_total;
                s.pending_total = 0;
                cfg::save(&self.cfg);
            }
            (
                s.cats.clone(),
                s.drives.clone(),
                s.busy,
                s.scanned,
                s.status.clone(),
                s.result.clone(),
                s.step,
            )
        };
        if !scanned && !busy {
            self.disk_job(ui.ctx(), false);
        }
        let sys = std::env::var("SystemDrive")
            .unwrap_or_else(|_| "C:".into())
            .to_uppercase();
        let selected: u64 = cats.iter().filter(|x| x.on).map(|x| x.bytes).sum();
        let all: u64 = cats.iter().map(|x| x.bytes).sum();
        let files: u64 = cats.iter().map(|x| x.files).sum();
        let max = cats.iter().map(|x| x.bytes).max().unwrap_or(1).max(1) as f32;
        let free_col = ui.visuals().widgets.noninteractive.bg_stroke.color;

        egui::ScrollArea::vertical().show(ui, |ui| {
            let full = ui.available_width() - 28.0;
            ui.add_space(6.0);

            // ---- diskler
            ui.label(RichText::new(t("Diskler", "Drives")).strong());
            ui.horizontal_wrapped(|ui| {
                for (name, total, free) in &drives {
                    let used = total.saturating_sub(*free);
                    let frac = used as f32 / (*total).max(1) as f32;
                    let col = crate::heat(c, frac * 100.0, 80.0, 92.0);
                    let is_sys = name.to_uppercase().starts_with(&sys);
                    crate::card(ui).show(ui, |ui| {
                        ui.vertical(|ui| {
                        ui.set_width(330.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(name.trim_end_matches('\\')).size(20.0).strong());
                            ui.label(
                                RichText::new(if is_sys {
                                    t("Windows diski", "Windows drive")
                                } else {
                                    t("Veri diski", "Data drive")
                                })
                                .weak(),
                            );
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(
                                    RichText::new(tr!("%{:.0} dolu", "{:.0}% full", frac * 100.0))
                                        .size(16.0)
                                        .strong()
                                        .color(col),
                                );
                            });
                        });
                        crate::stack_bar(ui, 330.0, 14.0, &[(frac, col)]);
                        ui.horizontal(|ui| {
                            ui.label(tr!(
                                "{} dolu / {}",
                                "{} used / {}",
                                fb(used as f64),
                                fb(*total as f64)
                            ));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(
                                    RichText::new(tr!("{} boş", "{} free", fb(*free as f64)))
                                        .strong(),
                                );
                            });
                        });
                        // Secilenler silinirse boslugun ne olacagi.
                        if is_sys && selected > 0 && !busy {
                            ui.colored_label(
                                c.green,
                                tr!(
                                    "Temizlik sonrası boş alan: yaklaşık {}",
                                    "Free space after cleanup: about {}",
                                    fb((*free + selected) as f64)
                                ),
                            );
                        }
                        });
                    });
                }
            });

            // ---- ozet ve eylemler
            ui.add_space(8.0);
            crate::card(ui).show(ui, |ui| {
                ui.set_width(full);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(t(
                                "Seçilenlerle kazanılacak alan",
                                "Space freed by the selection",
                            ))
                            .weak(),
                        );
                        ui.label(
                            RichText::new(if scanned || all > 0 {
                                fb(selected as f64)
                            } else {
                                "…".into()
                            })
                            .size(30.0)
                            .strong()
                            .color(c.green),
                        );
                        ui.label(tr!(
                            "Bulunan toplam {} · {} dosya",
                            "Total found {} · {} files",
                            fb(all as f64),
                            files
                        ));
                    });
                    ui.add_space(24.0);
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            if ui
                                .add_enabled(
                                    !busy && scanned && selected > 0,
                                    crate::primary(t(
                                        "    Seçilenleri temizle    ",
                                        "    Clean selected    ",
                                    ))
                                        .min_size(egui::vec2(210.0, 40.0)),
                                )
                                .clicked()
                            {
                                self.disk_job(ui.ctx(), true);
                            }
                            if ui
                                .add_enabled(
                                    !busy,
                                    egui::Button::new(t("Yeniden tara", "Scan again"))
                                        .min_size(egui::vec2(0.0, 40.0)),
                                )
                                .clicked()
                            {
                                self.disk_job(ui.ctx(), false);
                            }
                        });
                        if busy {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(&status);
                            });
                            ui.add(
                                egui::ProgressBar::new(step.0 as f32 / step.1.max(1) as f32)
                                    .desired_width(330.0)
                                    .desired_height(8.0),
                            );
                        } else {
                            ui.horizontal(|ui| {
                                ui.label(t("Seç:", "Select:"));
                                let mut set: Option<u8> = None;
                                if ui.small_button(t("Önerilenler", "Recommended")).clicked() {
                                    set = Some(0);
                                }
                                if ui.small_button(t("Tümü", "All")).clicked() {
                                    set = Some(1);
                                }
                                if ui.small_button(t("Hiçbiri", "None")).clicked() {
                                    set = Some(2);
                                }
                                if let Some(m) = set {
                                    let def = clean::categories();
                                    if let Ok(mut s) = self.ui.disk.lock() {
                                        for x in s.cats.iter_mut() {
                                            x.on = match m {
                                                0 => def.iter().any(|d| d.id == x.id && d.on),
                                                1 => true,
                                                _ => false,
                                            };
                                        }
                                    }
                                }
                            });
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new(t("Bugüne kadar kazanılan", "Freed so far")).weak(),
                            );
                            ui.label(
                                RichText::new(fb(self.cfg.cleaned as f64)).size(20.0).strong(),
                            );
                        });
                    });
                });
                if let Some(r) = &result {
                    ui.add_space(6.0);
                    egui::Frame::none()
                        .fill(c.green.gamma_multiply(0.16))
                        .rounding(6.0)
                        .inner_margin(8.0)
                        .show(ui, |ui| {
                            ui.set_width(full - 16.0);
                            ui.label(RichText::new(format!("✔ {r}")).strong().color(c.green));
                        });
                }
            });

            // ---- kategoriler
            ui.add_space(10.0);
            ui.label(
                RichText::new(t("Silinmesi zararsız dosyalar", "Files that are safe to delete"))
                    .strong(),
            );
            ui.label(
                RichText::new(t(
                    "İşaretli satırlar temizlenir. Kullanımdaki dosyalar ve son 24 saatte değişen geçici dosyalar atlanır; programlarınız ve belgeleriniz bu listede yer almaz.",
                    "Ticked rows are cleaned. Files in use and temporary files changed in the last 24 hours are skipped; your programs and documents are not in this list.",
                ))
                .weak(),
            );
            ui.add_space(4.0);
            let mut flips: Vec<(usize, bool)> = Vec::new();
            ui.push_id("cats", |ui| {
                TableBuilder::new(ui)
                    .striped(true)
                    .vscroll(false)
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                    .column(Column::exact(34.0))
                    .column(Column::remainder().at_least(320.0).clip(true))
                    .column(Column::exact(210.0))
                    .column(Column::exact(90.0))
                    .column(Column::exact(100.0))
                    .column(Column::exact(150.0))
                    .header(24.0, |mut h| {
                        let heads = [
                            "",
                            t("Kategori", "Category"),
                            t("Payı", "Share"),
                            t("Boyut", "Size"),
                            t("Dosya", "Files"),
                            t("Son temizlik", "Last cleanup"),
                        ];
                        for head in heads {
                            h.col(|ui| {
                                ui.label(RichText::new(head).strong());
                            });
                        }
                    })
                    .body(|mut body| {
                        for (i, cat) in cats.iter().enumerate() {
                            body.row(44.0, |mut row| {
                                row.col(|ui| {
                                    let mut on = cat.on;
                                    if ui.add_enabled(!busy, egui::Checkbox::new(&mut on, "")).changed() {
                                        flips.push((i, on));
                                    }
                                });
                                row.col(|ui| {
                                    ui.vertical(|ui| {
                                        ui.add_space(4.0);
                                        ui.label(RichText::new(cat.name).strong());
                                        ui.add(
                                            egui::Label::new(RichText::new(cat.desc).weak().small())
                                                .truncate(),
                                        )
                                        .on_hover_text(cat.desc);
                                    });
                                });
                                row.col(|ui| {
                                    let col = if cat.on { crate::accent() } else { free_col };
                                    crate::stack_bar(ui, 200.0, 12.0, &[(cat.bytes as f32 / max, col)]);
                                });
                                row.col(|ui| {
                                    if cat.scanned {
                                        let rt = RichText::new(fb(cat.bytes as f64));
                                        ui.label(if cat.bytes > 100 * 1_048_576 { rt.strong() } else { rt });
                                    } else {
                                        ui.label("…");
                                    }
                                });
                                row.col(|ui| {
                                    if cat.scanned {
                                        ui.label(RichText::new(cat.files.to_string()).weak());
                                    }
                                });
                                row.col(|ui| {
                                    if cat.freed > 0 {
                                        ui.colored_label(
                                            c.green,
                                            tr!("✔ {} silindi", "✔ {} deleted", fb(cat.freed as f64)),
                                        );
                                    } else if cat.skipped > 0 {
                                        ui.label(
                                            RichText::new(tr!(
                                                "{} dosya kullanımda",
                                                "{} files in use",
                                                cat.skipped
                                            ))
                                            .weak(),
                                        );
                                    }
                                });
                            });
                        }
                    });
            });
            if !flips.is_empty() {
                if let Ok(mut s) = self.ui.disk.lock() {
                    for (i, on) in flips {
                        if let Some(x) = s.cats.get_mut(i) {
                            x.on = on;
                        }
                    }
                }
            }

            // ---- derin temizlik
            ui.add_space(12.0);
            crate::card(ui).show(ui, |ui| {
                ui.set_width(full);
                ui.label(
                    RichText::new(t(
                        "Derin temizlik (Windows'un kendi araçları)",
                        "Deep clean (Windows' own tools)",
                    ))
                    .strong(),
                );
                ui.label(
                    RichText::new(t(
                        "Eski Windows güncelleme bileşenleri ve önceki Windows kurulumu (Windows.old) korumalı klasörlerdir; bunları yalnızca Windows'un kendi araçları güvenle siler.",
                        "Old Windows update components and the previous Windows installation (Windows.old) are protected folders; only Windows' own tools can delete them safely.",
                    ))
                    .weak(),
                );
                let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
                let old = std::path::Path::new(&windir).with_file_name("Windows.old");
                if old.is_dir() {
                    ui.colored_label(
                        c.orange,
                        t(
                            "Windows.old klasörü mevcut (genelde 10-30 GB). Depolama ayarları > Geçici dosyalar > Önceki Windows yüklemeleri ile silebilirsiniz.",
                            "A Windows.old folder exists (usually 10-30 GB). You can delete it under Storage settings > Temporary files > Previous Windows installations.",
                        ),
                    );
                }
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .button(t(
                            "Bileşen deposunu temizle (DISM)",
                            "Clean component store (DISM)",
                        ))
                        .on_hover_text(t(
                            "dism /online /cleanup-image /startcomponentcleanup — birkaç dakika sürer, bu sırada işlemci kullanır",
                            "dism /online /cleanup-image /startcomponentcleanup — takes a few minutes and uses the CPU meanwhile",
                        ))
                        .clicked()
                    {
                        let tx = self.tx.clone();
                        self.send(Cmd::Note(
                            t(
                                "DISM bileşen temizliği başlatıldı (birkaç dakika sürebilir)…",
                                "DISM component cleanup started (may take a few minutes)…",
                            )
                            .into(),
                        ));
                        std::thread::spawn(move || {
                            let ok = win::run_status(
                                "dism",
                                &["/online", "/cleanup-image", "/startcomponentcleanup"],
                            );
                            let _ = tx.send(Cmd::Note(if ok {
                                t(
                                    "DISM bileşen temizliği tamamlandı.",
                                    "DISM component cleanup finished.",
                                )
                                .into()
                            } else {
                                t(
                                    "DISM bileşen temizliği başarısız (yönetici modu gerekir).",
                                    "DISM component cleanup failed (administrator mode required).",
                                )
                                .into()
                            }));
                        });
                    }
                    if ui.button(t("Depolama ayarları", "Storage settings")).clicked() {
                        win::shell_open("ms-settings:storagesense", "");
                    }
                    if ui
                        .button(t("Windows Disk Temizleme", "Windows Disk Cleanup"))
                        .clicked()
                    {
                        win::shell_open("cleanmgr.exe", "");
                    }
                });
            });
            ui.add_space(10.0);
        });
    }

    // ------------------------------------------------- Programlar / Windows

    fn prog_reload(&self, ctx: &egui::Context) {
        let (st, ctx, tx) = (self.ui.progs.clone(), ctx.clone(), self.tx.clone());
        if let Ok(mut s) = st.lock() {
            if s.busy {
                return;
            }
            s.busy = true;
        }
        std::thread::spawn(move || {
            let l = apps::list();
            if let Ok(mut s) = st.lock() {
                // Kaldirilmasi beklenenlerden listeden cikanlar: kaldirma bitti.
                let gone: Vec<(String, String)> = s
                    .pending
                    .iter()
                    .filter(|(k, _)| !l.iter().any(|p| &prog_key(p) == k))
                    .cloned()
                    .collect();
                for (k, name) in gone {
                    s.pending.retain(|x| x.0 != k);
                    s.done = Some(tr!(
                        "{} kaldırıldı ve listeden çıkarıldı.",
                        "{} was uninstalled and removed from the list.",
                        name
                    ));
                    let _ = tx.send(Cmd::Note(tr!(
                        "Program kaldırıldı: {}.",
                        "Program uninstalled: {}.",
                        name
                    )));
                }
                s.list = l;
                s.busy = false;
                s.loaded = true;
                s.last = Some(Instant::now());
            }
            ctx.request_repaint();
        });
    }

    pub(crate) fn programs_tab(&mut self, ui: &mut egui::Ui) {
        let c = pal(ui);
        let (mut list, busy, loaded, pending, done, stale) = {
            let s = self.ui.progs.lock().unwrap();
            (
                s.list.clone(),
                s.busy,
                s.loaded,
                s.pending.clone(),
                s.done.clone(),
                s.last.map_or(true, |t| t.elapsed() > Duration::from_secs(3)),
            )
        };
        // Sekme acikken liste kendiliginden guncel tutulur.
        if !busy && stale {
            self.prog_reload(ui.ctx());
        }
        ui.ctx().request_repaint_after(Duration::from_secs(2));

        ui.add_space(4.0);
        ui.label(
            RichText::new(t("Windows yönetim araçları", "Windows management tools")).strong(),
        );
        ui.horizontal_wrapped(|ui| {
            for (label, target, params) in windows_tools() {
                if ui.button(label).clicked() {
                    win::shell_open(target, params);
                }
            }
        });
        ui.add_space(6.0);
        ui.separator();

        let f = self.ui.prog_filter.to_lowercase();
        if !f.is_empty() {
            list.retain(|p| {
                p.name.to_lowercase().contains(&f) || p.publisher.to_lowercase().contains(&f)
            });
        }
        if self.ui.prog_by_size {
            list.sort_by(|a, b| b.size.cmp(&a.size));
        }
        let total: u64 = list.iter().map(|p| p.size).sum();
        ui.horizontal(|ui| {
            ui.label(RichText::new(t("Yüklü programlar", "Installed programs")).strong());
            ui.label(t("Ara:", "Search:"));
            ui.add(egui::TextEdit::singleline(&mut self.ui.prog_filter).desired_width(180.0));
            ui.selectable_value(&mut self.ui.prog_by_size, false, t("Ada göre", "By name"));
            ui.selectable_value(&mut self.ui.prog_by_size, true, t("Boyuta göre", "By size"));
            ui.label(tr!(
                "{} program · bildirilen toplam {}",
                "{} programs · reported total {}",
                list.len(),
                fb(total as f64)
            ));
            if !loaded {
                ui.spinner();
            }
        });
        if !pending.is_empty() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.colored_label(
                    c.orange,
                    tr!(
                        "Kaldırma sihirbazı açık: {}. Sihirbaz bitince program listeden kendiliğinden çıkar.",
                        "Uninstall wizard open: {}. The program leaves the list by itself when the wizard finishes.",
                        pending.iter().map(|p| p.1.as_str()).collect::<Vec<_>>().join(", ")
                    ),
                );
            });
        } else if let Some(d) = &done {
            ui.colored_label(c.green, format!("✔ {d}"));
        }
        ui.label(
            RichText::new(t(
                "Kaldır düğmesi programın kendi kaldırma sihirbazını açar; liste birkaç saniyede bir kendiliğinden yenilenir. Microsoft Store uygulamaları bu listede yer almaz (üstteki \"Yüklü uygulamalar\" düğmesinden yönetilir).",
                "The Uninstall button opens the program's own uninstall wizard; the list refreshes by itself every few seconds. Microsoft Store apps are not in this list (manage them with the \"Installed apps\" button above).",
            ))
            .weak(),
        );
        ui.add_space(4.0);

        let confirm = self.ui.prog_confirm.clone();
        let mut ask: Option<Option<String>> = None;
        let mut run: Option<Program> = None;
        let mut cancel: Option<String> = None;
        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::initial(330.0).clip(true))
            .column(Column::initial(230.0).clip(true))
            .column(Column::initial(130.0).clip(true))
            .column(Column::initial(90.0))
            .column(Column::initial(90.0))
            .column(Column::remainder())
            .header(24.0, |mut h| {
                let heads = [
                    "Program",
                    t("Yayıncı", "Publisher"),
                    t("Sürüm", "Version"),
                    t("Boyut", "Size"),
                    t("Kurulum", "Installed"),
                    "",
                ];
                for head in heads {
                    h.col(|ui| {
                        ui.label(RichText::new(head).strong());
                    });
                }
            })
            .body(|body| {
                body.rows(23.0, list.len(), |mut row| {
                    let p = &list[row.index()];
                    let key = prog_key(p);
                    let waiting = pending.iter().any(|x| x.0 == key);
                    row.col(|ui| {
                        let rt = RichText::new(&p.name);
                        ui.label(if waiting { rt.weak() } else { rt });
                    });
                    row.col(|ui| {
                        ui.label(RichText::new(&p.publisher).weak());
                    });
                    row.col(|ui| {
                        ui.label(&p.version);
                    });
                    row.col(|ui| {
                        ui.label(if p.size > 0 {
                            fb(p.size as f64)
                        } else {
                            String::new()
                        });
                    });
                    row.col(|ui| {
                        ui.label(&p.date);
                    });
                    row.col(|ui| {
                        if waiting {
                            ui.spinner();
                            ui.colored_label(c.orange, t("Kaldırılıyor…", "Uninstalling…"));
                            if ui
                                .small_button("✖")
                                .on_hover_text(t(
                                    "Sihirbazı iptal ettiyseniz beklemeyi bırak",
                                    "Stop waiting if you cancelled the wizard",
                                ))
                                .clicked()
                            {
                                cancel = Some(key);
                            }
                        } else if confirm.as_ref() == Some(&key) {
                            if ui
                                .small_button(
                                    RichText::new(t("Evet, kaldır", "Yes, uninstall")).color(c.red),
                                )
                                .clicked()
                            {
                                run = Some(p.clone());
                                ask = Some(None);
                            }
                            if ui.small_button(t("Vazgeç", "Cancel")).clicked() {
                                ask = Some(None);
                            }
                        } else if ui.small_button(t("Kaldır", "Uninstall")).clicked() {
                            ask = Some(Some(key));
                        }
                    });
                });
            });
        if let Some(a) = ask {
            self.ui.prog_confirm = a;
        }
        if let Some(k) = cancel {
            if let Ok(mut s) = self.ui.progs.lock() {
                s.pending.retain(|x| x.0 != k);
            }
        }
        if let Some(p) = run {
            let ok = apps::uninstall(&p);
            if ok {
                if let Ok(mut s) = self.ui.progs.lock() {
                    s.pending.push((prog_key(&p), p.name.clone()));
                    s.done = None;
                }
            }
            self.send(Cmd::Note(if ok {
                tr!(
                    "Kaldırma sihirbazı açıldı: {}.",
                    "Uninstall wizard opened: {}.",
                    p.name
                )
            } else {
                tr!(
                    "Kaldırma sihirbazı açılamadı: {}.",
                    "Could not open the uninstall wizard: {}.",
                    p.name
                )
            }));
        }
    }

    // ------------------------------------------------------ Guc / Hizlandir

    pub(crate) fn power_tab(&mut self, ui: &mut egui::Ui, s: &Snapshot) {
        let c = pal(ui);
        if !self.ui.autostart_asked {
            self.ui.autostart_asked = true;
            let (a, ctx) = (self.ui.autostart.clone(), ui.ctx().clone());
            std::thread::spawn(move || {
                let v = win::autostart_enabled();
                if let Ok(mut g) = a.lock() {
                    *g = Some(v);
                }
                ctx.request_repaint();
            });
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.add_space(6.0);
            ui.heading(t("İşlemci güç modu", "CPU power mode"));
            ui.label(t(
                "Fan hızını donanım (BIOS) belirler; program onu işlemcinin ürettiği ısıyı azaltarak düşürür. \
                 Modlar ayrı bir \"HFix\" güç planına yazılır; kendi planlarınıza dokunulmaz ve aşağıdan tek tıkla geri dönebilirsiniz.",
                "Fan speed is set by the hardware (BIOS); the program lowers it by reducing the heat the CPU produces. \
                 Modes are written to a separate \"HFix\" power plan; your own plans are not touched and you can switch back below with one click.",
            ));
            let ours = s.power.plan_guid.eq_ignore_ascii_case(win::FB_PLAN);
            let hex = |v: Option<(u32, u32)>| v.map_or("?".to_string(), |x| x.0.to_string());
            let boost = match s.power.boost.map(|b| b.0) {
                Some(0) => t("kapalı", "off"),
                Some(1) => t("açık", "on"),
                Some(2) => t("agresif", "aggressive"),
                Some(3) => t("verimli", "efficient"),
                Some(4) => t("verimli agresif", "efficient aggressive"),
                Some(_) => t("özel", "custom"),
                None => "?",
            };
            ui.label(
                RichText::new(tr!(
                    "Şu an (prizde): plan \"{}\" · alt sınır %{} · üst sınır %{} · enerji tercihi (EPP) {} · turbo {}",
                    "Now (plugged in): plan \"{}\" · minimum {}% · maximum {}% · energy preference (EPP) {} · turbo {}",
                    s.power.plan,
                    hex(s.power.min),
                    hex(s.power.max),
                    hex(s.power.epp),
                    boost
                ))
                .strong(),
            );
            ui.add_space(4.0);
            let modes = [
                (
                    PowerMode::Quiet,
                    t(
                        "Turbo kapalı, üst sınır %99. En serin ve en sessiz mod; ağır işlerde yaklaşık %30 yavaşlar.",
                        "Turbo off, maximum 99%. The coolest and quietest mode; heavy work runs about 30% slower.",
                    ),
                ),
                (
                    PowerMode::Smart,
                    t(
                        "ÖNERİLEN. Boştayken frekansı düşürüp soğur, yük gelince tam hıza ve turboya çıkar. Günlük kullanım ve VMware için dengeli.",
                        "RECOMMENDED. Lowers the frequency and cools down when idle, goes to full speed and turbo under load. Balanced for daily use and VMware.",
                    ),
                ),
                (
                    PowerMode::Performance,
                    t(
                        "Yük altında en yüksek hız (agresif turbo, tüm çekirdekler uyanık); ama boştayken frekansı düşürür. Performanstan ödün vermeden fanı dinlendirir.",
                        "Maximum speed under load (aggressive turbo, all cores awake), but lowers the frequency when idle. Rests the fan without giving up performance.",
                    ),
                ),
                (
                    PowerMode::Max,
                    t(
                        "İşlemci hep en yüksek frekansta. Fan sürekli hızlı döner; yalnızca gerektiğinde kullanın.",
                        "CPU always at maximum frequency. The fan runs fast all the time; use only when needed.",
                    ),
                ),
            ];
            for (mode, desc) in modes {
                let active = ours && mode.names_plan(&s.power.plan);
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let col = match mode {
                            PowerMode::Max => c.red,
                            PowerMode::Smart | PowerMode::Quiet => c.green,
                            PowerMode::Performance => c.blue,
                        };
                        if ui
                            .add_sized(
                                [170.0, 30.0],
                                egui::Button::new(RichText::new(mode.title()).strong().color(col)),
                            )
                            .clicked()
                        {
                            self.send(Cmd::Power(mode));
                        }
                        if active {
                            ui.colored_label(c.green, t("● etkin", "● active"));
                        }
                        ui.add(egui::Label::new(desc).wrap());
                    });
                });
            }
            ui.add_space(4.0);
            ui.label(RichText::new(t("Tüm güç planları", "All power plans")).strong());
            for p in &s.power.plans {
                ui.horizontal(|ui| {
                    if p.active {
                        ui.colored_label(c.green, t("● etkin", "● active"));
                    } else if ui.small_button(t("Etkinleştir", "Activate")).clicked() {
                        self.send(Cmd::PowerPlan(p.guid.clone(), p.name.clone()));
                    }
                    ui.label(&p.name);
                });
            }

            ui.add_space(12.0);
            ui.separator();
            ui.heading(t("Bellek", "Memory"));
            ui.horizontal(|ui| {
                if ui.button(t("RAM'i şimdi temizle", "Clean RAM now")).clicked() {
                    self.tab = crate::Tab::Mem;
                    self.ram_clean(s.ram_seq);
                }
                ui.label(t(
                    "Sanal makine (VMware) süreçlerine dokunulmaz.",
                    "Virtual machine (VMware) processes are not touched.",
                ));
            });
            let mut a = self.cfg.auto;
            ui.horizontal(|ui| {
                ui.checkbox(
                    &mut a.ram_on,
                    t(
                        "Bellek kullanımı şu oranı geçince otomatik temizle:",
                        "Clean automatically when memory usage exceeds:",
                    ),
                );
                ui.add(egui::Slider::new(&mut a.ram_pct, 50..=95).suffix(" %"));
            });

            ui.add_space(12.0);
            ui.separator();
            ui.heading(t("Otomatik kısma", "Automatic throttling"));
            ui.horizontal(|ui| {
                ui.checkbox(
                    &mut a.thr_on,
                    t(
                        "30 saniye boyunca şu orandan fazla CPU kullanan süreçleri otomatik kıs:",
                        "Automatically throttle processes that use more CPU than this for 30 seconds:",
                    ),
                );
                ui.add(egui::Slider::new(&mut a.thr_pct, 5..=90).suffix(" %"));
            });
            ui.label(
                RichText::new(t(
                    "Kısılan süreç kapanmaz; düşük öncelik + verimlilik moduna alınır. Sistem ve VMware süreçleri hariç tutulur. \
                     O an kullandığınız ağır program (derleme, video işleme) da kısılabilir; Süreçler sekmesinden geri alınır.",
                    "A throttled process is not closed; it is switched to low priority + efficiency mode. System and VMware processes are excluded. \
                     A heavy program you are using at the moment (compiling, video processing) may also be throttled; undo it from the Processes tab.",
                ))
                .weak(),
            );
            if a != self.cfg.auto {
                self.cfg.auto = a;
                cfg::save(&self.cfg);
                self.send(Cmd::Auto(a));
            }
            if !self.cfg.rules.is_empty() {
                ui.add_space(4.0);
                ui.label(
                    RichText::new(t("Her zaman kısılan programlar", "Programs always throttled"))
                        .strong(),
                );
                let mut remove: Option<String> = None;
                ui.horizontal_wrapped(|ui| {
                    for r in &self.cfg.rules {
                        if ui
                            .button(format!("{r}  ✖"))
                            .on_hover_text(t("Kuralı kaldır", "Remove rule"))
                            .clicked()
                        {
                            remove = Some(r.clone());
                        }
                    }
                });
                if let Some(r) = remove {
                    self.cfg.rules.retain(|x| x != &r);
                    cfg::save(&self.cfg);
                    self.send(Cmd::Rules(self.cfg.rules.clone()));
                }
            }

            ui.add_space(12.0);
            ui.separator();
            ui.heading(t("Program ayarları", "Program settings"));
            let cur = self.ui.autostart.lock().ok().and_then(|g| *g);
            match cur {
                None => {
                    ui.label(t(
                        "Otomatik başlatma durumu okunuyor…",
                        "Reading the autostart status…",
                    ));
                }
                Some(v) => {
                    let mut on = v;
                    if ui
                        .checkbox(
                            &mut on,
                            t(
                                "Windows açılışında otomatik başlat (simge durumunda, yönetici onayı sormadan)",
                                "Start with Windows (minimized, without asking for administrator approval)",
                            ),
                        )
                        .changed()
                    {
                        let (a2, tx, ctx) =
                            (self.ui.autostart.clone(), self.tx.clone(), ui.ctx().clone());
                        std::thread::spawn(move || {
                            let ok = win::autostart_set(on);
                            let now = win::autostart_enabled();
                            if let Ok(mut g) = a2.lock() {
                                *g = Some(now);
                            }
                            let _ = tx.send(Cmd::Note(if ok {
                                tr!(
                                    "Otomatik başlatma {}.",
                                    "Autostart {}.",
                                    if on {
                                        t("açıldı", "turned on")
                                    } else {
                                        t("kapatıldı", "turned off")
                                    }
                                )
                            } else {
                                t(
                                    "Otomatik başlatma değiştirilemedi (yönetici modu gerekir).",
                                    "Autostart could not be changed (administrator mode required).",
                                )
                                .into()
                            }));
                            ctx.request_repaint();
                        });
                    }
                    ui.label(
                        RichText::new(t(
                            "Bu seçenek bir zamanlanmış görev oluşturur. Programı Başlangıç klasörüne kısayolla eklerseniz her açılışta yönetici onayı (UAC) sorulur; bu yöntemde sorulmaz.",
                            "This option creates a scheduled task. If you add the program to the Startup folder with a shortcut, administrator approval (UAC) is asked at every start; with this method it is not.",
                        ))
                        .weak(),
                    );
                }
            }
            ui.horizontal(|ui| {
                if ui.button(t("DNS önbelleğini temizle", "Flush DNS cache")).clicked() {
                    self.send(Cmd::FlushDns);
                }
            });

            ui.add_space(12.0);
            ui.separator();
            ui.heading(t(
                "Son 1 dakikada işlemciyi en çok ısıtanlar",
                "What heated the CPU most in the last minute",
            ));
            let mut agg: HashMap<&str, (f32, Vec<u32>)> = HashMap::new();
            for p in &s.procs {
                let e = agg.entry(p.name.as_str()).or_default();
                e.0 += p.avg;
                e.1.push(p.pid);
            }
            let mut v: Vec<_> = agg.into_iter().collect();
            v.sort_by(|a, b| b.1 .0.total_cmp(&a.1 .0));
            egui::Grid::new("hot").num_columns(3).striped(true).show(ui, |ui| {
                for (name, (avg, pids)) in v.into_iter().take(8) {
                    if avg < 0.3 {
                        break;
                    }
                    ui.label(name);
                    pct(ui, c, avg);
                    if mon::is_critical(name) || mon::is_vm(name) {
                        ui.label("");
                    } else if ui.small_button(t("Kıs", "Throttle")).clicked() {
                        self.send(Cmd::Throttle(pids, true));
                    }
                    ui.end_row();
                }
            });
            ui.add_space(8.0);
            ui.label(RichText::new(t("Çekirdek başına yük", "Load per core")).strong());
            ui.horizontal_wrapped(|ui| {
                for (i, core) in s.cores.iter().enumerate() {
                    ui.add(
                        egui::ProgressBar::new(core / 100.0)
                            .desired_width(110.0)
                            .text(tr!("Ç{} %{:.0}", "C{} {:.0}%", i + 1, core)),
                    );
                }
            });
            ui.add_space(10.0);
        });
    }
}
