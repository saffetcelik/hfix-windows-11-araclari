//! Arayuz dili.
//!
//! Metinler kodda Turkce ve Ingilizce olarak yan yana durur: `t("Kapat", "Close")` ya da bicimli
//! metinler icin `tr!("{} dosya", "{} files", n)`. Diger diller `diller/<kod>.json` dosyalarindan
//! gelir: her dosya Ingilizce metni o dildeki karsiligina eslerdir; karsiligi olmayan metin
//! Ingilizce gosterilir. "Otomatik" secenegi Windows goruntuleme diline gore secer.

use std::collections::HashMap;
use std::fmt::Display;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::RwLock;

pub struct Lang {
    pub code: &'static str,
    /// Dilin kendi adiyla adi (secim listesinde gosterilir).
    pub name: &'static str,
    /// Ingilizce adi (ipucu).
    pub english: &'static str,
    data: &'static str,
    pub rtl: bool,
}

/// Sira: once Turkce ve Ingilizce, sonra digerleri. Indeksler sabittir (0 Turkce, 1 Ingilizce).
pub const LANGS: [Lang; 20] = [
    Lang { code: "tr", name: "Türkçe", english: "Turkish", data: "", rtl: false },
    Lang { code: "en", name: "English", english: "English", data: "", rtl: false },
    Lang { code: "zh", name: "简体中文", english: "Chinese (Simplified)", data: include_str!("../diller/zh.json"), rtl: false },
    Lang { code: "zh-tw", name: "繁體中文", english: "Chinese (Traditional)", data: include_str!("../diller/zh-tw.json"), rtl: false },
    Lang { code: "es", name: "Español", english: "Spanish", data: include_str!("../diller/es.json"), rtl: false },
    Lang { code: "pt", name: "Português (Brasil)", english: "Portuguese (Brazil)", data: include_str!("../diller/pt.json"), rtl: false },
    Lang { code: "de", name: "Deutsch", english: "German", data: include_str!("../diller/de.json"), rtl: false },
    Lang { code: "fr", name: "Français", english: "French", data: include_str!("../diller/fr.json"), rtl: false },
    Lang { code: "ru", name: "Русский", english: "Russian", data: include_str!("../diller/ru.json"), rtl: false },
    Lang { code: "it", name: "Italiano", english: "Italian", data: include_str!("../diller/it.json"), rtl: false },
    Lang { code: "nl", name: "Nederlands", english: "Dutch", data: include_str!("../diller/nl.json"), rtl: false },
    Lang { code: "pl", name: "Polski", english: "Polish", data: include_str!("../diller/pl.json"), rtl: false },
    Lang { code: "cs", name: "Čeština", english: "Czech", data: include_str!("../diller/cs.json"), rtl: false },
    Lang { code: "uk", name: "Українська", english: "Ukrainian", data: include_str!("../diller/uk.json"), rtl: false },
    Lang { code: "id", name: "Bahasa Indonesia", english: "Indonesian", data: include_str!("../diller/id.json"), rtl: false },
    Lang { code: "vi", name: "Tiếng Việt", english: "Vietnamese", data: include_str!("../diller/vi.json"), rtl: false },
    Lang { code: "ja", name: "日本語", english: "Japanese", data: include_str!("../diller/ja.json"), rtl: false },
    Lang { code: "ko", name: "한국어", english: "Korean", data: include_str!("../diller/ko.json"), rtl: false },
    Lang { code: "ar", name: "العربية", english: "Arabic", data: include_str!("../diller/ar.json"), rtl: true },
    Lang { code: "fa", name: "فارسی", english: "Persian", data: include_str!("../diller/fa.json"), rtl: true },
];

const TR: usize = 0;
const EN: usize = 1;

static CUR: AtomicUsize = AtomicUsize::new(TR);
/// Gecerli dilin sozlugu: Ingilizce metin -> ceviri. Arapcada yer tutucusuz metinler sagdan sola
/// gosterilmeye hazir (sekillendirilmis) olarak saklanir.
static DICT: RwLock<Option<HashMap<String, &'static str>>> = RwLock::new(None);

#[link(name = "kernel32")]
extern "system" {
    fn GetUserDefaultUILanguage() -> u16;
}

/// Windows goruntuleme dili Turkce mi?
pub fn system_turkish() -> bool {
    unsafe { GetUserDefaultUILanguage() & 0x3ff == 0x1f }
}

/// Windows goruntuleme diline en uygun dil.
fn system_lang() -> usize {
    let id = unsafe { GetUserDefaultUILanguage() };
    let code = match id & 0x3ff {
        0x1f => "tr",
        // Basitlestirilmis Cince: Cin, Singapur. Geleneksel Cince: Tayvan, Hong Kong, Makao.
        0x04 if id == 0x0804 || id == 0x1004 => "zh",
        0x04 => "zh-tw",
        0x13 => "nl",
        0x15 => "pl",
        0x05 => "cs",
        0x22 => "uk",
        0x21 => "id",
        0x2a => "vi",
        0x29 => "fa",
        0x0a => "es",
        0x16 => "pt",
        0x07 => "de",
        0x0c => "fr",
        0x19 => "ru",
        0x10 => "it",
        0x11 => "ja",
        0x12 => "ko",
        0x01 => "ar",
        _ => "en",
    };
    LANGS.iter().position(|l| l.code == code).unwrap_or(EN)
}

/// `pref`: bir dil kodu ("tr", "en", "zh" ...) ya da baska her sey (otomatik).
pub fn set(pref: &str) {
    // Test: FB_DIL ortam degiskeni ayari ezer.
    let forced = std::env::var("FB_DIL").ok();
    let want = forced.as_deref().unwrap_or(pref);
    let idx = LANGS.iter().position(|l| l.code == want).unwrap_or_else(system_lang);
    let dict = (idx > EN).then(|| load(&LANGS[idx]));
    if let Ok(mut d) = DICT.write() {
        *d = dict;
    }
    CUR.store(idx, Ordering::Relaxed);
}

fn load(l: &Lang) -> HashMap<String, &'static str> {
    let raw: HashMap<String, String> = serde_json::from_str(l.data).unwrap_or_default();
    raw.into_iter()
        .filter(|(_, v)| !v.trim().is_empty())
        .map(|(k, v)| {
            let v = if l.rtl && !v.contains('{') { rtl(&v) } else { v };
            (k, &*Box::leak(v.into_boxed_str()))
        })
        .collect()
}

/// Gecerli dil.
pub fn current() -> &'static Lang {
    &LANGS[CUR.load(Ordering::Relaxed)]
}

#[doc(hidden)]
pub enum Mode {
    Tr,
    En,
    Other,
}

#[doc(hidden)]
pub fn mode() -> Mode {
    match CUR.load(Ordering::Relaxed) {
        TR => Mode::Tr,
        EN => Mode::En,
        _ => Mode::Other,
    }
}

fn lookup(en_text: &str) -> Option<&'static str> {
    DICT.read().ok()?.as_ref()?.get(en_text).copied()
}

/// Gecerli dildeki metni doner.
pub fn t(tr: &'static str, en_text: &'static str) -> &'static str {
    match CUR.load(Ordering::Relaxed) {
        TR => tr,
        EN => en_text,
        _ => lookup(en_text).unwrap_or(en_text),
    }
}

/// `tr!` makrosunun Turkce ve Ingilizce disindaki diller icin yaptigi bicimleme: cevrilmis
/// sablondaki yer tutucular ({}, {:.1} ...) sirayla argumanlarla doldurulur.
#[doc(hidden)]
pub fn fmt(en_tpl: &'static str, args: &[&dyn Display]) -> String {
    let tpl = lookup(en_tpl).unwrap_or(en_tpl);
    let s = subst(tpl, args);
    if current().rtl {
        rtl(&s)
    } else {
        s
    }
}

fn subst(tpl: &str, args: &[&dyn Display]) -> String {
    let mut out = String::with_capacity(tpl.len() + 16);
    let mut it = tpl.char_indices().peekable();
    let mut next = 0;
    while let Some((i, c)) = it.next() {
        match c {
            '{' if it.peek().map(|x| x.1) == Some('{') => {
                it.next();
                out.push('{');
            }
            '}' if it.peek().map(|x| x.1) == Some('}') => {
                it.next();
                out.push('}');
            }
            '{' => {
                let Some(end) = tpl[i..].find('}') else {
                    out.push_str(&tpl[i..]);
                    break;
                };
                let spec = &tpl[i + 1..i + end];
                while it.peek().is_some_and(|x| x.0 <= i + end) {
                    it.next();
                }
                if let Some(a) = args.get(next) {
                    let prec = spec
                        .split_once('.')
                        .and_then(|(_, p)| p.trim_end_matches(|ch: char| !ch.is_ascii_digit()).parse::<usize>().ok());
                    match prec {
                        Some(p) => out.push_str(&format!("{a:.p$}")),
                        None => out.push_str(&a.to_string()),
                    }
                }
                next += 1;
            }
            _ => out.push(c),
        }
    }
    out
}

/// Arapca metni egui'nin soldan saga ciziminde dogru gorunecek hale getirir: harfleri birlesik
/// bicimlerine cevirir, satirlari sagdan sola siralar ve parantezleri aynalar. egui satir
/// kaydirmayi soldan saga yaptigi icin uzun metinler once 90 karakterlik satirlara bolunur.
pub fn rtl(s: &str) -> String {
    let mut lines = Vec::new();
    for para in s.split('\n') {
        let mut cur = String::new();
        for w in para.split(' ') {
            if !cur.is_empty() && cur.chars().count() + 1 + w.chars().count() > 90 {
                lines.push(std::mem::take(&mut cur));
            }
            if !cur.is_empty() {
                cur.push(' ');
            }
            cur.push_str(w);
        }
        lines.push(cur);
    }
    lines.iter().map(|l| visual_line(l)).collect::<Vec<_>>().join("\n")
}

fn visual_line(line: &str) -> String {
    use unicode_bidi::{BidiInfo, Level};
    let shaped = ar_reshaper::reshape_line(line);
    let bidi = BidiInfo::new(&shaped, Some(Level::rtl()));
    let Some(para) = bidi.paragraphs.first() else {
        return shaped;
    };
    let (levels, runs) = bidi.visual_runs(para, para.range.clone());
    let mut out = String::with_capacity(shaped.len());
    for run in runs {
        let part = &shaped[run.clone()];
        if levels[run.start].is_rtl() {
            out.extend(part.chars().rev().map(|c| match c {
                '(' => ')',
                ')' => '(',
                '[' => ']',
                ']' => '[',
                '{' => '}',
                '}' => '{',
                '<' => '>',
                '>' => '<',
                '«' => '»',
                '»' => '«',
                _ => c,
            }));
        } else {
            out.push_str(part);
        }
    }
    out
}

/// Programin gorunen adi.
pub fn app_name() -> &'static str {
    t("HFix Windows 11 Araçları", "HFix Windows 11 Tools")
}

/// `tr!("Kapat", "Close")` -> &'static str; `tr!("{} dosya", "{} files", n)` -> String.
#[macro_export]
macro_rules! tr {
    ($tr:literal, $en:literal) => {
        $crate::lang::t($tr, $en)
    };
    ($tr:literal, $en:literal, $($arg:expr),+ $(,)?) => {
        match $crate::lang::mode() {
            $crate::lang::Mode::Tr => format!($tr, $($arg),+),
            $crate::lang::Mode::En => format!($en, $($arg),+),
            $crate::lang::Mode::Other => {
                $crate::lang::fmt($en, &[$(&$arg as &dyn ::std::fmt::Display),+])
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitution() {
        let n = 3.14159_f32;
        assert_eq!(subst("{} / {:.1} {{x}}", &[&"a", &n]), "a / 3.1 {x}");
        assert_eq!(subst("%{:.0}", &[&99.6_f32]), "%100");
        assert_eq!(subst("{} ok", &[]), " ok");
    }

    #[test]
    fn dictionaries_parse() {
        for l in &LANGS[2..] {
            let d: HashMap<String, String> = serde_json::from_str(l.data).expect(l.code);
            let _ = d.len();
        }
    }

    #[test]
    fn arabic_is_reordered() {
        let s = rtl("مرحبا (1)");
        assert!(!s.is_empty());
        assert!(s.contains("(1)"));
    }
}
