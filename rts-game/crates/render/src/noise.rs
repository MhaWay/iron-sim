//! Value noise on SplitMix64 (integer fixed-point lattice, smoothstep bilinear). Deterministic.

#[inline]
pub fn splitmix64(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[inline]
fn lattice(ix: i32, iy: i32, seed: u64) -> u32 {
    (splitmix64(seed ^ ((ix as u64) << 32) ^ ((iy as u64) << 16)) >> 48) as u32
}

#[inline]
fn smooth(t: u32) -> u32 {
    let t = t.min(65535);
    (((t * t) >> 16) * (3 * 65536 - 2 * t)) >> 16
}

/// Value noise, fixed-point (16.16) coords in, 0..65535 out. Integer-only.
pub fn value(x: i32, y: i32, seed: u64) -> u32 {
    let xi = x >> 16;
    let yi = y >> 16;
    let fx = (x & 0xFFFF) as u32;
    let fy = (y & 0xFFFF) as u32;
    let sx = smooth(fx) as i64;
    let sy = smooth(fy) as i64;
    let v00 = lattice(xi, yi, seed) as i64;
    let v10 = lattice(xi + 1, yi, seed) as i64;
    let v01 = lattice(xi, yi + 1, seed) as i64;
    let v11 = lattice(xi + 1, yi + 1, seed) as i64;
    let x1 = v00 + ((v10 - v00) * sx) / 65536;
    let x2 = v01 + ((v11 - v01) * sx) / 65536;
    let v = x1 + ((x2 - x1) * sy) / 65536;
    v.clamp(0, 65535) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deterministic_and_bounded() {
        assert_eq!(value(114514, 98765, 7), value(114514, 98765, 7));
        assert!(value(-123456, 77777, 1) <= 65535);
        assert!(value(12345678, -987654321, 12345) <= 65535);
        assert_ne!(
            value(1 << 17, 1 << 17, 7),
            value((1 << 17) + 65536, 1 << 17, 7)
        );
    }
}
