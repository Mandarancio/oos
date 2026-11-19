// Copyright (C) 2026 Julian Decker
// SPDX-License-Identifier: GPL-2.0-or-later

//! DHO1000 zone trigger and mask output, reconstructed from the stock library.
//!
//! Evidence: `DevAcquireSPU_SetZoneTrigCtrl` @0x2b9fd8, `DevAcquireSPU_SetMaskOutput` @0x2b9d9c,
//! `DevSystemSCU_SetMaskZoneTrig` @0x2ba080, and the compare-table builder
//! `DrvZoneTrigger_makeSingleTab` @0x3053a8 with `DevAcquireSPU_SetZoneTab1/2` @0x2ba000
//! (decompiled in `docs/decomp/libscope`); the `zoneChanMap` table was recovered from the stock
//! binary (file 0x00c12950). Not verified on hardware.
//!
//! The two zones share one rectangle definition (time span + vertical limits, `SetZone1/2`
//! @0x304bfc/@0x304e2c write the same settings block); each table slot picks its own channel.
//! The table has 1000 entries whose columns are decimated across the interleaved ADC banks:
//! `col = i / (4 / chan_count)` and `bank = base + (i * step) mod 4` from `ZONE_CHAN_MAP`.
//! The dual-zone geometry (`makeDoubleTab` @0x204e7c) is not decoded.

use crate::dho1000::Write;

/// Zone trigger: bits 24-27 of the SPU zone word (`0x11d0`), one bit per function.
pub const ZONE_CTRL: u32 = 0x11d0;
/// The two compare tables (`SetZoneTab1` writes `0x11d8`, `SetZoneTab2` `0x11dc`).
pub const TABLE: [u32; 2] = [0x11d8, 0x11dc];
/// The stock system always fills this many table entries.
pub const COLUMNS: usize = 1000;
/// Mask output: `(index * 4 + 4)` in bits 2-23, enables in bits 29-31 (`0x119c`).
pub const MASK_OUTPUT: u32 = 0x119c;
/// SCU function control (`0x4020`): mask/zone trigger enables in bits 12-15, bit 1.
pub const SCU_CTRL: u32 = 0x4020;

/// The stock `zoneChanMap` table, recovered from the binary: 44 rows of
/// (sample channel mask, table-1 channel, table-2 channel, base 1, base 2, step). The builder
/// looks up the row whose mask and both channels match and takes the bases and the step from it.
const ZONE_CHAN_MAP: [(u8, u8, u8, u32, u32, u32); 44] = [
    (1, 1, 1, 0, 0, 1),
    (2, 2, 2, 0, 0, 1),
    (4, 3, 3, 0, 0, 1),
    (8, 4, 4, 0, 0, 1),
    (3, 1, 2, 0, 1, 2),
    (3, 2, 1, 1, 0, 2),
    (5, 1, 3, 0, 1, 2),
    (5, 3, 1, 1, 0, 2),
    (9, 1, 4, 0, 1, 2),
    (9, 4, 1, 1, 0, 2),
    (6, 2, 3, 0, 1, 2),
    (6, 3, 2, 1, 0, 2),
    (10, 2, 4, 0, 1, 2),
    (10, 4, 2, 1, 0, 2),
    (12, 3, 4, 0, 1, 2),
    (12, 4, 3, 1, 0, 2),
    (3, 1, 1, 0, 0, 2),
    (3, 2, 2, 1, 1, 2),
    (5, 1, 1, 0, 0, 2),
    (5, 3, 3, 1, 1, 2),
    (9, 1, 1, 0, 0, 2),
    (9, 4, 4, 1, 1, 2),
    (6, 2, 2, 0, 0, 2),
    (6, 3, 3, 1, 1, 2),
    (10, 2, 2, 0, 0, 2),
    (10, 4, 4, 1, 1, 2),
    (12, 3, 3, 0, 0, 2),
    (12, 4, 4, 1, 1, 2),
    (15, 1, 1, 0, 0, 4),
    (15, 2, 2, 1, 1, 4),
    (15, 3, 3, 2, 2, 4),
    (15, 4, 4, 3, 3, 4),
    (15, 1, 2, 0, 1, 4),
    (15, 2, 1, 1, 0, 4),
    (15, 1, 3, 0, 2, 4),
    (15, 3, 1, 2, 0, 4),
    (15, 1, 4, 0, 3, 4),
    (15, 4, 1, 3, 0, 4),
    (15, 2, 3, 1, 2, 4),
    (15, 3, 2, 2, 1, 4),
    (15, 2, 4, 1, 3, 4),
    (15, 4, 2, 3, 1, 4),
    (15, 3, 4, 2, 3, 4),
    (15, 4, 3, 3, 2, 4),
];

/// The `zoneChanMap` lookup of `makeSingleTab`: the row for a channel mask and the two table
/// channels, as (base 1, base 2, step). A miss returns zeros, as the stock code does.
pub fn chan_map(mask: u8, tab1_ch: u8, tab2_ch: u8) -> (u32, u32, u32) {
    for &(m, c1, c2, b1, b2, s) in &ZONE_CHAN_MAP {
        if m == mask && c1 == tab1_ch && c2 == tab2_ch {
            return (b1, b2, s);
        }
    }
    (0, 0, 0)
}

/// The vertical code of a zone limit (`makeSingleTab`): `(limit + offset) * 7500 / scale`, the
/// 16-bit result sign-flipped (`& 0xffff ^ 0x8000`); the table word keeps the low byte.
pub fn zone_code(limit_nv: f64, offset_nv: f64, scale_nv: f64) -> u16 {
    let c = ((limit_nv + offset_nv) * 7500.0 / scale_nv) as i32 as u16;
    c ^ 0x8000
}

/// One table word (`DevAcquireSPU_SetZoneTab1` @0x2ba000): bank bits 28-31, column 18-27,
/// in-zone bit 16, upper limit 8-15, lower limit 0-7; bit 17 of `sh` kept.
pub fn tab_word(sh: u32, bank: u8, column: u32, in_zone: bool, lower: u16, upper: u16) -> Write {
    (
        TABLE[0],
        (u32::from(bank) << 28)
            | ((column & 0x3ff) << 18)
            | u32::from(in_zone) << 16
            | (u32::from(upper) & 0xff) << 8
            | u32::from(lower) & 0xff
            | (sh & 0x20000),
    )
}

/// The zone rectangle: a time span in table columns and the two vertical limit codes.
#[derive(Clone, Copy, Debug)]
pub struct Rect {
    /// First and one-past-last in-zone column (`makeSingleTab` marks `rate * t1 + 1 .. rate * t2`).
    pub col_range: (usize, usize),
    pub lower: u16,
    pub upper: u16,
}

/// The time span of a zone in table columns: the sample rate times the span seconds, the start
/// one column late exactly as the stock fill loop does.
pub fn span_cols(rate_hz: u64, t_start_s: f64, t_stop_s: f64) -> (usize, usize) {
    let c0 = (rate_hz as f64 * t_start_s) as usize + 1;
    let c1 = (rate_hz as f64 * t_stop_s) as usize;
    (c0, c1)
}

/// The single-zone table upload (`DrvZoneTrigger_makeSingleTab` @0x3053a8): 1000 words per
/// enabled table slot. `chan_count` is the number of sampled channels (1, 2 or 4; 3 is treated
/// as 4, as the stock code does); `mask` the sample channel mask; each slot carries its channel
/// (for the bank lookup) and its rectangle.
pub fn make_single_tab(
    chan_count: usize,
    mask: u8,
    tab1_ch: u8,
    tab2_ch: u8,
    tab1: Option<Rect>,
    tab2: Option<Rect>,
) -> Vec<Write> {
    let div = match chan_count {
        1 => 4,
        2 => 2,
        _ => 1,
    };
    let (base1, base2, step) = chan_map(mask, tab1_ch, tab2_ch);
    let mut w = Vec::with_capacity(COLUMNS * 2);
    let mut bank = 0u32;
    for i in 0..COLUMNS {
        let col = (i / div) as u32;
        if let Some(r) = tab1 {
            let in_zone = i >= r.col_range.0 && i < r.col_range.1;
            w.push((
                TABLE[0],
                tab_word(0, (bank.wrapping_add(base1)) as u8, col, in_zone, r.lower, r.upper).1,
            ));
        }
        if let Some(r) = tab2 {
            let in_zone = i >= r.col_range.0 && i < r.col_range.1;
            w.push((
                TABLE[1],
                tab_word(0, (bank.wrapping_add(base2)) as u8, col, in_zone, r.lower, r.upper).1,
            ));
        }
        bank = (bank + step) & 3;
    }
    w
}

/// `DevAcquireSPU_SetZoneTrigCtrl(a, b, c, d)`: four enables in bits 24-27.
pub fn zone_trig_ctrl(a: bool, b: bool, c: bool, d: bool) -> (u32, u32) {
    (
        ZONE_CTRL,
        (u32::from(a) << 24) | (u32::from(b) << 25) | (u32::from(c) << 26) | (u32::from(d) << 27),
    )
}

/// `DevAcquireSPU_SetMaskOutput(index, a, b, c)`: index in bits 2-23, enables in bits 29-31; the
/// shadow's bits 24-28 are kept.
pub fn mask_output(sh: u32, index: u32, a: bool, b: bool, c: bool) -> (u32, u32) {
    (
        MASK_OUTPUT,
        (index.wrapping_mul(4).wrapping_add(4) & 0x00ff_fffc)
            | (u32::from(a) << 29)
            | (u32::from(b) << 30)
            | (u32::from(c) << 31)
            | (sh & 0x1f00_0000),
    )
}

/// `DevSystemSCU_SetMaskZoneTrig(a, b, c, d, e)`: bits 12-15 and bit 1 of `0x4020`, the rest kept.
pub fn mask_zone_trig(sh: u32, a: bool, b: bool, c: bool, d: bool, e: bool) -> (u32, u32) {
    let v = (u32::from(a) << 15)
        | (u32::from(b) << 14)
        | (u32::from(c) << 13)
        | (u32::from(d) << 12)
        | (u32::from(e) << 1)
        | (sh & 0xffd);
    (SCU_CTRL, v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zone_bits_are_one_per_function() {
        assert_eq!(
            zone_trig_ctrl(true, false, false, false),
            (ZONE_CTRL, 1 << 24)
        );
        assert_eq!(
            zone_trig_ctrl(false, true, true, false),
            (ZONE_CTRL, (1 << 25) | (1 << 26))
        );
        assert_eq!(
            zone_trig_ctrl(true, true, true, true),
            (ZONE_CTRL, 0x0f00_0000)
        );
    }

    #[test]
    fn mask_output_packs_index_and_enables() {
        assert_eq!(
            mask_output(0, 0, true, false, false),
            (MASK_OUTPUT, 4 | (1 << 29))
        );
        let (_, v) = mask_output(0x1f00_0000, 3, true, true, true);
        assert_eq!(v & 0x00ff_fffc, 16, "index 3 -> 3*4+4");
        assert_eq!(v & 0xe000_0000, 0xe000_0000, "three enables");
        assert_eq!(v & 0x1f00_0000, 0x1f00_0000, "shadow bits kept");
    }

    #[test]
    fn mask_zone_trigger_sets_scu_bits_and_keeps_the_rest() {
        let (r, v) = mask_zone_trig(0x0ffd, true, false, true, false, true);
        assert_eq!(r, SCU_CTRL);
        assert_eq!(v & 0xf000, (1 << 15) | (1 << 13));
        assert_eq!(v & 0x2, 0x2, "bit 1");
        assert_eq!(v & 0xffd, 0xffd, "other bits kept");
    }

    #[test]
    fn chan_map_matches_the_recovered_rows() {
        // one channel on, both tables on it: base 0, step 1
        assert_eq!(chan_map(1, 1, 1), (0, 0, 1));
        // all four channels, tables on 2 and 4: bases 1 and 3, step 4
        assert_eq!(chan_map(15, 2, 4), (1, 3, 4));
        // a miss returns zeros, as the stock lookup does
        assert_eq!(chan_map(15, 1, 1), (0, 0, 4));
        assert_eq!(chan_map(7, 1, 1), (0, 0, 0), "mask 7 is not in the table");
    }

    #[test]
    fn zone_codes_flip_the_sign_bit() {
        // 1 V/div, no offset, limit 0 -> mid code flipped
        assert_eq!(zone_code(0.0, 0.0, 1e9), 0x8000);
        // half a division up: 7500 codes, flipped to 0x61d4-ish negative range
        let c = zone_code(500e6, 0.0, 1e9);
        assert_eq!(c, (3750u16) ^ 0x8000);
    }

    #[test]
    fn tab_words_pack_bank_column_limits() {
        let (r, v) = tab_word(0x20000, 3, 999, true, 0x1234, 0x5678);
        assert_eq!(r, TABLE[0]);
        assert_eq!(
            v,
            3 << 28 | 999 << 18 | 1 << 16 | 0x78 << 8 | 0x34 | 0x20000
        );
    }

    #[test]
    fn span_cols_start_one_column_late() {
        assert_eq!(span_cols(1000, 0.0, 0.5), (1, 500));
        assert_eq!(span_cols(1000, 0.25, 0.75), (251, 750));
    }

    #[test]
    fn single_tab_decimates_columns_across_banks() {
        let rect = Rect {
            col_range: (0, 500),
            lower: 0x80,
            upper: 0x7f,
        };
        let w = make_single_tab(4, 15, 2, 2, Some(rect), None);
        assert_eq!(w.len(), COLUMNS);
        // 4 channels: col = i, bank stays at base 1
        assert_eq!(w[0], (TABLE[0], 1 << 28 | 1 << 16 | 0x7f80));
        assert_eq!(w[499], (TABLE[0], 1 << 28 | 499 << 18 | 1 << 16 | 0x7f80));
        assert_eq!(w[500], (TABLE[0], 1 << 28 | 500 << 18 | 0x7f80), "out of the zone, limits still written");
        // one channel: col = i / 4, bank cycles 0-3
        let w = make_single_tab(1, 1, 1, 1, Some(rect), None);
        assert_eq!(w[0].1 & 0x3ff << 18 >> 0, 0);
        assert_eq!(w[1].1 >> 28, 1, "step 1: next bank");
        assert_eq!(w[4].1 >> 18 & 0x3ff, 1, "column 4 / 4");
    }

    #[test]
    fn both_tables_upload_when_both_zones_are_on() {
        let rect = Rect {
            col_range: (10, 20),
            lower: 1,
            upper: 2,
        };
        let w = make_single_tab(2, 3, 1, 2, Some(rect), Some(rect));
        assert_eq!(w.len(), 2 * COLUMNS);
        assert_eq!(w.iter().filter(|x| x.0 == TABLE[0]).count(), COLUMNS);
        assert_eq!(w.iter().filter(|x| x.0 == TABLE[1]).count(), COLUMNS);
        // mask 3, channels 1 and 2: bases 0 and 1, step 2
        let t1 = w.iter().find(|x| x.0 == TABLE[0]).unwrap().1;
        let t2 = w.iter().find(|x| x.0 == TABLE[1]).unwrap().1;
        assert_eq!(t1 >> 28, 0);
        assert_eq!(t2 >> 28, 1);
    }
}
