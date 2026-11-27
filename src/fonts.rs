//! Latin disi yazilar icin Windows'un kendi yazi tiplerini egui'ye yedek olarak ekler.
//!
//! Segoe UI (Arapca, Kiril, Yunan) kucuktur ve hep yuklenir. Microsoft YaHei (Cince ve Japonca) ve
//! Malgun Gothic (Korece) 13-19 MB oldugundan yalnizca o dil secildiginde ya da dil listesinin gorundugu Ayarlar sayfasi
//! acildiginda yuklenir. Yuklenen yazi tipi oturum boyunca bellekte kalir.

use eframe::egui;
use std::sync::Mutex;

const SEGOE: u8 = 1;
const ZH: u8 = 2;
const KO: u8 = 8;
const TW: u8 = 16;
/// Dil listesindeki adlar icin yeterli olanlar (繁體 karakterleri YaHei'de de var).
pub const ALL: u8 = SEGOE | ZH | KO;

/// (bit, egui adi, dosya, koleksiyondaki sira)
const FILES: [(u8, &str, &str, u32); 4] = [
    (SEGOE, "segoe", "segoeui.ttf", 0),
    (ZH, "yahei", "msyh.ttc", 0),
    (KO, "malgun", "malgun.ttf", 0),
    (TW, "jhenghei", "msjh.ttc", 0),
];

struct State {
    /// Yazi tiplerine eklenmis olanlar.
    applied: u8,
    /// Okunmus dosyalar (bir kez okunur).
    data: Vec<(u8, &'static [u8])>,
    /// Segoe UI ana yazi tipi mi (Vietnamca: varsayilan yazi tipinde Vietnam harfleri yok)?
    segoe_first: bool,
}

static STATE: Mutex<State> = Mutex::new(State { applied: 0, data: Vec::new(), segoe_first: false });

/// Gecerli dilin gerektirdigi yazi tipleri.
fn needed_for_lang() -> u8 {
    SEGOE
        | match crate::lang::current().code {
            "zh" => ZH,
            // Yu Gothic egui'de Latin harflerle ayni satir hizasinda durmuyor; Microsoft YaHei kana da
            // icerir ve hizasi dogrudur.
            "ja" => ZH,
            "ko" => KO,
            "zh-tw" => TW | ZH,
            _ => 0,
        }
}

/// Her karede cagrilabilir: eksik yazi tipi yoksa hicbir sey yapmaz. `extra` ek olarak
/// istenenler (dil listesi icin ALL).
pub fn ensure(ctx: &egui::Context, extra: u8) {
    let want = needed_for_lang() | extra;
    let Ok(mut st) = STATE.lock() else {
        return;
    };
    let segoe_first = crate::lang::current().code == "vi";
    if want & !st.applied == 0 && segoe_first == st.segoe_first {
        return;
    }
    st.segoe_first = segoe_first;
    let dir = std::path::PathBuf::from(std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into())).join("Fonts");
    for (bit, _, file, _) in FILES {
        if want & bit != 0 && !st.data.iter().any(|d| d.0 == bit) {
            if let Ok(bytes) = std::fs::read(dir.join(file)) {
                st.data.push((bit, Box::leak(bytes.into_boxed_slice())));
            }
        }
    }
    let mut defs = egui::FontDefinitions::default();
    let mut order: Vec<&(u8, &'static [u8])> = st.data.iter().collect();
    // Geleneksel Cince seciliyse JhengHei, YaHei'den once gelir.
    let tw_first = crate::lang::current().code == "zh-tw";
    order.sort_by_key(|(bit, _)| match *bit {
        TW if tw_first => 1,
        b => b,
    });
    for (bit, bytes) in order {
        let (_, name, _, index) = FILES.iter().find(|f| f.0 == *bit).copied().unwrap_or(FILES[0]);
        let mut fd = egui::FontData::from_static(bytes);
        fd.index = index;
        defs.font_data.insert(name.to_owned(), fd);
        for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            let list = defs.families.entry(fam).or_default();
            if *bit == SEGOE && segoe_first {
                list.insert(0, name.to_owned());
            } else {
                list.push(name.to_owned());
            }
        }
    }
    ctx.set_fonts(defs);
    st.applied = st.data.iter().fold(0, |m, d| m | d.0) | want;
}
