// Copyright (C) 2026 Julian Decker
// SPDX-License-Identifier: GPL-2.0-or-later

//! DHO1000 hardware frequency counter, reconstructed from the stock library.
//!
//! Evidence: `libscope.so` decompiled (`docs/decomp/libscope/full-decomp`), function addresses in
//! the doc comments. The counter is a reciprocal counter on one channel's trigger comparator with
//! a 500 ns reference tick; the DHO800/900 counterpart is `crate::dho::counter`.
//! **Nothing here has run on hardware yet.**
//!
//! Register map (0x2200 block):
//!
//! | Register | Use |
//! |---|---|
//! | `INIT` / `COMPARE` | init and compare values (both 0x8080 in the stock code) |
//! | `SOURCE` / `CH_EN` | source enable bit 0, per-channel enable nibble |
//! | `CFG` | mode, gate, control bits |
//! | `GATE` | raw gate value |
//! | `LATCH` | write 1 to snapshot the results |
//! | `START`/`END`/`NUM`/`PERIOD`/`EVENTS` | five 64-bit results, low word first |

use crate::dho1000::Write;

pub const INIT: u32 = 0x2200;
pub const COMPARE: u32 = 0x2204;
pub const SOURCE: u32 = 0x2208;
pub const CH_EN: u32 = 0x220c;
/// Mode (bits 5-9), gate mode (bit 1), one flag (bit 4) of the counter configuration
/// (`SetCntMode` @0x2b8ccc); `SetCntGate` @0x2b8c78 adds the gate time code (bits 10-14) and a
/// second flag (bit 3); `SetCntCtrl` @0x2b8c48 sets bits 0 and 2.
pub const CFG: u32 = 0x2210;
pub const GATE: u32 = 0x2214;
/// Read latch: write 1 to snapshot the results (`GetCntRdVld` @0x2b8d08).
pub const LATCH: u32 = 0x2218;
pub const START: [u32; 2] = [0x221c, 0x2220];
pub const END: [u32; 2] = [0x2224, 0x2228];
pub const NUM: [u32; 2] = [0x222c, 0x2230];
pub const PERIOD: [u32; 2] = [0x2234, 0x2238];
pub const EVENTS: [u32; 2] = [0x223c, 0x2240];

/// The reference tick of the reciprocal counter: 500 ns (2 MHz), the unit the stock formula
/// converts the gate into (`DrvCounter_GetValue` @0x306cd4 divides by 500000 and multiplies back).
pub const TICK_FS: u64 = 500_000;

/// `DevAnalyzeCnt_SetCntInit` @0x2b8bdc: init and compare words, the source enable bit and the
/// per-channel enable nibble (the rest of `sh` kept).
pub fn set_init(sh: u32, source_on: bool, ch: usize) -> [Write; 4] {
    [
        (INIT, 0x8080),
        (COMPARE, 0x8080),
        (SOURCE, u32::from(source_on)),
        (CH_EN, (sh & 0xf0) | (1u32 << (ch - 1) & 0xf)),
    ]
}

/// `DevSystemTPU`-neighbouring `DevAnalyzeCnt_SetCntMode` @0x2b8ccc: mode in bits 5-9, two
/// single-bit settings in bits 1 and 4.
pub fn set_mode(sh: u32, mode: u8, p2: bool, p3: bool) -> Write {
    (
        CFG,
        (u32::from(mode) & 0x1f) << 5
            | u32::from(p2) << 1
            | u32::from(p3) << 4
            | (sh & 0xfc0d),
    )
}

/// `DevAnalyzeCnt_SetCntGate` @0x2b8c78: gate time code in bits 10-14, one flag in bit 3, and
/// the raw gate value in `GATE`.
pub fn set_gate(sh: u32, code: u8, p2: bool, value: u32) -> [Write; 2] {
    [
        (
            CFG,
            (u32::from(code) & 0x1f) << 10 | u32::from(p2) << 3 | (sh & 0x83f7),
        ),
        (GATE, value),
    ]
}

/// `DevAnalyzeCnt_SetCntCtrl` @0x2b8c48: bits 0 and 2.
pub fn set_ctrl(sh: u32, run: bool, clear: bool) -> Write {
    (CFG, u32::from(run) | u32::from(clear) << 2 | (sh & 0xfffa))
}

/// The counter results after a `LATCH` snapshot, as the five 64-bit registers give them.
#[derive(Clone, Copy, Debug, Default)]
pub struct Results {
    /// First edge timestamp (bit 63 of the high word is the valid flag).
    pub start: u64,
    /// Last edge timestamp.
    pub end: u64,
    /// Number of edges in the gate.
    pub num: u64,
    /// Single-period measurement (bit 63 of the high word is the valid flag).
    pub period: u64,
    /// Total event count.
    pub events: u64,
}

impl Results {
    /// Rebuild a result word from its low and high register halves (the valid flags in bit 63
    /// are masked off, as the stock getters do).
    pub fn word(lo: u32, hi: u32) -> u64 {
        (u64::from(hi) << 32 | u64::from(lo)) & 0x7fff_ffff_ffff_ffff
    }

    pub fn freq_valid(&self) -> bool {
        self.start >> 63 == 0 && self.num >= 2
    }

    pub fn period_valid(&self) -> bool {
        self.period >> 63 == 0
    }
}

/// The measurement mode of `DrvCounter_GetValue` @0x306cd4.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Frequency,
    Period,
    Totalize,
}

/// `DrvCounter_GetValue` @0x306cd4: the reciprocal-counter value in Hz (frequency), seconds
/// (period) or events (totalize). `gate_fs` is the gate time in femtoseconds. Returns `None` for
/// the stock error case (a period below 100 ticks with no valid measurement).
///
/// The reciprocal path measures `((gate/TICK - start - end) / max(num - 1, 1)) * TICK` in
/// femtoseconds; below two edges it falls back to the single-period word.
pub fn value(mode: Mode, gate_fs: u64, r: &Results) -> Option<f64> {
    let period_fs = match mode {
        Mode::Totalize => return Some(r.events as f64),
        _ => {
            if r.freq_valid() {
                let gate_ticks = gate_fs / TICK_FS;
                let n = (r.num - 1).max(1);
                ((gate_ticks.wrapping_sub(r.start).wrapping_sub(r.end)) / n) as f64
                    * TICK_FS as f64
            } else if r.period_valid() && r.period > 0 {
                r.period as f64
            } else {
                return None;
            }
        }
    };
    if period_fs < 100.0 {
        return None;
    }
    match mode {
        Mode::Frequency => Some(1.0 / (period_fs * 1e-15)),
        Mode::Period => Some(period_fs * 1e-15),
        Mode::Totalize => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_writes_the_stock_constants_and_channel_bit() {
        let w = set_init(0x30, true, 2);
        assert_eq!(w[0], (INIT, 0x8080));
        assert_eq!(w[1], (COMPARE, 0x8080));
        assert_eq!(w[2], (SOURCE, 1));
        assert_eq!(w[3], (CH_EN, 0x32), "bit 1 set, nibble rest kept");
        let w = set_init(0, false, 4);
        assert_eq!(w[2], (SOURCE, 0));
        assert_eq!(w[3], (CH_EN, 8));
    }

    #[test]
    fn mode_gate_and_ctrl_pack_their_bits() {
        assert_eq!(set_mode(0, 3, true, false), (CFG, 3 << 5 | 1 << 1));
        assert_eq!(set_gate(0, 7, true, 1234), [(CFG, 7 << 10 | 1 << 3), (GATE, 1234)]);
        assert_eq!(set_ctrl(0, true, true), (CFG, 1 | 4));
    }

    #[test]
    fn words_mask_the_valid_flags() {
        assert_eq!(Results::word(0xffff_ffff, 0x7fff_ffff), u64::MAX >> 1);
        assert_eq!(Results::word(1, 0x8000_0000), 1, "the valid bit is not part of the value");
    }

    #[test]
    fn reciprocal_path_divides_the_gate_by_the_edge_count() {
        // 1 s gate with 1001 edges: the gate span over 1000 intervals is exactly 1 ms -> 1 kHz
        let r = Results {
            start: 0,
            end: 0,
            num: 1001,
            period: 0,
            events: 1001,
        };
        let khz = value(Mode::Frequency, 1_000_000_000_000_000, &r).unwrap();
        assert!((khz - 1000.0).abs() < 1.0, "got {khz} Hz");
        let s = value(Mode::Period, 1_000_000_000_000_000, &r).unwrap();
        assert!((s - 1e-3).abs() < 1e-9, "got {s} s");
    }

    #[test]
    fn single_period_fallback_below_two_edges() {
        let r = Results {
            start: 0,
            end: 0,
            num: 1,
            period: 2_000_000_000, // 2 us in fs
            events: 1,
        };
        assert!(!r.freq_valid());
        assert!(r.period_valid());
        let hz = value(Mode::Frequency, 1_000_000_000_000_000, &r).unwrap();
        assert!((hz - 500_000.0).abs() < 1.0);
    }

    #[test]
    fn totalize_returns_the_event_count_and_short_periods_fail() {
        let r = Results {
            start: 0,
            end: 0,
            num: 0,
            period: 0,
            events: 42,
        };
        assert_eq!(value(Mode::Totalize, 0, &r), Some(42.0));
        let r = Results {
            period: 50 * TICK_FS, // no valid flags set
            ..r
        };
        assert_eq!(value(Mode::Period, 0, &r), Some(50.0 * TICK_FS as f64 * 1e-15));
    }
}
