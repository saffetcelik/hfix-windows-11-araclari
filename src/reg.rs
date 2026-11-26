//! Kayit defteri icin kucuk bir sarmalayici (dogrudan advapi32).
#![allow(non_snake_case)]

use std::ptr::{null, null_mut};

pub const HKCU: isize = 0x8000_0001u32 as i32 as isize;
pub const HKLM: isize = 0x8000_0002u32 as i32 as isize;

const KEY_READ: u32 = 0x20019;
const KEY_WRITE: u32 = 0x20006;

#[link(name = "advapi32")]
extern "system" {
    fn RegOpenKeyExW(h: isize, sub: *const u16, opt: u32, sam: u32, out: *mut isize) -> i32;
    fn RegCreateKeyExW(
        h: isize,
        sub: *const u16,
        res: u32,
        class: *const u16,
        opt: u32,
        sam: u32,
        sec: *const u8,
        out: *mut isize,
        disp: *mut u32,
    ) -> i32;
    fn RegCloseKey(h: isize) -> i32;
    fn RegEnumKeyExW(
        h: isize,
        idx: u32,
        name: *mut u16,
        len: *mut u32,
        res: *const u32,
        class: *mut u16,
        clen: *mut u32,
        ft: *mut u8,
    ) -> i32;
    fn RegEnumValueW(
        h: isize,
        idx: u32,
        name: *mut u16,
        len: *mut u32,
        res: *const u32,
        ty: *mut u32,
        data: *mut u8,
        dlen: *mut u32,
    ) -> i32;
    fn RegQueryValueExW(
        h: isize,
        name: *const u16,
        res: *const u32,
        ty: *mut u32,
        data: *mut u8,
        dlen: *mut u32,
    ) -> i32;
    fn RegLoadMUIStringW(
        h: isize,
        name: *const u16,
        out: *mut u16,
        cb: u32,
        needed: *mut u32,
        flags: u32,
        dir: *const u16,
    ) -> i32;
    fn RegDeleteValueW(h: isize, name: *const u16) -> i32;
    fn RegDeleteTreeW(h: isize, sub: *const u16) -> i32;
    fn RegSetValueExW(
        h: isize,
        name: *const u16,
        res: u32,
        ty: u32,
        data: *const u8,
        dlen: u32,
    ) -> i32;
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// REG_SZ / REG_EXPAND_SZ verisini metne cevirir.
pub fn to_string(data: &[u8]) -> String {
    let u: Vec<u16> = data
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|c| *c != 0)
        .collect();
    String::from_utf16_lossy(&u)
}

pub struct Key(isize);

impl Key {
    pub fn open(root: isize, path: &str, write: bool) -> Option<Key> {
        let mut h = 0isize;
        let sam = if write { KEY_READ | KEY_WRITE } else { KEY_READ };
        let rc = unsafe { RegOpenKeyExW(root, wide(path).as_ptr(), 0, sam, &mut h) };
        (rc == 0).then_some(Key(h))
    }

    pub fn create(root: isize, path: &str) -> Option<Key> {
        let mut h = 0isize;
        let rc = unsafe {
            RegCreateKeyExW(
                root,
                wide(path).as_ptr(),
                0,
                null(),
                0,
                KEY_READ | KEY_WRITE,
                null(),
                &mut h,
                null_mut(),
            )
        };
        (rc == 0).then_some(Key(h))
    }

    pub fn subkeys(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut i = 0;
        loop {
            let mut name = [0u16; 260];
            let mut len = name.len() as u32;
            let rc = unsafe {
                RegEnumKeyExW(
                    self.0,
                    i,
                    name.as_mut_ptr(),
                    &mut len,
                    null(),
                    null_mut(),
                    null_mut(),
                    null_mut(),
                )
            };
            if rc != 0 {
                break;
            }
            out.push(String::from_utf16_lossy(&name[..len as usize]));
            i += 1;
        }
        out
    }

    /// (ad, tur, veri) listesi.
    pub fn values(&self) -> Vec<(String, u32, Vec<u8>)> {
        let mut out = Vec::new();
        let mut i = 0;
        loop {
            let mut name = vec![0u16; 16384];
            let mut len = name.len() as u32;
            let mut ty = 0u32;
            let mut data = vec![0u8; 32768];
            let mut dlen = data.len() as u32;
            let rc = unsafe {
                RegEnumValueW(
                    self.0,
                    i,
                    name.as_mut_ptr(),
                    &mut len,
                    null(),
                    &mut ty,
                    data.as_mut_ptr(),
                    &mut dlen,
                )
            };
            if rc == 259 {
                break; // ERROR_NO_MORE_ITEMS
            }
            if rc == 0 {
                data.truncate(dlen as usize);
                out.push((String::from_utf16_lossy(&name[..len as usize]), ty, data));
            }
            i += 1;
            if i > 5000 {
                break;
            }
        }
        out
    }

    pub fn get(&self, name: &str) -> Option<(u32, Vec<u8>)> {
        let w = wide(name);
        let mut ty = 0u32;
        let mut dlen = 0u32;
        let rc = unsafe {
            RegQueryValueExW(self.0, w.as_ptr(), null(), &mut ty, null_mut(), &mut dlen)
        };
        if rc != 0 {
            return None;
        }
        let mut data = vec![0u8; dlen as usize + 2];
        let mut dlen2 = data.len() as u32;
        let rc = unsafe {
            RegQueryValueExW(
                self.0,
                w.as_ptr(),
                null(),
                &mut ty,
                data.as_mut_ptr(),
                &mut dlen2,
            )
        };
        if rc != 0 {
            return None;
        }
        data.truncate(dlen2 as usize);
        Some((ty, data))
    }

    pub fn get_str(&self, name: &str) -> Option<String> {
        let (ty, d) = self.get(name)?;
        (ty == 1 || ty == 2).then(|| to_string(&d))
    }

    /// "@dosya.dll,-123" bicimindeki yerellestirilmis metni cozer.
    pub fn get_mui(&self, name: &str) -> Option<String> {
        let mut buf = vec![0u16; 2048];
        let mut need = 0u32;
        let rc = unsafe {
            RegLoadMUIStringW(
                self.0,
                wide(name).as_ptr(),
                buf.as_mut_ptr(),
                (buf.len() * 2) as u32,
                &mut need,
                0,
                null(),
            )
        };
        if rc == 0 {
            let n = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
            let s = String::from_utf16_lossy(&buf[..n]);
            if !s.is_empty() {
                return Some(s);
            }
        }
        self.get_str(name).filter(|s| !s.starts_with('@'))
    }

    pub fn set_u32(&self, name: &str, v: u32) -> bool {
        let b = v.to_le_bytes();
        unsafe { RegSetValueExW(self.0, wide(name).as_ptr(), 0, 4, b.as_ptr(), 4) == 0 }
    }

    pub fn set_str(&self, name: &str, v: &str) -> bool {
        let w = wide(v);
        unsafe {
            RegSetValueExW(self.0, wide(name).as_ptr(), 0, 1, w.as_ptr() as *const u8, (w.len() * 2) as u32) == 0
        }
    }

    /// Anahtari alt anahtarlariyla birlikte siler.
    pub fn delete_tree(root: isize, path: &str) -> bool {
        let rc = unsafe { RegDeleteTreeW(root, wide(path).as_ptr()) };
        rc == 0 || rc == 2
    }

    pub fn delete_value(&self, name: &str) -> bool {
        unsafe { RegDeleteValueW(self.0, wide(name).as_ptr()) == 0 }
    }

    pub fn get_u32(&self, name: &str) -> Option<u32> {
        let (ty, d) = self.get(name)?;
        (ty == 4 && d.len() >= 4).then(|| u32::from_le_bytes([d[0], d[1], d[2], d[3]]))
    }

    pub fn set_bin(&self, name: &str, data: &[u8]) -> bool {
        unsafe {
            RegSetValueExW(
                self.0,
                wide(name).as_ptr(),
                0,
                3,
                data.as_ptr(),
                data.len() as u32,
            ) == 0
        }
    }
}

impl Drop for Key {
    fn drop(&mut self) {
        unsafe { RegCloseKey(self.0) };
    }
}

/// %DEGISKEN% ifadelerini ortam degiskenleriyle acar.
pub fn expand_env(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(a) = rest.find('%') {
        out.push_str(&rest[..a]);
        let after = &rest[a + 1..];
        match after.find('%') {
            Some(b) => {
                let var = &after[..b];
                match std::env::var(var) {
                    Ok(v) if !var.is_empty() => out.push_str(&v),
                    _ => {
                        out.push('%');
                        out.push_str(var);
                        out.push('%');
                    }
                }
                rest = &after[b + 1..];
            }
            None => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}
