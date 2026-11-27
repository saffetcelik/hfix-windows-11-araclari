//! Windows Hızlandırma sekmesi: ayarlar ve gereksiz uygulamalar.

use crate::mon::{Cmd, Snapshot};
use crate::tweak::{self, App as StoreApp, Tweak, GROUPS};
use crate::{card, pal, primary, tr, App};
use eframe::egui::{self, RichText};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct BoostState {
    on: HashMap<&'static str, bool>,
    apps: Vec<StoreApp>,
    busy: bool,
    loaded: bool,
    status: String,
}

pub struct State {
    st: Arc<Mutex<BoostState>>,
    list: Vec<Tweak>,
    view_apps: bool,
    app_sel: HashSet<String>,
    confirm_all: bool,
    confirm_apps: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            st: Arc::default(),
            list: tweak::all(),
            view_apps: false,
            app_sel: HashSet::new(),
            confirm_all: false,
            confirm_apps: false,
        }
    }
}

fn states() -> HashMap<&'static str, bool> {
    tweak::all().iter().map(|t| (t.id, tweak::applied(t))).collect()
}

impl App {
    /// Arka planda bir is calistirir, sonra ayar durumlarini yeniden okur.
    fn boost_job(&self, ctx: &egui::Context, status: &str, apps_too: bool, job: impl FnOnce(&Arc<Mutex<BoostState>>) -> String + Send + 'static) {
        let (st, ctx, tx) = (self.ui4.st.clone(), ctx.clone(), self.tx.clone());
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
            let on = states();
            let apps = if apps_too { Some(tweak::apps()) } else { None };
            if let Ok(mut g) = st.lock() {
                g.on = on;
                if let Some(a) = apps {
                    g.apps = a;
                }
                g.busy = false;
                g.loaded = true;
                g.status.clear();
            }
            ctx.request_repaint();
        });
    }

    fn boost_apply(&self, ctx: &egui::Context, ids: Vec<&'static str>, undo: bool) {
        let label = if undo { tr!("Geri alınıyor…", "Undoing…") } else { tr!("Ayarlar uygulanıyor…", "Applying settings…") };
        self.boost_job(ctx, label, false, move |st| {
            let all = tweak::all();
            let (mut ok, mut fail) = (0, Vec::new());
            for (i, id) in ids.iter().enumerate() {
                let Some(t) = all.iter().find(|t| t.id == *id) else { continue };
                if let Ok(mut g) = st.lock() {
                    g.status = format!("{}/{}: {}", i + 1, ids.len(), t.name);
                }
                let done = if undo { tweak::undo(t) } else { tweak::apply(t) };
                if done {
                    ok += 1;
                } else {
                    fail.push(t.name);
                }
            }
            let mut msg = tr!(
                "Windows hızlandırma: {}/{} ayar {}.",
                "Windows speed-up: {}/{} settings {}.",
                ok,
                ids.len(),
                if undo { tr!("geri alındı", "undone") } else { tr!("uygulandı", "applied") }
            );
            if !fail.is_empty() {
                msg += &tr!(
                    " Tamamlanamayanlar (yönetici modu gerekebilir): {}.",
                    " Could not be completed (administrator mode may be required): {}.",
                    fail.join(", ")
                );
            }
            msg
        });
    }

    pub(crate) fn boost_tab(&mut self, ui: &mut egui::Ui, s: &Snapshot) {
        let c = pal(ui);
        let (on, apps, busy, loaded, status) = {
            let g = self.ui4.st.lock().unwrap();
            (g.on.clone(), g.apps.clone(), g.busy, g.loaded, g.status.clone())
        };
        if !loaded && !busy {
            self.boost_job(ui.ctx(), tr!("Ayarlar okunuyor…", "Reading settings…"), true, |_| String::new());
        }
        let is_on = |id: &str| on.get(id).copied().unwrap_or(false);
        let total = self.ui4.list.len();
        let done = self.ui4.list.iter().filter(|t| is_on(t.id)).count();
        let rec_left: Vec<&'static str> = self
            .ui4
            .list
            .iter()
            .filter(|t| t.rec && !is_on(t.id))
            .map(|t| t.id)
            .collect();

        ui.add_space(6.0);
        let full = ui.available_width() - 28.0;
        card(ui).show(ui, |ui| {
            ui.set_width(full);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(tr!("Uygulanan ayar", "Settings applied")).weak());
                    ui.label(
                        RichText::new(if loaded { format!("{done} / {total}") } else { "…".into() })
                            .size(28.0)
                            .strong()
                            .color(if rec_left.is_empty() && loaded { c.green } else { c.blue }),
                    );
                    ui.label(if rec_left.is_empty() {
                        tr!("Önerilen ayarların tümü uygulanmış", "All recommended settings are applied").to_string()
                    } else {
                        tr!("{} önerilen ayar bekliyor", "{} recommended settings pending", rec_left.len())
                    });
                });
                ui.add_space(20.0);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        if self.ui4.confirm_all {
                            ui.colored_label(
                                c.orange,
                                tr!(
                                    "{} ayar uygulanacak. Hepsi tek tek geri alınabilir.",
                                    "{} settings will be applied. Each one can be undone individually.",
                                    rec_left.len()
                                ),
                            );
                            if ui.add(primary(tr!("  Onayla  ", "  Confirm  "))).clicked() {
                                self.ui4.confirm_all = false;
                                self.boost_apply(ui.ctx(), rec_left.clone(), false);
                            }
                            if ui.add(egui::Button::new(tr!("Vazgeç", "Cancel")).min_size(egui::vec2(0.0, 32.0))).clicked() {
                                self.ui4.confirm_all = false;
                            }
                        } else {
                            if ui
                                .add_enabled(
                                    !busy && !rec_left.is_empty(),
                                    primary(tr!("   Önerilen ayarların tümünü uygula   ", "   Apply all recommended settings   "))
                                        .min_size(egui::vec2(260.0, 38.0)),
                                )
                                .on_hover_text(tr!(
                                    "\"Önerilen\" etiketli, günlük kullanımı etkilemeyen ayarları tek seferde uygular",
                                    "Applies in one go the settings labelled \"recommended\", which do not affect everyday use"
                                ))
                                .clicked()
                            {
                                self.ui4.confirm_all = true;
                            }
                            if ui
                                .add_enabled(
                                    !busy,
                                    egui::Button::new(tr!("Geri yükleme noktası oluştur", "Create restore point")).min_size(egui::vec2(0.0, 38.0)),
                                )
                                .on_hover_text(tr!(
                                    "Ayarları uygulamadan önce Windows geri yükleme noktası oluşturur (Sistem Koruması açık olmalıdır)",
                                    "Creates a Windows restore point before you apply settings (System Protection must be turned on)"
                                ))
                                .clicked()
                            {
                                self.boost_job(ui.ctx(), tr!("Geri yükleme noktası oluşturuluyor…", "Creating restore point…"), false, |_| {
                                    if tweak::restore_point() {
                                        tr!("Geri yükleme noktası oluşturuldu: HFix Windows 11.", "Restore point created: HFix Windows 11.").into()
                                    } else {
                                        tr!(
                                            "Geri yükleme noktası oluşturulamadı (Sistem Koruması kapalı olabilir ya da son 24 saatte bir nokta oluşturulmuş).",
                                            "Could not create a restore point (System Protection may be off, or a point was already created in the last 24 hours)."
                                        )
                                        .into()
                                    }
                                });
                            }
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
                            tr!(
                                "Sınırlı mod: sistem geneli ayarlar için yönetici gerekir.",
                                "Limited mode: system-wide settings require administrator rights."
                            ),
                        );
                    } else {
                        ui.label(
                            RichText::new(tr!(
                                "Bazı ayarlar oturumu kapatıp açınca ya da bilgisayarı yeniden başlatınca geçerli olur.",
                                "Some settings take effect after you sign out and back in or restart the computer."
                            ))
                            .weak(),
                        );
                    }
                });
            });
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.ui4.view_apps, false, RichText::new(tr!("  Windows ayarları  ", "  Windows settings  ")).strong());
            ui.selectable_value(
                &mut self.ui4.view_apps,
                true,
                RichText::new(tr!("  Gereksiz uygulamalar ({})  ", "  Unneeded apps ({})  ", apps.len())).strong(),
            );
            ui.label(
                RichText::new(tr!(
                    "Güvenliği azaltan ayarlar (Defender, güncellemeler, işlemci korumaları, UAC) bilerek eklenmedi.",
                    "Settings that reduce security (Defender, updates, CPU mitigations, UAC) are left out on purpose."
                ))
                .weak(),
            );
        });
        if tweak::hidden_count() > 0 {
            ui.label(
                RichText::new(tr!(
                    "Windows 10: yalnızca Windows 11'de bulunan {} ayar gösterilmiyor.",
                    "Windows 10: {} settings that exist only on Windows 11 are not shown.",
                    tweak::hidden_count()
                ))
                .weak(),
            );
        }
        ui.separator();

        if self.ui4.view_apps {
            self.boost_apps(ui, &apps, busy);
            return;
        }

        let mut act: Option<(&'static str, bool)> = None;
        let mut group_apply: Option<Vec<&'static str>> = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            let full = ui.available_width() - 28.0;
            for gi in 0..GROUPS.len() {
                // Baslik ve aciklama gecerli dilde; grup uyeligi indeksle belirlenir.
                let (title, gdesc) = tweak::group(gi);
                let items: Vec<&Tweak> = self.ui4.list.iter().filter(|t| t.group == gi).collect();
                let gdone = items.iter().filter(|t| is_on(t.id)).count();
                let pending: Vec<&'static str> = items.iter().filter(|t| t.rec && !is_on(t.id)).map(|t| t.id).collect();
                card(ui).show(ui, |ui| {
                    ui.set_width(full);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(title).size(17.0).strong());
                        ui.label(
                            RichText::new(tr!("{} / {} uygulanmış", "{} / {} applied", gdone, items.len()))
                                .color(if gdone == items.len() { c.green } else { ui.visuals().weak_text_color() }),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add_enabled(
                                    !busy && !pending.is_empty(),
                                    egui::Button::new(tr!(
                                        "Bu gruptaki önerilenleri uygula ({})",
                                        "Apply recommended in this group ({})",
                                        pending.len()
                                    )),
                                )
                                .clicked()
                            {
                                group_apply = Some(pending.clone());
                            }
                        });
                    });
                    ui.label(RichText::new(gdesc).weak());
                    ui.add_space(4.0);
                    let text_w = (full - 290.0).max(260.0);
                    for t in items {
                        let active = is_on(t.id);
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.allocate_ui_with_layout(
                                egui::vec2(108.0, 38.0),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    if active {
                                        ui.colored_label(c.green, RichText::new(tr!("■ Uygulandı", "■ Applied")).strong());
                                    } else {
                                        ui.label(RichText::new(tr!("○ Varsayılan", "○ Default")).weak());
                                    }
                                },
                            );
                            ui.allocate_ui_with_layout(
                                egui::vec2(text_w, 38.0),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new(t.name).strong());
                                        if t.rec {
                                            ui.colored_label(c.blue, RichText::new(tr!("önerilen", "recommended")).small());
                                        } else {
                                            ui.label(RichText::new(tr!("isteğe bağlı", "optional")).small().weak());
                                        }
                                    });
                                    ui.add(egui::Label::new(RichText::new(t.desc).weak()).truncate())
                                        .on_hover_text(t.desc);
                                },
                            );
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if active {
                                    if ui.add_enabled(!busy, egui::Button::new(tr!("Geri al", "Undo")).min_size(egui::vec2(90.0, 26.0))).clicked() {
                                        act = Some((t.id, true));
                                    }
                                } else if ui
                                    .add_enabled(!busy, egui::Button::new(RichText::new(tr!("Uygula", "Apply")).strong()).min_size(egui::vec2(90.0, 26.0)))
                                    .clicked()
                                {
                                    act = Some((t.id, false));
                                }
                            });
                        });
                    }
                });
                ui.add_space(8.0);
            }
            card(ui).show(ui, |ui| {
                ui.set_width(full);
                ui.label(RichText::new(tr!("Diğer hızlandırma adımları", "Other speed-up steps")).size(17.0).strong());
                ui.label(RichText::new(tr!("Bu işlemler kendi sayfalarında yapılır.", "These are done on their own pages.")).weak());
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    for (label, tab) in [
                        (tr!("İşlemci güç modunu seç", "Choose the CPU power mode"), crate::Tab::Power),
                        (tr!("Başlangıçta açılanları azalt", "Reduce startup programs"), crate::Tab::Startup),
                        (tr!("Gereksiz servisleri durdur", "Stop unneeded services"), crate::Tab::Services),
                        (tr!("Diski temizle", "Clean up the disk"), crate::Tab::Disk),
                        (tr!("Belleği temizle", "Free up memory"), crate::Tab::Mem),
                    ] {
                        if ui.add(egui::Button::new(label).min_size(egui::vec2(0.0, 30.0))).clicked() {
                            self.tab = tab;
                        }
                    }
                });
            });
            ui.add_space(10.0);
        });
        if let Some((id, undo)) = act {
            self.boost_apply(ui.ctx(), vec![id], undo);
        }
        if let Some(ids) = group_apply {
            self.boost_apply(ui.ctx(), ids, false);
        }
    }

    fn boost_apps(&mut self, ui: &mut egui::Ui, apps: &[StoreApp], busy: bool) {
        let c = pal(ui);
        self.ui4.app_sel.retain(|f| apps.iter().any(|a| &a.full == f));
        let n = self.ui4.app_sel.len();
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(!busy, egui::Button::new(tr!("Önerilenleri seç", "Select recommended"))).clicked() {
                self.ui4.app_sel = apps.iter().filter(|a| a.rec).map(|a| a.full.clone()).collect();
                self.ui4.confirm_apps = false;
            }
            if self.ui4.confirm_apps {
                ui.colored_label(
                    c.red,
                    tr!(
                        "{} uygulama kaldırılacak. Geri almak için Microsoft Store'dan yeniden kurmanız gerekir.",
                        "{} apps will be removed. To get them back you will need to reinstall them from the Microsoft Store.",
                        n
                    ),
                );
                if ui.button(RichText::new(tr!("Evet, kaldır", "Yes, remove")).color(c.red)).clicked() {
                    self.ui4.confirm_apps = false;
                    let targets: Vec<StoreApp> = apps.iter().filter(|a| self.ui4.app_sel.contains(&a.full)).cloned().collect();
                    self.ui4.app_sel.clear();
                    self.boost_job(ui.ctx(), tr!("Uygulamalar kaldırılıyor…", "Removing apps…"), true, move |st| {
                        let (mut ok, mut fail) = (Vec::new(), Vec::new());
                        for (i, a) in targets.iter().enumerate() {
                            if let Ok(mut g) = st.lock() {
                                g.status = tr!("{}/{}: {} kaldırılıyor…", "{}/{}: removing {}…", i + 1, targets.len(), a.label);
                            }
                            if tweak::remove_app(a) {
                                ok.push(a.label);
                            } else {
                                fail.push(a.label);
                            }
                        }
                        let mut msg = tr!(
                            "Gereksiz uygulamalar: {} kaldırıldı ({}).",
                            "Unneeded apps: {} removed ({}).",
                            ok.len(),
                            ok.join(", ")
                        );
                        if !fail.is_empty() {
                            msg += &tr!(" Kaldırılamayanlar: {}.", " Could not be removed: {}.", fail.join(", "));
                        }
                        msg
                    });
                }
                if ui.button(tr!("Vazgeç", "Cancel")).clicked() {
                    self.ui4.confirm_apps = false;
                }
            } else {
                if ui.add_enabled(!busy && n > 0, primary(&tr!("  Seçilen {} uygulamayı kaldır  ", "  Remove {} selected apps  ", n))).clicked() {
                    self.ui4.confirm_apps = true;
                }
                if n > 0 && ui.button(tr!("Seçimi temizle", "Clear selection")).clicked() {
                    self.ui4.app_sel.clear();
                }
                if ui.add_enabled(!busy, egui::Button::new(tr!("Yenile", "Refresh"))).clicked() {
                    self.boost_job(ui.ctx(), tr!("Uygulamalar okunuyor…", "Reading apps…"), true, |_| String::new());
                }
            }
        });
        ui.label(
            RichText::new(tr!(
                "Yalnızca Windows'la birlikte gelen ya da üretici tarafından önceden kurulmuş, kaldırılması sorun çıkarmayan uygulamalar listelenir. \
                 Microsoft Store, Fotoğraflar, Hesap Makinesi, Not Defteri, Paint ve Terminal listede yer almaz. \"İsteğe bağlı\" olanları kullanmıyorsanız kaldırın.",
                "Only apps that ship with Windows or were preinstalled by the manufacturer and are safe to remove are listed. \
                 Microsoft Store, Photos, Calculator, Notepad, Paint and Terminal are not in the list. Remove the \"optional\" ones if you do not use them."
            ))
            .weak(),
        );
        ui.add_space(4.0);
        if apps.is_empty() {
            ui.colored_label(
                c.green,
                tr!(
                    "✔ Bu bilgisayarda kaldırılacak gereksiz uygulama bulunamadı.",
                    "✔ No unneeded apps to remove were found on this computer."
                ),
            );
            return;
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("bloat").num_columns(4).striped(true).spacing([16.0, 8.0]).show(ui, |ui| {
                for a in apps {
                    let mut on = self.ui4.app_sel.contains(&a.full);
                    if ui.add_enabled(!busy, egui::Checkbox::new(&mut on, "")).changed() {
                        if on {
                            self.ui4.app_sel.insert(a.full.clone());
                        } else {
                            self.ui4.app_sel.remove(&a.full);
                        }
                        self.ui4.confirm_apps = false;
                    }
                    ui.label(RichText::new(a.label).strong());
                    if a.rec {
                        ui.colored_label(c.blue, tr!("kaldırılması önerilir", "removal recommended"));
                    } else {
                        ui.label(RichText::new(tr!("isteğe bağlı", "optional")).weak());
                    }
                    ui.label(RichText::new(&a.name).weak());
                    ui.end_row();
                }
            });
        });
    }
}
