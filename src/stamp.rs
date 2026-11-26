//! Derlenen exe'ye surum bilgisi (urun adi, surum, yayinci) ve simge ekler.
//!
//! Kaynak derleyicisi (rc.exe / windres) gerektirmemek icin Windows'un kendi UpdateResource
//! islevleri kullanilir: yayimlama adimi derlenen exe'yi kopyalar ve
//! `HFixWin11.exe --damgala <kopya>` ile damgalar. Imzalar bu adimdan sonra atilir.

use crate::update::VERSION;
use std::ffi::c_void;

#[link(name = "kernel32")]
extern "system" {
    fn BeginUpdateResourceW(file: *const u16, delete_existing: i32) -> *mut c_void;
    fn UpdateResourceW(h: *mut c_void, kind: usize, name: usize, lang: u16, data: *const u8, size: u32) -> i32;
    fn EndUpdateResourceW(h: *mut c_void, discard: i32) -> i32;
}

const RT_ICON: usize = 3;
const RT_GROUP_ICON: usize = 14;
const RT_VERSION: usize = 16;
const LANG_EN_US: u16 = 0x0409;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn pad4(b: &mut Vec<u8>) {
    while b.len() % 4 != 0 {
        b.push(0);
    }
}

/// Surum bilgisi agacinin bir dugumu: uzunluk, deger uzunlugu, tur, anahtar, deger, alt dugumler.
fn node(key: &str, text: bool, value: &[u8], value_len: u16, children: &[Vec<u8>]) -> Vec<u8> {
    let mut b = vec![0u8; 6];
    for u in wide(key) {
        b.extend_from_slice(&u.to_le_bytes());
    }
    pad4(&mut b);
    b.extend_from_slice(value);
    for c in children {
        pad4(&mut b);
        b.extend_from_slice(c);
    }
    let len = b.len() as u16;
    b[0..2].copy_from_slice(&len.to_le_bytes());
    b[2..4].copy_from_slice(&value_len.to_le_bytes());
    b[4..6].copy_from_slice(&(text as u16).to_le_bytes());
    b
}

fn string(key: &str, value: &str) -> Vec<u8> {
    let w = wide(value);
    let bytes: Vec<u8> = w.iter().flat_map(|u| u.to_le_bytes()).collect();
    node(key, true, &bytes, w.len() as u16, &[])
}

fn version_info() -> Vec<u8> {
    let mut parts = VERSION.split(['.', '-', '+']).map(|p| p.parse::<u32>().unwrap_or(0));
    let (ma, mi, pa) = (parts.next().unwrap_or(0), parts.next().unwrap_or(0), parts.next().unwrap_or(0));
    let (ms, ls) = ((ma << 16) | (mi & 0xFFFF), (pa & 0xFFFF) << 16);
    // VS_FIXEDFILEINFO
    let fixed: Vec<u8> = [
        0xFEEF_04BDu32, // imza
        0x0001_0000,    // yapi surumu
        ms,
        ls,
        ms,
        ls,
        0x3F,      // bayrak maskesi
        0,         // bayraklar
        0x0004_0004, // VOS_NT_WINDOWS32
        1,         // VFT_APP
        0,
        0,
        0,
    ]
    .iter()
    .flat_map(|v| v.to_le_bytes())
    .collect();
    let name = "HFix Windows 11 Araçları";
    let table = node(
        "040904B0",
        true,
        &[],
        0,
        &[
            string("CompanyName", "HukukFix"),
            string("FileDescription", name),
            string("FileVersion", VERSION),
            string("InternalName", "HFixWin11"),
            string("LegalCopyright", "© 2026 HukukFix. MIT License."),
            string("OriginalFilename", "HFixWin11.exe"),
            string("ProductName", name),
            string("ProductVersion", VERSION),
        ],
    );
    let strings = node("StringFileInfo", true, &[], 0, &[table]);
    let mut tr = Vec::new();
    tr.extend_from_slice(&LANG_EN_US.to_le_bytes());
    tr.extend_from_slice(&0x04B0u16.to_le_bytes());
    let var = node("VarFileInfo", true, &[], 0, &[node("Translation", false, &tr, 4, &[])]);
    node("VS_VERSION_INFO", false, &fixed, fixed.len() as u16, &[strings, var])
}

/// `exe` dosyasina surum bilgisi ve simge ekler. Cikis kodu: 0 basarili, 1 basarisiz.
pub fn cmd_stamp(exe: &str) -> i32 {
    let sizes = [16usize, 24, 32, 48, 64, 256];
    let images: Vec<Vec<u8>> = sizes.iter().map(|&n| crate::icon::bmp_entry(n)).collect();
    // GRPICONDIR: simge grubunun dizini; her girdi bir RT_ICON kaynagina (numarasiyla) isaret eder.
    let mut group = Vec::new();
    group.extend_from_slice(&[0, 0, 1, 0]);
    group.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    for (i, (n, img)) in sizes.iter().zip(&images).enumerate() {
        let dim = if *n >= 256 { 0 } else { *n as u8 };
        group.extend_from_slice(&[dim, dim, 0, 0, 1, 0, 32, 0]);
        group.extend_from_slice(&(img.len() as u32).to_le_bytes());
        group.extend_from_slice(&(i as u16 + 1).to_le_bytes());
    }
    let ver = version_info();
    unsafe {
        let h = BeginUpdateResourceW(wide(exe).as_ptr(), 0);
        if h.is_null() {
            return 1;
        }
        let mut ok = true;
        for (i, img) in images.iter().enumerate() {
            ok &= UpdateResourceW(h, RT_ICON, i + 1, LANG_EN_US, img.as_ptr(), img.len() as u32) != 0;
        }
        ok &= UpdateResourceW(h, RT_GROUP_ICON, 1, LANG_EN_US, group.as_ptr(), group.len() as u32) != 0;
        ok &= UpdateResourceW(h, RT_VERSION, 1, LANG_EN_US, ver.as_ptr(), ver.len() as u32) != 0;
        let done = EndUpdateResourceW(h, !ok as i32) != 0;
        !(ok && done) as i32
    }
}
