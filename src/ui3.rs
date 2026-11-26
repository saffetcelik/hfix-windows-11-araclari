//! Risk Analizi sekmesi.

use crate::mon::{Cmd, Snapshot};
use crate::risk::{self, Finding, Held};
use crate::lang::t;
use crate::sig::Risk;
use crate::tr;
use crate::{card, pal, primary, win, App};
use eframe::egui::{self, RichText};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct RiskState {
    list: Vec<Finding>,
    held: Vec<Held>,
    busy: bool,
    scanned: bool,
    status: String,
    when: String,
    trusted: usize,
}

#[derive(Default)]
pub struct State {
    st: Arc<Mutex<RiskState>>,
    /// (bulgu anahtari, kalici sil mi)
    confirm: Option<(String, bool)>,
    purge_confirm: Option<String>,
    started: bool,
}

impl App {
    fn risk_scan(&self, ctx: &egui::Context, s: &Snapshot) {
        let (st, ctx) = (self.ui3.st.clone(), ctx.clone());
        if let Ok(mut g) = st.lock() {
            if g.busy {
                return;
            }
            g.busy = true;
            g.status = t("Tarama başlıyor…", "Starting scan…").into();
        }
        let procs: Vec<_> = s
            .procs
            .iter()
            .map(|p| (p.name.clone(), p.exe.clone(), p.risk, p.trust.clone()))
            .collect();
        std::thread::spawn(move || {
            let (st2, ctx2) = (st.clone(), ctx.clone());
            let list = risk::scan(procs, &move |m: &str| {
                if let Ok(mut g) = st2.lock() {
                    g.status = m.to_string();
                }
                ctx2.request_repaint();
            });
            if let Ok(mut g) = st.lock() {
                g.list = list;
                g.held = risk::held();
                g.trusted = risk::trusted().len();
                g.busy = false;
                g.scanned = true;
                g.when = win::clock();
                g.status.clear();
            }
            ctx.request_repaint();
        });
    }

    /// Arka planda bir is calistirir; bitince durumu gunceller.
    fn risk_job(&self, ctx: &egui::Context, status: &str, job: impl FnOnce(&Arc<Mutex<RiskState>>) -> String + Send + 'static) {
        let (st, ctx, tx) = (self.ui3.st.clone(), ctx.clone(), self.tx.clone());
        if let Ok(mut g) = st.lock() {
            if g.busy {
                return;
            }
            g.busy = true;
            g.status = status.to_string();
        }
        std::thread::spawn(move || {
            let msg = job(&st);
            if !msg.is_empty() {
                let _ = tx.send(Cmd::Note(msg));
            }
            if let Ok(mut g) = st.lock() {
                g.held = risk::held();
                g.trusted = risk::trusted().len();
                g.busy = false;
                g.status.clear();
            }
            ctx.request_repaint();
        });
    }

    pub(crate) fn risk_tab(&mut self, ui: &mut egui::Ui, s: &Snapshot) {
        let c = pal(ui);
        let (list, held, busy, scanned, status, when, trusted) = {
            let g = self.ui3.st.lock().unwrap();
            (
                g.list.clone(),
                g.held.clone(),
                g.busy,
                g.scanned,
                g.status.clone(),
                g.when.clone(),
                g.trusted,
            )
        };
        // Ilk acilista, imza denetimleri bittikten sonra kendiliginden tara.
        if !self.ui3.started && !s.procs.is_empty() {
            self.ui3.started = true;
            self.risk_scan(ui.ctx(), s);
        }
        let red = list.iter().filter(|f| f.level == Risk::Suspicious).count();
        let orange = list.len() - red;
        let threats = list
            .iter()
            .filter(|f| f.verdict.as_ref().is_some_and(|v| v.0))
            .count();

        ui.add_space(6.0);
        let full = ui.available_width() - 28.0;
        card(ui).show(ui, |ui| {
            ui.set_width(full);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    let (text, col) = if !scanned {
                        (t("Taranıyor…", "Scanning…").to_string(), c.blue)
                    } else if threats > 0 {
                        (tr!("{} doğrulanmış tehdit", "Confirmed threats: {}", threats), c.red)
                    } else if red > 0 {
                        (tr!("{} şüpheli bulgu", "Suspicious findings: {}", red), c.red)
                    } else if orange > 0 {
                        (tr!("{} incelenecek bulgu", "Findings to review: {}", orange), c.orange)
                    } else {
                        (t("Şüpheli bir şey bulunmadı", "Nothing suspicious found").to_string(), c.green)
                    };
                    ui.label(RichText::new(t("Risk durumu", "Risk status")).weak());
                    ui.label(RichText::new(text).size(26.0).strong().color(col));
                    let last = if when.is_empty() {
                        String::new()
                    } else {
                        tr!(" · son tarama {}", " · last scan {}", when)
                    };
                    ui.label(tr!(
                        "{} şüpheli · {} imzasız / incelenmeli{}",
                        "{} suspicious · {} unsigned / to review{}",
                        red,
                        orange,
                        last
                    ));
                });
                ui.add_space(20.0);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(!busy, primary(t("    Yeniden tara    ", "    Scan again    ")).min_size(egui::vec2(170.0, 38.0)))
                            .clicked()
                        {
                            self.risk_scan(ui.ctx(), s);
                        }
                        let files: Vec<String> = list.iter().filter(|f| f.has_file()).map(|f| f.key.clone()).collect();
                        if ui
                            .add_enabled(
                                !busy && !files.is_empty(),
                                egui::Button::new(t("Bulguları Defender ile doğrula", "Verify findings with Defender")).min_size(egui::vec2(0.0, 38.0)),
                            )
                            .on_hover_text(t(
                                "Listedeki her dosyayı Microsoft Defender'a taratır; gerçek virüs olanlar \"TEHDİT\" olarak işaretlenir",
                                "Scans every file in the list with Microsoft Defender; real malware is marked as \"THREAT\"",
                            ))
                            .clicked()
                        {
                            self.risk_job(ui.ctx(), t("Defender dosyaları tarıyor…", "Defender is scanning the files…"), move |st| {
                                let mut found = 0;
                                for (i, key) in files.iter().enumerate() {
                                    let path = st.lock().ok().and_then(|mut g| {
                                        g.status = tr!("Defender tarıyor: {}/{}", "Defender scanning: {}/{}", i + 1, files.len());
                                        g.list.iter().find(|f| &f.key == key).map(|f| f.path.clone())
                                    });
                                    let Some(path) = path else { continue };
                                    let (v, text) = risk::defender_file(&path);
                                    if v == Some(true) {
                                        found += 1;
                                    }
                                    if let Ok(mut g) = st.lock() {
                                        if let Some(f) = g.list.iter_mut().find(|f| &f.key == key) {
                                            f.verdict = Some((v == Some(true), text));
                                            if v == Some(true) {
                                                f.level = Risk::Suspicious;
                                            }
                                        }
                                    }
                                }
                                tr!(
                                    "Risk analizi: {} dosya Defender ile tarandı, {} tehdit bulundu.",
                                    "Risk analysis: Defender scanned {} files, threats found: {}.",
                                    files.len(),
                                    found
                                )
                            });
                        }
                        if ui
                            .add_enabled(!busy, egui::Button::new(t("Defender hızlı tarama", "Defender quick scan")).min_size(egui::vec2(0.0, 38.0)))
                            .on_hover_text(t(
                                "Tüm sistemi Microsoft Defender ile hızlı tarar (birkaç dakika sürer). Bulduklarını Defender kendisi temizler.",
                                "Runs a Microsoft Defender quick scan of the whole system (takes a few minutes). Defender cleans up what it finds itself.",
                            ))
                            .clicked()
                        {
                            self.risk_job(
                                ui.ctx(),
                                t("Defender hızlı taraması sürüyor (birkaç dakika)…", "Defender quick scan in progress (a few minutes)…"),
                                |_| risk::defender_quick(),
                            );
                        }
                        if ui
                            .add(egui::Button::new(t("Windows Güvenliği", "Windows Security")).min_size(egui::vec2(0.0, 38.0)))
                            .clicked()
                        {
                            win::shell_open("windowsdefender:", "");
                        }
                    });
                    if busy {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(&status);
                        });
                    } else if !s.admin {
                        ui.colored_label(
                            c.orange,
                            t(
                                "Sınırlı mod: görevler taranamaz, temizleme için yönetici gerekir.",
                                "Limited mode: scheduled tasks cannot be scanned, and cleaning requires administrator rights.",
                            ),
                        );
                    }
                });
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new(t(
                    "Bu sekme bir antivirüs değildir: imzası olmayan, riskli konumdan çalışan ya da gizlenmeye çalışan şeyleri kurallarla bulur. \
                     Kırmızı satır \"büyük olasılıkla istenmeyen\", turuncu satır \"tanımıyorsanız inceleyin\" demektir; kendi kurduğunuz imzasız programlar da turuncu çıkar. \
                     Emin değilseniz önce \"Defender ile doğrula\"ya, sonra \"Karantina\"ya basın: karantinadaki dosya geri yüklenebilir.",
                    "This tab is not an antivirus: it uses rules to find things that are unsigned, run from a risky location or try to hide. \
                     A red row means \"most likely unwanted\", an orange row means \"review it if you don't recognize it\"; unsigned programs you installed yourself also show up in orange. \
                     If you are not sure, click \"Verify with Defender\" first, then \"Quarantine\": a quarantined file can be restored.",
                ))
                .weak(),
            );
        });
        ui.add_space(6.0);

        let confirm = self.ui3.confirm.clone();
        let mut ask: Option<Option<(String, bool)>> = None;
        let mut act: Option<(Finding, bool)> = None;
        let mut trust: Option<String> = None;
        let mut scan_one: Option<Finding> = None;
        let held_h = if held.is_empty() { 0.0 } else { 34.0 + 24.0 * held.len().min(5) as f32 };
        let table_h = (ui.available_height() - held_h - 30.0).max(120.0);
        // Her bulgu kendi kutusunda: ad, neden, konum ve dugmeler alt alta durur; pencere daralinca
        // metin alt satira gecer, hicbir sey sagda gorunmez kalmaz.
        egui::ScrollArea::vertical()
            .id_salt("risk")
            .max_height(table_h)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                let w = ui.available_width() - 16.0;
                for (n, f) in list.iter().enumerate() {
                    let col = if f.level == Risk::Suspicious { c.red } else { c.orange };
                    let mut frame = egui::Frame::none()
                        .inner_margin(egui::Margin::symmetric(10.0, 8.0))
                        .rounding(6.0);
                    if n % 2 == 0 {
                        frame = frame.fill(ui.visuals().faint_bg_color);
                    }
                    frame.show(ui, |ui| {
                        ui.set_width(w - 20.0);
                        ui.horizontal_wrapped(|ui| {
                            ui.colored_label(col, if f.level == Risk::Suspicious { t("■ Şüpheli", "■ Suspicious") } else { t("■ İncele", "■ Review") });
                            ui.label(RichText::new(&f.name).color(col).strong().size(15.0));
                            ui.label(RichText::new(f.kinds()).weak());
                            match &f.verdict {
                                Some((true, v)) => {
                                    ui.colored_label(c.red, RichText::new(tr!("Defender: TEHDİT · {}", "Defender: THREAT · {}", v)).strong());
                                }
                                Some((false, v)) => {
                                    ui.colored_label(c.green, format!("Defender: {v}"));
                                }
                                None => {}
                            }
                        });
                        ui.add(egui::Label::new(&f.reason).wrap());
                        if !f.path.is_empty() {
                            ui.add(egui::Label::new(RichText::new(&f.path).weak()).wrap());
                        }
                        ui.add_space(2.0);
                        ui.horizontal_wrapped(|ui| {
                            if let Some((tool, params)) = f.setting() {
                                if ui.button(t("Ayarı aç", "Open setting")).clicked() {
                                    win::shell_open(tool, params);
                                }
                            } else if confirm.as_ref().map(|x| &x.0) == Some(&f.key) {
                                let del = confirm.as_ref().is_some_and(|x| x.1);
                                ui.colored_label(
                                    c.red,
                                    if del {
                                        t("Dosya kalıcı olarak silinecek, geri alınamaz. Emin misiniz?", "The file will be deleted permanently and cannot be recovered. Are you sure?")
                                    } else {
                                        t("Karantinaya alınsın mı?", "Move to quarantine?")
                                    },
                                );
                                if ui.button(RichText::new(t("Evet", "Yes")).color(c.red)).clicked() {
                                    act = Some((f.clone(), del));
                                    ask = Some(None);
                                }
                                if ui.button(t("Vazgeç", "Cancel")).clicked() {
                                    ask = Some(None);
                                }
                            } else {
                                if ui
                                    .add_enabled(!busy, egui::Button::new(t("Karantinaya al", "Quarantine")))
                                    .on_hover_text(t(
                                        "Süreci sonlandırır, başlangıç / servis / görev kaydını kapatır, dosyayı karantina klasörüne taşır. Geri alınabilir.",
                                        "Ends the process, turns off its startup / service / task entry and moves the file to the quarantine folder. Can be undone.",
                                    ))
                                    .clicked()
                                {
                                    ask = Some(Some((f.key.clone(), false)));
                                }
                                if ui
                                    .add_enabled(!busy, egui::Button::new(RichText::new(t("Kalıcı sil", "Delete permanently")).color(c.red)))
                                    .on_hover_text(t(
                                        "Süreci sonlandırır, kayıtlarını siler ve dosyayı kalıcı olarak siler. Geri alınamaz.",
                                        "Ends the process, deletes its entries and deletes the file permanently. Cannot be undone.",
                                    ))
                                    .clicked()
                                {
                                    ask = Some(Some((f.key.clone(), true)));
                                }
                                if ui
                                    .button(t("Güvenilir say", "Mark as trusted"))
                                    .on_hover_text(t("Bu öğeyi tanıyorum; bir daha listeleme", "I know this item; don't list it again"))
                                    .clicked()
                                {
                                    trust = Some(f.key.clone());
                                }
                            }
                            if f.verdict.is_none()
                                && f.has_file()
                                && ui
                                    .add_enabled(!busy, egui::Button::new(t("Defender ile tara", "Scan with Defender")))
                                    .clicked()
                            {
                                scan_one = Some(f.clone());
                            }
                            if f.path.contains('\\') && Path::new(&f.path).exists() && ui.button(t("Konumu aç", "Open location")).clicked() {
                                win::open_location(&f.path);
                            }
                        });
                    });
                    ui.add_space(2.0);
                }
            });
        if scanned && list.is_empty() && !busy {
            ui.colored_label(c.green, t("✔ İşaretlenen bir öğe yok.", "✔ No flagged items."));
        }
        if trusted > 0 {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(tr!(
                        "{} öğe \"güvenilir\" olarak işaretli ve gizleniyor.",
                        "Items marked as \"trusted\" and hidden: {}.",
                        trusted
                    ))
                    .weak(),
                );
                if ui.small_button(t("Güvenilir listesini sıfırla", "Reset trusted list")).clicked() {
                    risk::clear_trusted();
                    self.risk_scan(ui.ctx(), s);
                }
            });
        }

        // ---- karantina
        if !held.is_empty() {
            ui.separator();
            ui.label(RichText::new(tr!("Karantina ({} dosya)", "Quarantine (files: {})", held.len())).strong());
            let pc = self.ui3.purge_confirm.clone();
            let mut restore: Option<Held> = None;
            let mut purge: Option<Held> = None;
            let mut pask: Option<Option<String>> = None;
            egui::ScrollArea::vertical().id_salt("held").max_height(130.0).show(ui, |ui| {
                for h in &held {
                    ui.horizontal(|ui| {
                        if ui.small_button(t("Geri yükle", "Restore")).clicked() {
                            restore = Some(h.clone());
                        }
                        if pc.as_ref() == Some(&h.stored) {
                            if ui.small_button(RichText::new(t("Evet, sil", "Yes, delete")).color(c.red)).clicked() {
                                purge = Some(h.clone());
                                pask = Some(None);
                            }
                            if ui.small_button(t("Vazgeç", "Cancel")).clicked() {
                                pask = Some(None);
                            }
                        } else if ui.small_button(RichText::new(t("Kalıcı sil", "Delete permanently")).color(c.red)).clicked() {
                            pask = Some(Some(h.stored.clone()));
                        }
                        ui.label(&h.original);
                        ui.label(RichText::new(&h.time).weak());
                    });
                }
            });
            if let Some(p) = pask {
                self.ui3.purge_confirm = p;
            }
            if let Some(h) = restore {
                self.risk_job(ui.ctx(), t("Geri yükleniyor…", "Restoring…"), move |_| match risk::restore(&h) {
                    Ok(()) => tr!(
                        "Karantinadan geri yüklendi: {}. Başlangıç / servis kaydını yeniden açmak için ilgili sekmeyi kullanın.",
                        "Restored from quarantine: {}. To turn its startup / service entry back on, use the related tab.",
                        h.original
                    ),
                    Err(e) => tr!("Geri yüklenemedi: {} — {}.", "Could not restore: {} — {}.", h.original, e),
                });
            }
            if let Some(h) = purge {
                self.risk_job(ui.ctx(), t("Siliniyor…", "Deleting…"), move |_| match risk::purge(&h) {
                    Ok(()) => tr!(
                        "Karantinadaki dosya kalıcı olarak silindi: {}.",
                        "Quarantined file deleted permanently: {}.",
                        h.original
                    ),
                    Err(e) => tr!("Silinemedi: {} — {}.", "Could not delete: {} — {}.", h.original, e),
                });
            }
        }

        if let Some(a) = ask {
            self.ui3.confirm = a;
        }
        if let Some(key) = trust {
            risk::trust(&key);
            if let Ok(mut g) = self.ui3.st.lock() {
                g.list.retain(|f| f.key != key);
                g.trusted = risk::trusted().len();
            }
        }
        if let Some(f) = scan_one {
            self.risk_job(ui.ctx(), t("Defender dosyayı tarıyor…", "Defender is scanning the file…"), move |st| {
                let (v, text) = risk::defender_file(&f.path);
                if let Ok(mut g) = st.lock() {
                    if let Some(x) = g.list.iter_mut().find(|x| x.key == f.key) {
                        x.verdict = Some((v == Some(true), text.clone()));
                        if v == Some(true) {
                            x.level = Risk::Suspicious;
                        }
                    }
                }
                tr!("Risk analizi: {} — {}.", "Risk analysis: {} — {}.", f.name, text)
            });
        }
        if let Some((f, delete)) = act {
            let pids: Vec<u32> = s
                .procs
                .iter()
                .filter(|p| !f.path.is_empty() && p.exe.eq_ignore_ascii_case(&f.path))
                .map(|p| p.pid)
                .collect();
            self.risk_job(ui.ctx(), t("Temizleniyor…", "Cleaning…"), move |st| match risk::neutralize(&f, delete, &pids) {
                Ok(done) => {
                    if let Ok(mut g) = st.lock() {
                        g.list.retain(|x| x.key != f.key);
                    }
                    tr!("Risk analizi: {} — {}.", "Risk analysis: {} — {}.", f.name, done)
                }
                Err(e) => tr!("Risk analizi: {} temizlenemedi — {}.", "Risk analysis: could not clean {} — {}.", f.name, e),
            });
        }
    }
}

use std::path::Path;
