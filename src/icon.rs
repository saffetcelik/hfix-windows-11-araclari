//! Program simgesi: kod ile cizilir (mavi yuvarlak kare uzerinde beyaz "H" ve hiz cizgisi), boylece
//! derlemede kaynak derleyicisine gerek kalmaz. Pencere, tepsi ve kisayol ayni cizimi kullanir.

/// `n` x `n` RGBA goruntu (ustten alta).
pub fn rgba(n: usize) -> Vec<u8> {
    const SS: usize = 4; // kenar yumusatma icin alt ornekleme
    let mut out = vec![0u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let (mut r, mut g, mut b, mut a) = (0f32, 0f32, 0f32, 0f32);
            for sy in 0..SS {
                for sx in 0..SS {
                    let u = (x as f32 + (sx as f32 + 0.5) / SS as f32) / n as f32;
                    let v = (y as f32 + (sy as f32 + 0.5) / SS as f32) / n as f32;
                    if let Some([pr, pg, pb]) = pixel(u, v) {
                        r += pr;
                        g += pg;
                        b += pb;
                        a += 1.0;
                    }
                }
            }
            let i = (y * n + x) * 4;
            if a > 0.0 {
                out[i] = (r / a) as u8;
                out[i + 1] = (g / a) as u8;
                out[i + 2] = (b / a) as u8;
                out[i + 3] = (a / (SS * SS) as f32 * 255.0) as u8;
            }
        }
    }
    out
}

/// Birim karedeki (0..1) noktanin rengi; simgenin disindaysa None.
fn pixel(u: f32, v: f32) -> Option<[f32; 3]> {
    // Yuvarlak koseli kare.
    let (m, rad) = (0.04, 0.22);
    let dx = ((u - 0.5).abs() - (0.5 - m - rad)).max(0.0);
    let dy = ((v - 0.5).abs() - (0.5 - m - rad)).max(0.0);
    if dx * dx + dy * dy > rad * rad {
        return None;
    }
    let white = [255.0, 255.0, 255.0];
    // "H": iki dikme ve orta cizgi.
    let stem = |x0: f32| u >= x0 && u <= x0 + 0.13 && (0.24..=0.76).contains(&v);
    if stem(0.24) || stem(0.63) || ((0.24..=0.76).contains(&u) && (0.44..=0.56).contains(&v)) {
        return Some(white);
    }
    // Ustten alta hafif koyulasan mavi.
    Some([30.0 - 12.0 * v, 126.0 - 34.0 * v, 222.0 - 40.0 * v])
}

/// Birden cok boyut iceren .ico dosyasi (32 bit BMP girdileri).
pub fn ico() -> Vec<u8> {
    let sizes = [16usize, 24, 32, 48, 64, 256];
    let images: Vec<Vec<u8>> = sizes.iter().map(|&n| bmp_entry(n)).collect();
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len();
    for (n, img) in sizes.iter().zip(&images) {
        let dim = if *n >= 256 { 0 } else { *n as u8 };
        out.extend_from_slice(&[dim, dim, 0, 0, 1, 0, 32, 0]);
        out.extend_from_slice(&(img.len() as u32).to_le_bytes());
        out.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += img.len();
    }
    for img in images {
        out.extend_from_slice(&img);
    }
    out
}

fn bmp_entry(n: usize) -> Vec<u8> {
    let px = rgba(n);
    let mut out = Vec::with_capacity(40 + n * n * 4 + n * n / 8);
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(n as i32).to_le_bytes());
    out.extend_from_slice(&(2 * n as i32).to_le_bytes()); // renk + maske
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&[0; 24]);
    for y in (0..n).rev() {
        for x in 0..n {
            let i = (y * n + x) * 4;
            out.extend_from_slice(&[px[i + 2], px[i + 1], px[i], px[i + 3]]);
        }
    }
    // Saydamlik alfa kanalindan gelir; maske bos birakilir (satirlar 32 bite tamamlanir).
    out.resize(out.len() + n.div_ceil(32) * 4 * n, 0);
    out
}
