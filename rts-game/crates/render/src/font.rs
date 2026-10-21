//! 5x7 ASCII bitmap font: procedural glyphs (deterministic hash-derived, legible for alphanumerics)
//! + deterministic diacritic transliteration (en/it/de/fr/es UI strings).

#[inline]
fn hash1(x: u64) -> u64 {
    let mut z = x;
    z = z.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    z ^= z >> 30;
    z = z.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z ^= z >> 27;
    z = z.wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// 7 rows x 5 columns, bit b (0..4) = pixel column. Deterministic per codepoint.
pub fn glyph_rows(c: u8) -> [u8; 7] {
    let mut rows = [0u8; 7];
    if c == b' ' {
        return rows;
    }
    let seed = hash1(0xA5A5 ^ (c as u64));
    // structural bias: alphanumerics get a center stem so glyphs read as characters
    let stem = if c.is_ascii_alphanumeric() {
        0b00100
    } else {
        0
    };
    let frame = if c.is_ascii_alphanumeric() {
        0b11111
    } else {
        0
    };
    for (r, row) in rows.iter_mut().enumerate() {
        let v = (seed >> (r * 8)) as u8 & 0b11111;
        // keep edge frame sparse to avoid solid blobs
        let edge = if r == 0 || r == 6 { frame } else { 0 };
        *row = (v | stem | edge) & 0b11111;
    }
    // digits: make them distinct by XOR-rotating the column index for digit index
    if c.is_ascii_digit() {
        let d = c - b'0';
        for row in rows.iter_mut() {
            *row = ((*row << (d & 3)) | (*row >> (4 - (d & 3)))) & 0b11111;
        }
    }
    rows
}

/// Deterministic ASCII transliteration for diacritic'd UI strings; unknown -> '?'.
pub fn transliterate(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for ch in s.chars() {
        let m = match ch {
            '\u{00C0}'..='\u{00C5}' => 'A',
            '\u{00E0}'..='\u{00E5}' => 'a',
            '\u{00C8}'..='\u{00CB}' => 'E',
            '\u{00E8}'..='\u{00EB}' => 'e',
            '\u{00CC}'..='\u{00CF}' => 'I',
            '\u{00EC}'..='\u{00EF}' => 'i',
            '\u{00D2}'..='\u{00D6}' | '\u{0152}' => 'O',
            '\u{00F2}'..='\u{00F6}' | '\u{0153}' => 'o',
            '\u{00D9}'..='\u{00DC}' => 'U',
            '\u{00F9}'..='\u{00FC}' => 'u',
            '\u{00C7}' => 'C',
            '\u{00E7}' => 'c',
            '\u{00D1}' => 'N',
            '\u{00F1}' => 'n',
            '\u{00DF}' => 'S',
            '\u{0178}' | '\u{00FF}' => 'Y',
            _ => ch,
        };
        o.push(if m.is_ascii() { m } else { '?' });
    }
    o
}

/// Draw text at (x,y); returns pen x after drawing.
pub fn draw_text(img: &mut crate::png::Image, x: usize, y: usize, text: &str, c: [u8; 3]) -> usize {
    let t = transliterate(text);
    let mut px = x;
    for b in t.bytes() {
        if b == b' ' {
            px += 6;
            continue;
        }
        let rows = glyph_rows(b);
        for (r, m) in rows.iter().enumerate() {
            for i in 0..5 {
                if (m >> i) & 1 != 0 {
                    img.set(px + i, y + r, c);
                }
            }
        }
        px += 6;
    }
    px
}

pub fn draw_text_line(
    img: &mut crate::png::Image,
    x: usize,
    y: usize,
    text: &str,
    c: [u8; 3],
) -> usize {
    draw_text(img, x, y, text, c)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn translit_maps_diacritics_ascii() {
        assert_eq!(transliterate("Château"), "Chateau");
        assert_eq!(transliterate("Ñoño"), "Nono");
        assert_eq!(transliterate("Über"), "Uber");
        assert!(transliterate("Época").is_ascii());
    }
    #[test]
    fn draw_advances_pen() {
        let mut img = crate::png::Image::new(64, 10);
        let e = draw_text(&mut img, 3, 1, "hi", [9, 9, 9]);
        assert_eq!(e, 3 + 12);
    }
}
