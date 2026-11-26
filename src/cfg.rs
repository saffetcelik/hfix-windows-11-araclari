//! Ayarlarin program klasorundeki ayarlar.txt dosyasinda saklanmasi.

use crate::mon::AutoCfg;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq)]
pub enum Theme {
    /// Windows'un acik / koyu ayarini izler.
    System,
    Light,
    Dark,
}

#[derive(Clone)]
pub struct Cfg {
    pub theme: Theme,
    /// "oto", "tr" ya da "en".
    pub lang: String,
    /// Yeni surumler kendiliginden indirilsin mi?
    pub auto_update: bool,
    pub group: bool,
    pub auto: AutoCfg,
    /// Her zaman kisilacak surec adlari.
    pub rules: Vec<String>,
    /// Disk temizliginde bugune kadar kazanilan toplam bayt.
    pub cleaned: u64,
}

impl Default for Cfg {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            lang: "oto".into(),
            auto_update: true,
            group: true,
            auto: AutoCfg::default(),
            rules: Vec::new(),
            cleaned: 0,
        }
    }
}

/// Programin kendi dosyalarini (ayarlar, yedekler, karantina) tuttugu klasor: exe'nin yani.
pub fn dir() -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.to_path_buf())
}

fn path() -> Option<PathBuf> {
    Some(dir()?.join("ayarlar.txt"))
}

pub fn load() -> Cfg {
    let mut c = Cfg::default();
    let Some(text) = path().and_then(|p| std::fs::read_to_string(p).ok()) else {
        return c;
    };
    let mut theme_set = false;
    for line in text.lines() {
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let v = v.trim();
        let b = v == "1";
        match k.trim() {
            "tema" => {
                theme_set = true;
                c.theme = match v {
                    "acik" => Theme::Light,
                    "koyu" => Theme::Dark,
                    _ => Theme::System,
                }
            }
            // Eski surum: yalnizca acik/koyu vardi ve varsayilan koyuydu. Acik secilmisse bu
            // kullanicinin tercihidir; koyu ise bilemeyiz, sistemi izleriz.
            "acik_tema" if !theme_set && b => c.theme = Theme::Light,
            "dil" if v == "tr" || v == "en" => c.lang = v.to_string(),
            "oto_guncelle" => c.auto_update = b,
            "grupla" => c.group = b,
            "oto_ram" => c.auto.ram_on = b,
            "oto_ram_yuzde" => c.auto.ram_pct = v.parse().unwrap_or(85).clamp(50, 95),
            "oto_kis" => c.auto.thr_on = b,
            "oto_kis_yuzde" => c.auto.thr_pct = v.parse().unwrap_or(25).clamp(5, 90),
            "hep_kis" => {
                c.rules = v
                    .split('|')
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .collect()
            }
            "temizlenen" => c.cleaned = v.parse().unwrap_or(0),
            _ => {}
        }
    }
    c
}

pub fn save(c: &Cfg) {
    let Some(p) = path() else {
        return;
    };
    let text = format!(
        "tema={}\ndil={}\noto_guncelle={}\ngrupla={}\noto_ram={}\noto_ram_yuzde={}\noto_kis={}\noto_kis_yuzde={}\nhep_kis={}\ntemizlenen={}\n",
        match c.theme {
            Theme::System => "sistem",
            Theme::Light => "acik",
            Theme::Dark => "koyu",
        },
        c.lang,
        c.auto_update as u8,
        c.group as u8,
        c.auto.ram_on as u8,
        c.auto.ram_pct,
        c.auto.thr_on as u8,
        c.auto.thr_pct,
        c.rules.join("|"),
        c.cleaned
    );
    let _ = std::fs::write(p, text);
}
