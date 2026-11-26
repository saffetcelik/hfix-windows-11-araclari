//! Ayarlar ve Hakkinda sayfalari.

use crate::cfg::{self, Theme};
use crate::lang::{self, t};
use crate::update::{self, Phase};
use crate::{card, install, pal, primary, tr, win, App};
use eframe::egui::{self, RichText};

#[derive(Default)]
pub struct State {
    /// "Kisayol olusturuldu" gibi kisa bilgi satiri.
    note: String,
}

fn section(ui: &mut egui::Ui, title: &str, hint: &str) {
    ui.label(RichText::new(title).size(16.0).strong());
    if !hint.is_empty() {
        ui.label(RichText::new(hint).weak());
    }
    ui.add_space(6.0);
}

impl App {
    pub fn settings_tab(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.set_max_width(760.0);

            card(ui).show(ui, |ui| {
                ui.set_width(ui.available_width());
                section(
                    ui,
                    t("Görünüm", "Appearance"),
                    t(
                        "\"Sistem\" seçiliyken program Windows'un açık / koyu ayarını izler.",
                        "With \"System\" the program follows the light / dark setting of Windows.",
                    ),
                );
                ui.horizontal(|ui| {
                    ui.label(t("Tema:", "Theme:"));
                    let mut theme = self.cfg.theme;
                    ui.selectable_value(&mut theme, Theme::System, t("Sistem", "System"));
                    ui.selectable_value(&mut theme, Theme::Light, t("Açık", "Light"));
                    ui.selectable_value(&mut theme, Theme::Dark, t("Koyu", "Dark"));
                    if theme != self.cfg.theme {
                        self.cfg.theme = theme;
                        cfg::save(&self.cfg);
                        crate::apply_theme(ui.ctx(), theme);
                    }
                });
            });
            ui.add_space(10.0);

            card(ui).show(ui, |ui| {
                ui.set_width(ui.available_width());
                section(
                    ui,
                    t("Dil", "Language"),
                    t(
                        "\"Otomatik\" seçiliyken Windows Türkçe ise Türkçe, değilse İngilizce açılır.",
                        "With \"Automatic\" the program opens in Turkish if Windows is in Turkish, otherwise in English.",
                    ),
                );
                ui.horizontal(|ui| {
                    ui.label(t("Dil:", "Language:"));
                    let mut l = self.cfg.lang.clone();
                    ui.selectable_value(&mut l, "oto".to_string(), t("Otomatik", "Automatic"));
                    ui.selectable_value(&mut l, "tr".to_string(), "Türkçe");
                    ui.selectable_value(&mut l, "en".to_string(), "English");
                    if l != self.cfg.lang {
                        self.cfg.lang = l;
                        cfg::save(&self.cfg);
                        lang::set(&self.cfg.lang);
                    }
                });
            });
            ui.add_space(10.0);

            card(ui).show(ui, |ui| {
                ui.set_width(ui.available_width());
                section(ui, t("Güncelleme", "Updates"), "");
                if ui
                    .checkbox(
                        &mut self.cfg.auto_update,
                        t("Yeni sürümleri otomatik indir", "Download new versions automatically"),
                    )
                    .on_hover_text(t(
                        "Program GitHub'daki sürümleri denetler; imzası doğrulanan yeni sürümü indirir ve yeniden başlatınca kurar.",
                        "The program checks the releases on GitHub, downloads a new version whose signature is valid and installs it on restart.",
                    ))
                    .changed()
                {
                    cfg::save(&self.cfg);
                    update::set_auto(self.cfg.auto_update);
                }
                ui.label(
                    RichText::new(t(
                        "Sürüm bilgisi ve elle güncelleme Hakkında sayfasındadır.",
                        "Version information and manual updating are on the About page.",
                    ))
                    .weak(),
                );
            });
            ui.add_space(10.0);

            card(ui).show(ui, |ui| {
                ui.set_width(ui.available_width());
                section(ui, t("Kısayol ve program klasörü", "Shortcut and program folder"), "");
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add_enabled(
                            install::is_installed_copy(),
                            egui::Button::new(t("Masaüstü kısayolunu yeniden oluştur", "Recreate the desktop shortcut")),
                        )
                        .on_disabled_hover_text(t(
                            "Bu kopya kurulu değil (taşınabilir modda çalışıyor).",
                            "This copy is not installed (running in portable mode).",
                        ))
                        .clicked()
                    {
                        self.ui5.note = if install::create_shortcuts() > 0 {
                            t("Kısayollar oluşturuldu.", "Shortcuts created.").to_string()
                        } else {
                            t("Kısayol oluşturulamadı.", "The shortcut could not be created.").to_string()
                        };
                    }
                    if ui.button(t("Program klasörünü aç", "Open the program folder")).clicked() {
                        if let Some(d) = cfg::dir() {
                            win::shell_open(&d.to_string_lossy(), "");
                        }
                    }
                });
                if !self.ui5.note.is_empty() {
                    ui.label(&self.ui5.note);
                }
                ui.label(
                    RichText::new(t(
                        "Ayarlar, yedekler ve karantina bu klasörde durur. \"Windows ile başlat\" seçeneği Güç Modu sayfasındadır.",
                        "Settings, backups and the quarantine are kept in this folder. The \"Start with Windows\" option is on the Power Mode page.",
                    ))
                    .weak(),
                );
            });
        });
    }

    pub fn about_tab(&mut self, ui: &mut egui::Ui) {
        let c = pal(ui);
        let info = update::info();
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.set_max_width(760.0);

            card(ui).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(lang::app_name()).size(22.0).strong());
                ui.label(tr!("Sürüm {}", "Version {}", update::VERSION));
                ui.add_space(4.0);
                ui.label(t(
                    "Windows 11 için ücretsiz, açık kaynak bakım ve hızlandırma aracı: süreç ve ağ izleme, RAM ve disk temizliği, güç modları, başlangıç ve servis yönetimi, Windows ayarlarıyla hızlandırma ve risk taraması.",
                    "A free, open-source maintenance and speed-up tool for Windows 11: process and network monitoring, RAM and disk cleaning, power modes, startup and service management, Windows tweaks and a risk scan.",
                ));
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    if ui.button(t("GitHub sayfası", "GitHub page")).clicked() {
                        win::shell_open(&update::project_page(), "");
                    }
                    if ui.button(t("Sürüm notları", "Release notes")).clicked() {
                        win::shell_open(&format!("{}/releases", update::project_page()), "");
                    }
                    if ui.button(t("Sorun bildir", "Report a problem")).clicked() {
                        win::shell_open(&format!("{}/issues", update::project_page()), "");
                    }
                });
                ui.add_space(4.0);
                ui.label(RichText::new(t("Lisans: MIT · HukukFix", "License: MIT · HukukFix")).weak());
            });
            ui.add_space(10.0);

            card(ui).show(ui, |ui| {
                ui.set_width(ui.available_width());
                section(ui, t("Güncelleme", "Updates"), "");
                let busy = matches!(info.phase, Phase::Checking | Phase::Downloading);
                match info.phase {
                    Phase::Idle => {
                        ui.label(t("Henüz denetlenmedi.", "Not checked yet."));
                    }
                    Phase::Checking => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(t("Güncellemeler denetleniyor…", "Checking for updates…"));
                        });
                    }
                    Phase::Downloading => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(tr!("Sürüm {} indiriliyor…", "Downloading version {}…", info.version));
                        });
                    }
                    Phase::Latest => {
                        ui.colored_label(c.green, t("Program güncel.", "The program is up to date."));
                    }
                    Phase::Ready => {
                        ui.colored_label(
                            c.green,
                            tr!(
                                "Sürüm {} indirildi ve doğrulandı. Yeniden başlatınca kurulur.",
                                "Version {} has been downloaded and verified. It is installed on restart.",
                                info.version
                            ),
                        );
                    }
                    Phase::Error => {
                        ui.colored_label(c.orange, error_text(info.error));
                    }
                }
                if !info.checked.is_empty() && !busy {
                    ui.label(RichText::new(tr!("Son denetim: {}", "Last check: {}", info.checked)).weak());
                }
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    if info.phase == Phase::Ready {
                        if ui
                            .add(primary(t("Yeniden başlat ve kur", "Restart and install")))
                            .clicked()
                        {
                            update::apply_and_restart();
                        }
                        if !info.notes.is_empty() && ui.button(t("Yenilikler", "What's new")).clicked() {
                            win::shell_open(&info.notes, "");
                        }
                    }
                    if ui
                        .add_enabled(!busy, egui::Button::new(t("Güncellemeleri denetle", "Check for updates")))
                        .clicked()
                    {
                        update::check_now();
                    }
                });
                ui.add_space(4.0);
                if ui
                    .checkbox(
                        &mut self.cfg.auto_update,
                        t("Yeni sürümleri otomatik indir", "Download new versions automatically"),
                    )
                    .changed()
                {
                    cfg::save(&self.cfg);
                    update::set_auto(self.cfg.auto_update);
                }
                ui.label(
                    RichText::new(t(
                        "Güncellemeler GitHub'daki sürümlerden gelir. Her sürüm, açık yarısı programa gömülü bir anahtarla imzalanır; imzası, sürümü ve SHA-256 özeti tutmayan dosya kurulmaz.",
                        "Updates come from the releases on GitHub. Every release is signed with a key whose public half is built into the program; a file whose signature, version and SHA-256 digest do not match is never installed.",
                    ))
                    .weak(),
                );
            });
        });
    }
}

fn error_text(code: &str) -> &'static str {
    match code {
        "network" => t(
            "GitHub'a ulaşılamadı. İnternet bağlantınızı denetleyip yeniden deneyin.",
            "GitHub could not be reached. Check your internet connection and try again.",
        ),
        "signature" => t(
            "Yeni sürümün imzası doğrulanamadı; kurulmadı.",
            "The signature of the new version could not be verified; it was not installed.",
        ),
        "mismatch" => t(
            "İndirilen dosya imzalanan dosyayla aynı değil; kurulmadı.",
            "The downloaded file is not the signed one; it was not installed.",
        ),
        "no_asset" => t(
            "Yeni sürümün dosyaları henüz yayımlanmamış. Daha sonra yeniden deneyin.",
            "The files of the new version are not published yet. Try again later.",
        ),
        "disk" => t(
            "Güncelleme diske yazılamadı.",
            "The update could not be written to disk.",
        ),
        _ => t("Güncelleme denetlenemedi.", "The update check failed."),
    }
}
