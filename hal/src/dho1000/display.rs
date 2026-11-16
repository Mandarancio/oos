// Copyright (C) 2026 Julian Decker
// SPDX-License-Identifier: GPL-2.0-or-later

//! DHO1000 display path: compression and linear interpolation, reconstructed from the stock library.
//!
//! Evidence: `DevAcquireSPU_SetCompress` @0x2a74fc, `DevAcquireSPU_SetLinearInterp` @0x2a7334 and
//! their callers in `ConfigHorizontal` @0x2d962c (decompiled in `docs/decomp/libscope`). Not yet
//! verified on hardware.
//!
//! These are the two stages between the record memory and the TX stream: compression decides how many
//! TX points a window makes, interpolation how many the frame asks for. A mismatch leaves the frame
//! incomplete and the read stalls (the same failure the DHO800/900 documents in `record-play.md`);
//! `passthrough()` is the known fix - compression off, interpolation off.

/// Main compression (register 0x1170, bit 31 = more than one channel on).
pub const COMPRESS_MAIN: u32 = 0x1170;
/// Zoom compression (register 0x1174).
pub const COMPRESS_ZOOM: u32 = 0x1174;
/// Interpolation enable bits: bit 0 = main, bit 1 = zoom.
pub const INTERP_EN: u32 = 0x1154;
/// Main interpolation phase step and offset.
pub const INTERP_MAIN: [u32; 2] = [0x1158, 0x115c];
/// Zoom interpolation phase step and offset.
pub const INTERP_ZOOM: [u32; 2] = [0x1160, 0x1164];

/// `DevAcquireSPU_SetCompress(more_than_one_channel, path, value)`: bits 0-27 = value, bit 31 =
/// more than one channel on, bit 30 always set, bits 28-29 kept from the shadow.
pub fn compress(sh: u32, more_than_one_channel: bool, path: u8, value: u32) -> (u32, u32) {
    let reg = if path == 0 {
        COMPRESS_MAIN
    } else {
        COMPRESS_ZOOM
    };
    let v = (value & 0x0fff_ffff)
        | (sh & 0x3000_0000)
        | 0x4000_0000
        | (u32::from(more_than_one_channel) << 31);
    (reg, v)
}

/// `DevAcquireSPU_SetLinearInterp(enable, path, num, den)`: phase step
/// `(den << 32 - den) / num` when enabled, `0xffffffff` when not; the enable bit for the path is
/// `!enable & 1` (bit 0 main, bit 1 zoom), the other bit kept.
pub fn linear_interp(sh: u32, enable: bool, path: u8, num: u32, den: u32) -> [(u32, u32); 3] {
    let bit = if path == 0 { 1u32 << 0 } else { 1u32 << 1 };
    let en = if enable { 0 } else { bit } | (sh & !bit);
    let step = if enable && num != 0 {
        (u64::from(den) << 32).wrapping_sub(u64::from(den)) as u64 / u64::from(num)
    } else {
        0xffff_ffff
    } as u32;
    let (off, stepreg) = if path == 0 {
        (INTERP_MAIN[0], INTERP_MAIN[1])
    } else {
        (INTERP_ZOOM[0], INTERP_ZOOM[1])
    };
    [(INTERP_EN, en), (off, 0), (stepreg, step)]
}

/// The read-out fix for a window that never completes: compression and interpolation off on both
/// paths (`record-play.md` 2, the same chain P in this FPGA).
pub fn passthrough() -> Vec<(u32, u32)> {
    vec![
        (COMPRESS_MAIN, 0x4000_0001),
        (COMPRESS_ZOOM, 0x4000_0001),
        (INTERP_EN, 0x3),
        (INTERP_MAIN[0], 0),
        (INTERP_MAIN[1], 0xffff_ffff),
        (INTERP_ZOOM[0], 0),
        (INTERP_ZOOM[1], 0xffff_ffff),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compression_word_packs_channel_bit_and_flags() {
        assert_eq!(compress(0, false, 0, 1), (COMPRESS_MAIN, 0x4000_0001));
        assert_eq!(compress(0, true, 0, 4), (COMPRESS_MAIN, 0xc000_0004));
        let (r, v) = compress(0x3000_0000, false, 1, 0x1234);
        assert_eq!(r, COMPRESS_ZOOM);
        assert_eq!(v, 0x7000_1234, "shadow bits 28-29 kept");
    }

    #[test]
    fn interpolation_step_matches_the_formula() {
        // enable main, num=2 den=1 -> ((1<<32)-1)/2 = 0x7fffffff
        let w = linear_interp(0, true, 0, 2, 1);
        assert_eq!(w[0], (INTERP_EN, 0x0));
        assert_eq!(w[1], (INTERP_MAIN[0], 0));
        assert_eq!(w[2], (INTERP_MAIN[1], 0x7fff_ffff));
    }

    #[test]
    fn disabled_interpolation_sets_the_path_bit_and_max_step() {
        // path 1 (zoom) disabled: bit 1 set, step 0xffffffff
        let w = linear_interp(0, false, 1, 4, 1);
        assert_eq!(w[0], (INTERP_EN, 0b10));
        assert_eq!(w[1], (INTERP_ZOOM[0], 0));
        assert_eq!(w[2], (INTERP_ZOOM[1], 0xffff_ffff));
    }

    #[test]
    fn passthrough_turns_both_paths_off() {
        let w = passthrough();
        assert_eq!(w[0], (COMPRESS_MAIN, 0x4000_0001));
        assert_eq!(w[1], (COMPRESS_ZOOM, 0x4000_0001));
        assert_eq!(w[2], (INTERP_EN, 0x3));
        assert_eq!(w.len(), 7);
    }
}
