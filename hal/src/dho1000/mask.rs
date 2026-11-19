// Copyright (C) 2026 Julian Decker
// SPDX-License-Identifier: GPL-2.0-or-later

//! DHO1000 hardware mask/pass-fail unit, reconstructed from the stock library.
//!
//! Evidence: `libscope.so` decompiled (`docs/decomp/libscope/full-decomp`), function addresses in
//! the doc comments. The unit compares the compressed sample stream of one channel against a
//! 1000-column table of 8-bit limits and counts tested frames and per-channel fails; the
//! DHO800/900 counterpart is `crate::dho::mask`. **Nothing here has run on hardware yet.**
//!
//! Register map:
//!
//! | Register | Use |
//! |---|---|
//! | `CTRL` | latch bit 28, clear-fails bit 29, clear-frames bit 30, channel nibble 24-27 |
//! | `TABLE` | one column per write: column 16-25, upper limit 8-15, lower limit 0-7, bank 26-31 |
//! | `COMPRESS` | samples per column, bits 0-27, enable bit 31 |
//! | `FRAMES` | tested-frame count, 64-bit |
//! | `CROSS[4]` | per-channel fail counts, 64-bit each |
//!
//! The SCU side (`0x4020`) carries the enable (bit 15), save-pass (14), save-fail (13) and
//! stop-on-fail (12) bits — the same word `zone::mask_zone_trig` writes.

use crate::dho1000::zone::SCU_CTRL;
use crate::dho1000::Write;

/// The stock system always fills this many columns, spread over the whole record.
pub const COLUMNS: usize = 1000;

pub const CTRL: u32 = 0x1190;
pub const TABLE: u32 = 0x1194;
pub const COMPRESS: u32 = 0x1184;
pub const FRAMES: [u32; 2] = [0x11a0, 0x11a4];
/// Per-channel fail ("cross") counts (`GetMaskCrossCnt` @0x2b9ec0).
pub const CROSS: [[u32; 2]; 4] = [
    [0x11b0, 0x11b4],
    [0x11b8, 0x11bc],
    [0x11c0, 0x11c4],
    [0x11c8, 0x11cc],
];

/// `DevAcquireSPU_SetMaskCtrl` @0x2b9b38: clear-frames bit 30, clear-fails bit 29, latch bit 28,
/// the channel nibble in bits 24-27.
pub fn ctrl(sh: u32, clear_frames: bool, clear_fails: bool, latch: bool, ch: usize) -> Write {
    (
        CTRL,
        u32::from(clear_frames) << 30
            | u32::from(clear_fails) << 29
            | u32::from(latch) << 28
            | ((ch as u32) & 0xf) << 24
            | (sh & 0x870f_ffff),
    )
}

/// `DevAcquireSPU_ClearSum` @0x2b9bdc: bits 29 and 30 set, then cleared — resets both counters.
pub fn clear_sum(sh: u32) -> [Write; 2] {
    [(CTRL, sh | 0x6000_0000), (CTRL, sh & 0x9fff_ffff)]
}

/// `DevSystemSCU_queryMaskReport` @0x2ba320: the latch pulse that copies the counters into the
/// readable registers — bit 28 cleared, set, cleared.
pub fn latch(sh: u32) -> [Write; 3] {
    [
        (CTRL, sh & 0xefff_ffff),
        (CTRL, sh | 0x1000_0000),
        (CTRL, sh & 0xefff_ffff),
    ]
}

/// `DevAcquireSPU_SetMaskCompress` @0x2b9b24: samples per column in bits 0-27, enable bit 31
/// (the stock setter keeps no other field).
pub fn compress(on: bool, samples_per_column: u32) -> Write {
    (
        COMPRESS,
        (samples_per_column & 0x0fff_ffff) | u32::from(on) << 31 | 0x4000_0000,
    )
}

/// `DevAcquireSPU_SelectTabPoint` @0x2b9d5c: bank in bits 28-31, column in 16-25.
pub fn select_point(sh: u32, bank: u8, column: u32) -> Write {
    (TABLE, (u32::from(bank) << 28) | ((column & 0x3ff) << 16) | (sh & 0x0c00_ffff))
}

/// One column word of the table: `column << 16 | lower | upper << 8 | 0xfc000000`
/// (`DevSystemSCU_setMaskRule` @0x2ba2a8 writes 1000 of them; the fixed top byte is the
/// write strobe with all banks enabled).
pub fn column_word(column: usize, lower: u8, upper: u8) -> u32 {
    ((column as u32) << 16) | u32::from(lower) | (u32::from(upper) << 8) | 0xfc00_0000
}

/// The stock 16-to-8 bit conversion of a limit (`DrvMask_setMaskRule` @0x30a938):
/// `(v + 0xff) >> 8` for positive values (round half up), a plain shift for negative ones.
pub fn limit_byte(v: i32) -> u8 {
    let rounded = if v >= 0 { v + 0xff } else { v };
    ((rounded >> 8) & 0xff) as u8
}

/// The table upload: one write per column, the column field counting up by itself
/// (`setMaskRule` writes the same register 1000 times).
pub fn table(lower: &[u8; COLUMNS], upper: &[u8; COLUMNS]) -> Vec<Write> {
    let mut w = Vec::with_capacity(COLUMNS);
    for i in 0..COLUMNS {
        w.push((TABLE, column_word(i, lower[i], upper[i])));
    }
    w
}

/// The SCU mask bits (`SetMaskEn` @0x2ba0cc bit 15, `SetMaskSavePass` @0x2ba0ec bit 14,
/// `SetMaskSaveFail` @0x2ba114 bit 13; bit 12 is the stop-on-fail the zone code shares).
pub fn scu(sh: u32, enable: bool, save_pass: bool, save_fail: bool, stop_on_fail: bool) -> Write {
    (
        SCU_CTRL,
        u32::from(enable) << 15
            | u32::from(save_pass) << 14
            | u32::from(save_fail) << 13
            | u32::from(stop_on_fail) << 12
            | (sh & 0x0fff),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctrl_packs_the_flags_and_channel() {
        let (_, v) = ctrl(0x870f_ffff, true, false, true, 3);
        assert_eq!(v, 0x870f_ffff | 1 << 30 | 1 << 28 | 3 << 24);
        assert_eq!(ctrl(0, false, false, false, 1).1, 1 << 24);
    }

    #[test]
    fn clear_sum_and_latch_pulse_their_bits() {
        assert_eq!(clear_sum(0), [(CTRL, 0x6000_0000), (CTRL, 0)]);
        assert_eq!(
            latch(0x1000_0000),
            [
                (CTRL, 0),
                (CTRL, 0x1000_0000),
                (CTRL, 0),
            ],
            "the shadow's other bits ride along"
        );
    }

    #[test]
    fn compress_and_select_point_pack_their_fields() {
        assert_eq!(compress(true, 1023), (COMPRESS, 1023 | 1 << 31 | 0x4000_0000));
        assert_eq!(select_point(0, 3, 999), (TABLE, 3 << 28 | 999 << 16));
    }

    #[test]
    fn column_words_count_the_column_and_strobe() {
        assert_eq!(column_word(0, 0x12, 0x34), 0xfc00_0000 | 0x3412);
        assert_eq!(column_word(999, 0, 0xff), 0xfc00_0000 | 999 << 16 | 0xff << 8);
    }

    #[test]
    fn limit_bytes_round_half_up_for_positive_values() {
        assert_eq!(limit_byte(0), 0);
        assert_eq!(limit_byte(255), 1, "round half up: 255 + 255 = 510 >> 8");
        assert_eq!(limit_byte(256), 1);
        assert_eq!(limit_byte(511), 2, "511 + 255 = 766, >> 8 = 2");
        assert_eq!(limit_byte(-1), 0xff, "negative values shift as they are");
        assert_eq!(limit_byte(-256), 0xff);
    }

    #[test]
    fn table_writes_one_word_per_column() {
        let mut lo = [0u8; COLUMNS];
        let mut hi = [0u8; COLUMNS];
        lo[999] = 1;
        hi[999] = 2;
        let w = table(&lo, &hi);
        assert_eq!(w.len(), COLUMNS);
        assert_eq!(w[0], (TABLE, 0xfc00_0000));
        assert_eq!(w[999], (TABLE, 0xfc00_0000 | 999 << 16 | 1 | 2 << 8));
    }

    #[test]
    fn scu_bits_match_the_stock_setters() {
        let (_, v) = scu(0x0fff, true, true, true, true);
        assert_eq!(v, 0xf000 | 0x0fff);
        assert_eq!(scu(0, false, false, false, false).1, 0);
    }
}
