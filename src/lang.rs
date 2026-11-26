//! Arayuz dili: Turkce ve Ingilizce.
//!
//! Metinler kodda iki dilde yan yana durur: `t("Kapat", "Close")` ya da bicimli metinler icin
//! `tr!("{} dosya", "{} files", n)`. "Otomatik" secenegi Windows goruntuleme dili Turkce ise
//! Turkce, degilse Ingilizce acar.

use std::sync::atomic::{AtomicBool, Ordering};

static EN: AtomicBool = AtomicBool::new(false);

#[link(name = "kernel32")]
extern "system" {
    fn GetUserDefaultUILanguage() -> u16;
}

/// Windows goruntuleme dili Turkce mi?
pub fn system_turkish() -> bool {
    unsafe { GetUserDefaultUILanguage() & 0x3ff == 0x1f }
}

/// `pref`: "tr", "en" ya da baska her sey (otomatik).
pub fn set(pref: &str) {
    // Test: FB_DIL ortam degiskeni ayari ezer.
    let forced = std::env::var("FB_DIL").ok();
    let en = match forced.as_deref().unwrap_or(pref) {
        "tr" => false,
        "en" => true,
        _ => !system_turkish(),
    };
    EN.store(en, Ordering::Relaxed);
}

pub fn en() -> bool {
    EN.load(Ordering::Relaxed)
}

/// Gecerli dildeki metni doner.
pub fn t(tr: &'static str, en_text: &'static str) -> &'static str {
    if en() {
        en_text
    } else {
        tr
    }
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
    ($tr:literal, $en:literal, $($arg:tt)*) => {
        if $crate::lang::en() {
            format!($en, $($arg)*)
        } else {
            format!($tr, $($arg)*)
        }
    };
}
