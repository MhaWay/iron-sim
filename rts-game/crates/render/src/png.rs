//! Minimal pure-Rust PNG encoder (RGB8, stored deflate chunks, CRC32). No external crates;
//! deterministic byte-for-byte for identical input. Render layer only (never feeds the sim hash).

pub struct Image {
    pub w: usize,
    pub h: usize,
    pub data: Vec<u8>, // w*h*3, row-major RGB
}

impl Image {
    pub fn new(w: usize, h: usize) -> Image {
        Image {
            w,
            h,
            data: vec![0; w * h * 3],
        }
    }
    #[inline]
    pub fn set(&mut self, x: usize, y: usize, c: [u8; 3]) {
        if x < self.w && y < self.h {
            let i = (y * self.w + x) * 3;
            self.data[i] = c[0];
            self.data[i + 1] = c[1];
            self.data[i + 2] = c[2];
        }
    }
    #[inline]
    pub fn get(&self, x: usize, y: usize) -> [u8; 3] {
        if x < self.w && y < self.h {
            let i = (y * self.w + x) * 3;
            [self.data[i], self.data[i + 1], self.data[i + 2]]
        } else {
            [0, 0, 0]
        }
    }
    pub fn fill(&mut self, c: [u8; 3]) {
        let d = vec![c[0], c[1], c[2]];
        let mut v = Vec::with_capacity(self.w * self.h * 3);
        for _ in 0..(self.w * self.h) {
            v.extend_from_slice(&d);
        }
        self.data = v;
    }
    pub fn rect(&mut self, x: usize, y: usize, w: usize, h: usize, c: [u8; 3]) {
        for yy in y..(y + h).min(self.h) {
            for xx in x..(x + w).min(self.w) {
                self.set(xx, yy, c);
            }
        }
    }
    pub fn rect_border(&mut self, x: usize, y: usize, w: usize, h: usize, c: [u8; 3]) {
        for xx in x..(x + w).min(self.w) {
            self.set(xx, y, c);
            self.set(xx, (y + h).saturating_sub(1), c);
        }
        for yy in y..(y + h).min(self.h) {
            self.set(x, yy, c);
            self.set((x + w).saturating_sub(1), yy, c);
        }
    }
    pub fn hline(&mut self, x: usize, y: usize, len: usize, c: [u8; 3]) {
        for xx in x..(x + len).min(self.w) {
            self.set(xx, y, c);
        }
    }
    pub fn vline(&mut self, x: usize, y: usize, len: usize, c: [u8; 3]) {
        for yy in y..(y + len).min(self.h) {
            self.set(x, yy, c);
        }
    }

    /// Encode to PNG bytes using stored (uncompressed) deflate blocks — tiny encoder.
    pub fn encode(&self) -> Vec<u8> {
        let stride = self.w * 3;
        let mut raw: Vec<u8> = Vec::with_capacity((stride + 1) * self.h);
        for y in 0..self.h {
            raw.push(0); // filter byte: none
            let off = y * stride;
            raw.extend_from_slice(&self.data[off..off + stride]);
        }
        let idat = stored_deflate(&raw);
        let mut out: Vec<u8> = Vec::new();
        out.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
        chunk(&mut out, b"IHDR", &{
            let mut b = Vec::new();
            b.extend_from_slice(&(self.w as u32).to_be_bytes());
            b.extend_from_slice(&(self.h as u32).to_be_bytes());
            b.push(8); // bit depth
            b.push(2); // color type: RGB
            b.push(0);
            b.push(0);
            b.push(0);
            b
        });
        chunk(&mut out, b"IDAT", &idat);
        chunk(&mut out, b"IEND", &[]);
        out
    }
}

fn stored_deflate(raw: &[u8]) -> Vec<u8> {
    // zlib wrapper: 0x78 0x01 (32K window, no preset dict), stored blocks, adler32 of raw.
    let mut out = Vec::with_capacity(raw.len() + 16);
    out.push(0x78);
    out.push(0x01);
    let mut i = 0usize;
    if raw.is_empty() {
        out.push(1);
        out.extend_from_slice(&[0, 0, 0xff, 0xff]);
    } else {
        while i < raw.len() {
            let n = (raw.len() - i).min(65535);
            let last = i + n >= raw.len();
            out.push(if last { 1 } else { 0 });
            let l = n as u16;
            out.push((l & 0xff) as u8);
            out.push((l >> 8) as u8);
            out.push((!l & 0xff) as u8);
            out.push((!l >> 8) as u8);
            out.extend_from_slice(&raw[i..i + n]);
            i += n;
        }
    }
    // adler32
    let (mut a, mut bsum) = (1u32, 0u32);
    for &byte in raw {
        a = (a + byte as u32) % 65521;
        bsum = (bsum + a) % 65521;
    }
    out.extend_from_slice(&((bsum << 16) | a).to_be_bytes());
    out
}

fn chunk(out: &mut Vec<u8>, tag: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut buf = Vec::with_capacity(4 + data.len());
    buf.extend_from_slice(tag);
    buf.extend_from_slice(data);
    let c = crc32(&buf);
    out.extend_from_slice(tag);
    out.extend_from_slice(data);
    out.extend_from_slice(&c.to_be_bytes());
}

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for i in 0..256u32 {
        let mut c = i;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
        table[i as usize] = c;
    }
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c = table[((c ^ b as u32) & 0xff) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn png_deterministic_and_signature() {
        let mut img = Image::new(8, 4);
        img.rect(1, 1, 4, 2, [10, 20, 30]);
        let a = img.encode();
        let b = img.encode();
        assert_eq!(a, b);
        assert_eq!(&a[0..8], &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
        assert_eq!(&a[16..20], &8u32.to_be_bytes());
        assert_eq!(&a[20..24], &4u32.to_be_bytes());
    }
}
