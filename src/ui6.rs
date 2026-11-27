//! Onarim ve Ag sayfasi: sistem dosyasi onarimi, ag sifirlama, DNS secimi ve kucuk bakim araclari.
//! Komutlar ve DNS adresleri ChrisTitusTech/winutil ile hellzerg/optimizer projelerindekilerle aynidir.

use crate::lang::t;
use crate::mon::{Cmd, Snapshot};
use crate::{card, pal, primary, tr, win, App};
use eframe::egui::{self, RichText};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Inner {
    busy: bool,
    /// Fiziksel bagdastiricilarin su anki DNS sunuculari ("Wi-Fi: 1.1.1.1, 1.0.0.1").
    dns: String,
    /// Son islemin sonucu: (basarili mi, metin).
    note: Option<(bool, String)>,
}

#[derive(Default)]
pub struct State {
    st: Arc<Mutex<Inner>>,
    asked: bool,
    /// Ag sifirlama icin onay bekleniyor mu?
    confirm_net: bool,
    pick: usize,
}

/// (ad TR, ad EN, aciklama TR, aciklama EN, IPv4 ve IPv6 adresleri)
const DNS: [(&str, &str, &str, &str, &[&str]); 7] = [
    ("Otomatik (modem / servis sağlayıcı)", "Automatic (router / provider)", "Windows'un varsayılanı.", "The Windows default.", &[]),
    ("Cloudflare", "Cloudflare", "Hızlı, filtresiz.", "Fast, unfiltered.", &["1.1.1.1", "1.0.0.1", "2606:4700:4700::1111", "2606:4700:4700::1001"]),
    ("Cloudflare · zararlı site engelleme", "Cloudflare · malware blocking", "Bilinen zararlı siteleri engeller.", "Blocks known malicious sites.", &["1.1.1.2", "1.0.0.2", "2606:4700:4700::1112", "2606:4700:4700::1002"]),
    ("Google", "Google", "Yaygın, filtresiz.", "Widely used, unfiltered.", &["8.8.8.8", "8.8.4.4", "2001:4860:4860::8888", "2001:4860:4860::8844"]),
    ("Quad9", "Quad9", "Zararlı siteleri engeller, gizlilik odaklı.", "Blocks malicious sites, privacy-focused.", &["9.9.9.9", "149.112.112.112", "2620:fe::fe", "2620:fe::9"]),
    ("AdGuard · reklam ve izleyici engelleme", "AdGuard · ad and tracker blocking", "Reklam ve izleyici alan adlarını engeller.", "Blocks ad and tracker domains.", &["94.140.14.14", "94.140.15.15", "2a10:50c0::ad1:ff", "2a10:50c0::ad2:ff"]),
    ("OpenDNS", "OpenDNS", "Cisco'nun genel DNS'i.", "Cisco's public DNS.", &["208.67.222.222", "208.67.220.220", "2620:119:35::35", "2620:119:53::53"]),
];

/// Yalnizca fiziksel ve bagli bagdastiricilar: sanal makine aglarina dokunulmaz.
const ADAPTERS: &str = "Get-NetAdapter -Physical | Where-Object Status -eq 'Up'";

fn ps(script: &str) -> (bool, String) {
    match win::run_code("powershell", &["-NoProfile", "-NonInteractive", "-Command", script]) {
        Some((code, out)) => (code == 0, out.trim().to_string()),
        None => (false, String::new()),
    }
}

fn read_dns() -> String {
    ps(&format!(
        "({ADAPTERS} | ForEach-Object {{ $n=$_.Name; $d=(Get-DnsClientServerAddress -InterfaceIndex $_.ifIndex -AddressFamily IPv4).ServerAddresses -join ', '; \"$n\"+': '+$d }}) -join \"`n\""
    ))
    .1
}

/// Komutu gorunur bir konsol penceresinde calistirir; pencere is bitince acik kalir.
fn console(title: &str, cmd: &str) {
    win::shell_open("cmd.exe", &format!("/k title {title} & {cmd}"));
}

impl App {
    fn repair_job(&self, ctx: &egui::Context, f: impl FnOnce() -> (bool, String) + Send + 'static) {
        let (st, ctx) = (self.ui6.st.clone(), ctx.clone());
        if let Ok(mut s) = st.lock() {
            if s.busy {
                return;
            }
            s.busy = true;
        }
        std::thread::spawn(move || {
            let note = f();
            let dns = read_dns();
            if let Ok(mut s) = st.lock() {
                s.busy = false;
                s.dns = dns;
                s.note = Some(note);
            }
            ctx.request_repaint();
        });
    }

    pub(crate) fn repair_tab(&mut self, ui: &mut egui::Ui, s: &Snapshot) {
        let c = pal(ui);
        if !self.ui6.asked {
            self.ui6.asked = true;
            let (st, ctx) = (self.ui6.st.clone(), ui.ctx().clone());
            std::thread::spawn(move || {
                let dns = read_dns();
                if let Ok(mut g) = st.lock() {
                    g.dns = dns;
                }
                ctx.request_repaint();
            });
        }
        let (busy, dns, note) = {
            let g = self.ui6.st.lock().unwrap();
            (g.busy, g.dns.clone(), g.note.clone())
        };
        let admin = s.admin;

        egui::ScrollArea::vertical().show(ui, |ui| {
            let full = ui.available_width() - 28.0;
            ui.add_space(6.0);
            if !admin {
                ui.colored_label(
                    c.orange,
                    t(
                        "Sınırlı mod: bu sayfadaki işlemler için programı yönetici olarak açın.",
                        "Limited mode: open the program as administrator to use the actions on this page.",
                    ),
                );
                ui.add_space(4.0);
            }
            if busy {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(t("İşlem sürüyor…", "Working…"));
                });
            } else if let Some((ok, text)) = &note {
                ui.colored_label(if *ok { c.green } else { c.orange }, text);
            }

            // ---- sistem dosyalari
            card(ui).show(ui, |ui| {
                ui.set_width(full);
                ui.label(RichText::new(t("Windows'u onar", "Repair Windows")).size(16.0).strong());
                ui.add(
                    egui::Label::new(
                        RichText::new(t(
                            "Bozuk sistem dosyaları mavi ekrana, açılmayan ayarlara ve başarısız güncellemelere yol açar. Araçlar ayrı bir komut penceresinde çalışır; ilerlemeyi oradan izlersiniz. Tam onarım 10-30 dakika sürebilir.",
                            "Corrupt system files cause blue screens, settings that will not open and failed updates. The tools run in a separate command window where you can follow the progress. A full repair can take 10-30 minutes.",
                        ))
                        .weak(),
                    )
                    .wrap(),
                );
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add_enabled(admin, primary(t("Tam onarım (disk + SFC + DISM)", "Full repair (disk + SFC + DISM)")))
                        .on_hover_text(t(
                            "Sırayla: disk denetimi, sistem dosyası denetimi, Windows görüntüsünün onarımı.",
                            "In order: disk check, system file check, repair of the Windows image.",
                        ))
                        .clicked()
                    {
                        console(
                            "HFix",
                            "chkdsk /scan /perf & sfc /scannow & dism /online /cleanup-image /restorehealth",
                        );
                        self.send(Cmd::Note(t("Onarım: tam onarım başlatıldı.", "Repair: full repair started.").into()));
                    }
                    if ui
                        .add_enabled(admin, egui::Button::new(t("Sistem dosyalarını denetle (SFC)", "Check system files (SFC)")).min_size(egui::vec2(0.0, 32.0)))
                        .clicked()
                    {
                        console("HFix SFC", "sfc /scannow");
                    }
                    if ui
                        .add_enabled(admin, egui::Button::new(t("Windows görüntüsünü onar (DISM)", "Repair the Windows image (DISM)")).min_size(egui::vec2(0.0, 32.0)))
                        .clicked()
                    {
                        console("HFix DISM", "dism /online /cleanup-image /restorehealth");
                    }
                    if ui
                        .add_enabled(admin, egui::Button::new(t("Diski denetle", "Check the disk")).min_size(egui::vec2(0.0, 32.0)))
                        .clicked()
                    {
                        console("HFix chkdsk", "chkdsk /scan /perf");
                    }
                    if ui
                        .add_enabled(admin, egui::Button::new(t("Güncelleme artıklarını temizle", "Clean up update leftovers")).min_size(egui::vec2(0.0, 32.0)))
                        .on_hover_text(t(
                            "Bileşen deposundaki eski güncelleme dosyalarını siler (DISM StartComponentCleanup).",
                            "Deletes old update files from the component store (DISM StartComponentCleanup).",
                        ))
                        .clicked()
                    {
                        console("HFix DISM", "dism /online /cleanup-image /startcomponentcleanup");
                    }
                });
            });
            ui.add_space(8.0);

            // ---- DNS
            card(ui).show(ui, |ui| {
                ui.set_width(full);
                ui.label(RichText::new(t("DNS sunucusu", "DNS server")).size(16.0).strong());
                ui.add(
                    egui::Label::new(
                        RichText::new(t(
                            "DNS, site adlarını adrese çevirir. Daha hızlı ya da zararlı siteleri engelleyen bir sunucu seçebilirsiniz. Yalnızca bağlı fiziksel bağdaştırıcılar (Wi-Fi, Ethernet) değişir; sanal makine ağlarına dokunulmaz.",
                            "DNS turns site names into addresses. You can pick a server that is faster or blocks malicious sites. Only connected physical adapters (Wi-Fi, Ethernet) are changed; virtual machine networks are not touched.",
                        ))
                        .weak(),
                    )
                    .wrap(),
                );
                ui.add_space(4.0);
                ui.label(RichText::new(t("Şu an:", "Now:")).strong());
                if dns.is_empty() {
                    ui.label(RichText::new(t("okunuyor…", "reading…")).weak());
                } else {
                    for line in dns.lines() {
                        ui.label(line);
                    }
                }
                ui.add_space(6.0);
                for (i, (tr_name, en_name, tr_desc, en_desc, addrs)) in DNS.iter().enumerate() {
                    ui.horizontal_wrapped(|ui| {
                        ui.radio_value(&mut self.ui6.pick, i, crate::lang::t(tr_name, en_name));
                        let v4: Vec<&str> = addrs.iter().copied().filter(|a| !a.contains(':')).collect();
                        ui.label(RichText::new(crate::lang::t(tr_desc, en_desc)).weak());
                        if !v4.is_empty() {
                            ui.label(RichText::new(v4.join(" · ")).weak());
                        }
                    });
                }
                ui.add_space(6.0);
                if ui
                    .add_enabled(admin && !busy, primary(t("Seçilen DNS'i uygula", "Apply the selected DNS")))
                    .clicked()
                {
                    let (name, addrs) = (crate::lang::t(DNS[self.ui6.pick].0, DNS[self.ui6.pick].1), DNS[self.ui6.pick].4);
                    let script = if addrs.is_empty() {
                        format!("{ADAPTERS} | ForEach-Object {{ Set-DnsClientServerAddress -InterfaceIndex $_.ifIndex -ResetServerAddresses -ErrorAction Stop }}; Clear-DnsClientCache")
                    } else {
                        let list: Vec<String> = addrs.iter().map(|a| format!("'{a}'")).collect();
                        format!(
                            "{ADAPTERS} | ForEach-Object {{ Set-DnsClientServerAddress -InterfaceIndex $_.ifIndex -ServerAddresses ({}) -ErrorAction Stop }}; Clear-DnsClientCache",
                            list.join(",")
                        )
                    };
                    let tx = self.tx.clone();
                    self.repair_job(ui.ctx(), move || {
                        let ok = ps(&script).0;
                        let text = if ok {
                            tr!("DNS ayarlandı: {}.", "DNS set: {}.", name)
                        } else {
                            tr!("DNS değiştirilemedi: {}.", "The DNS could not be changed: {}.", name)
                        };
                        let _ = tx.send(Cmd::Note(text.clone()));
                        (ok, text)
                    });
                }
            });
            ui.add_space(8.0);

            // ---- ag ve diger
            card(ui).show(ui, |ui| {
                ui.set_width(full);
                ui.label(RichText::new(t("Ağ ve hızlı düzeltmeler", "Network and quick fixes")).size(16.0).strong());
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .button(t("DNS önbelleğini temizle", "Flush DNS cache"))
                        .on_hover_text(t("Açılmayan ya da yanlış adrese giden siteler için.", "For sites that will not open or go to the wrong address."))
                        .clicked()
                    {
                        self.send(Cmd::FlushDns);
                    }
                    if ui
                        .add_enabled(admin && !busy, egui::Button::new(t("Saati internetten eşitle", "Sync the clock from the internet")))
                        .clicked()
                    {
                        self.repair_job(ui.ctx(), || {
                            let ok = ps("Start-Service w32time -ErrorAction SilentlyContinue; w32tm /resync /force | Out-Null; exit $LASTEXITCODE").0;
                            (
                                ok,
                                if ok {
                                    t("Saat eşitlendi.", "The clock was synchronized.").to_string()
                                } else {
                                    t("Saat eşitlenemedi.", "The clock could not be synchronized.").to_string()
                                },
                            )
                        });
                    }
                    if ui
                        .button(t("Gezgin'i yeniden başlat", "Restart File Explorer"))
                        .on_hover_text(t(
                            "Görev çubuğu ve masaüstü kapanıp yeniden açılır. Donan görev çubuğu ya da uygulanmayan görünüm ayarları için.",
                            "The taskbar and desktop close and reopen. For a frozen taskbar or appearance settings that did not take effect.",
                        ))
                        .clicked()
                    {
                        win::run_status("taskkill", &["/f", "/im", "explorer.exe"]);
                        win::shell_open("explorer.exe", "");
                    }
                    if self.ui6.confirm_net {
                        ui.colored_label(
                            c.red,
                            t(
                                "Kayıtlı sabit IP ve proxy ayarları silinir, yeniden başlatma gerekir. Emin misiniz?",
                                "Saved static IP and proxy settings are removed and a restart is required. Are you sure?",
                            ),
                        );
                        if ui.button(RichText::new(t("Evet, sıfırla", "Yes, reset")).color(c.red)).clicked() {
                            self.ui6.confirm_net = false;
                            let tx = self.tx.clone();
                            self.repair_job(ui.ctx(), move || {
                                let a = win::run_status("netsh", &["winsock", "reset"]);
                                win::run_status("netsh", &["int", "ip", "reset"]);
                                let text = if a {
                                    t(
                                        "Ağ ayarları sıfırlandı. Bilgisayarı yeniden başlatın.",
                                        "Network settings were reset. Restart the computer.",
                                    )
                                } else {
                                    t("Ağ ayarları sıfırlanamadı.", "The network settings could not be reset.")
                                }
                                .to_string();
                                let _ = tx.send(Cmd::Note(text.clone()));
                                (a, text)
                            });
                        }
                        if ui.button(t("Vazgeç", "Cancel")).clicked() {
                            self.ui6.confirm_net = false;
                        }
                    } else if ui
                        .add_enabled(admin && !busy, egui::Button::new(t("Ağ ayarlarını sıfırla", "Reset network settings")))
                        .on_hover_text(t(
                            "İnternet bağlı göründüğü halde çalışmıyorsa: Winsock ve TCP/IP yığınını sıfırlar.",
                            "When the connection looks fine but nothing loads: resets Winsock and the TCP/IP stack.",
                        ))
                        .clicked()
                    {
                        self.ui6.confirm_net = true;
                    }
                });
            });
            ui.add_space(10.0);
        });
    }
}
