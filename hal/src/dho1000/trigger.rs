// Copyright (C) 2026 Julian Decker
// SPDX-License-Identifier: GPL-2.0-or-later

//! DHO1000 trigger block (TPU), reconstructed from the stock library.
//!
//! Evidence: `libscope.so` decompiled (`docs/decomp/libscope/full-decomp`), function addresses in
//! the doc comments. Every setter below mirrors one decompiled function one-to-one; the composed
//! `edge()` returns the writes the stock code performs for an analog edge trigger. **Nothing here
//! has run on hardware yet**: the sequences are the stock software's, not a measurement.
//!
//! Register map (TPU window 0x2000..0x22ff):
//!
//! | Register | Use |
//! |---|---|
//! | `LEVEL_A[4]` / `LEVEL_B[4]` | per-channel thresholds, `lo \| hi << 16` |
//! | `SOURCE_A` / `SOURCE_B` / `SOURCE_R` | comparator source fields + event bits |
//! | `TIME_A[3]` / `TIME_B[3]` / `TIME_R[3]` | trigger A/B/R times |
//! | `LOGIC_A[2]` / `LOGIC_B[2]` / `LOGIC_R[2]` | logic operations |
//! | `MODE` | trigger mode / force / ABR FSM / SPU bypass |
//! | `HOLD_EVENT` / `HOLD_TIME[2]` | holdoff event number, time and type |
//! | `IIR` / `DELAY[2]` / `R_TIMEOUT[2]` / `EXT_HYS` | filters, delay, timeout, EXT hysteresis |

/// Per-channel trigger-A thresholds, from the 4-entry table the stock code indexes with the
/// channel number (`DevSystemTPU_SetLevelA` @0x2af4c8; table at file 0x00a0af20).
pub const LEVEL_A: [u32; 4] = [0x2004, 0x200c, 0x20b8, 0x20c0];
/// Per-channel trigger-B thresholds (`DevSystemTPU_SetLevelB` @0x2af4f0; table at 0x00a0af30).
pub const LEVEL_B: [u32; 4] = [0x2008, 0x2010, 0x20bc, 0x20c4];

pub const SOURCE_A: u32 = 0x201c;
pub const SOURCE_B: u32 = 0x2034;
pub const SOURCE_R: u32 = 0x204c;
pub const TIME_A: [u32; 3] = [0x2020, 0x2024, 0x2028];
pub const TIME_B: [u32; 3] = [0x2038, 0x203c, 0x2040];
pub const TIME_R: [u32; 3] = [0x2050, 0x2054, 0x2058];
pub const LOGIC_A: [u32; 2] = [0x2014, 0x2018];
pub const LOGIC_B: [u32; 2] = [0x202c, 0x2030];
pub const LOGIC_R: [u32; 2] = [0x2044, 0x2048];
/// Trigger mode (bits 0-4), force, ABR FSM, SPU bypass (`SetTriggerMode` @0x2afce0 and the
/// `SetTriggerForce`/`SetTriggerABRFSM`/`SetTrigSPUBypass` siblings share this register).
pub const MODE: u32 = 0x205c;
pub const DELAY: [u32; 2] = [0x2060, 0x2064];
pub const R_TIMEOUT: [u32; 2] = [0x2068, 0x206c];
/// Number of the A and B events the ABR machine waits for.
pub const ABR_EVENT_A: u32 = 0x2070;
pub const ABR_EVENT_B: u32 = 0x2074;
pub const HOLD_EVENT: u32 = 0x2080;
pub const HOLD_TIME: [u32; 2] = [0x2084, 0x2088];
pub const IIR: u32 = 0x208c;
/// External-trigger hysteresis (`DevSystemTPU_SetExtTrigHys`).
pub const EXT_HYS: u32 = 0x2264;

/// Comparator input number of the A side of analog channel `ch` (1-4): `2ch - 2`
/// (`DrvBaseTrigger_applyCfg` @0x2e7724: `iVar8 = src * 2 - 2`).
pub fn cmp_a(ch: usize) -> u8 {
    (2 * ch - 2) as u8
}

/// Comparator input number of the B side of analog channel `ch` (1-4): `2ch - 1`.
pub fn cmp_b(ch: usize) -> u8 {
    (2 * ch - 1) as u8
}

/// Threshold code for a trigger level in nV (`DrvBaseTrigger_SetLevelAjx` @0x2e7ed8):
/// `(level + offset) * 7500 / scale / gain + 32768`, clamped to 1..=0xfffe.
/// `scale_nv` is the channel's V/div in nV, `gain` the digital gain.
pub fn level_code(level_nv: f64, offset_nv: f64, scale_nv: f64, gain: f64) -> u16 {
    let c = (level_nv + offset_nv) * 7500.0 / scale_nv / gain + 32768.0;
    if c < 1.0 {
        1
    } else if c > 0xfffe as f64 {
        0xfffe
    } else {
        c as u16
    }
}

/// The two thresholds the stock code writes for a level with noise rejection: the upper one is
/// `level + noise * scale`, the lower one `level - noise * scale` (`DrvBaseTrigger_SetLevelAjx`).
pub fn level_codes(
    level_nv: f64,
    offset_nv: f64,
    scale_nv: f64,
    gain: f64,
    noise_reject_nv: f64,
) -> (u16, u16) {
    (
        level_code(level_nv - noise_reject_nv, offset_nv, scale_nv, gain),
        level_code(level_nv + noise_reject_nv, offset_nv, scale_nv, gain),
    )
}

/// `DevSystemTPU_SetLevelA`: one (register, value) write, `lo | hi << 16`.
pub fn set_level_a(ch: usize, lo: u16, hi: u16) -> (u32, u32) {
    (LEVEL_A[ch - 1], u32::from(lo) | (u32::from(hi) << 16))
}

/// `DevSystemTPU_SetLevelB`.
pub fn set_level_b(ch: usize, lo: u16, hi: u16) -> (u32, u32) {
    (LEVEL_B[ch - 1], u32::from(lo) | (u32::from(hi) << 16))
}

/// `DevSystemTPU_SetSourceA` @0x2af94c: `a | b << 6 | c << 8 | d << 14`, event bits preserved.
pub fn source_a(a: u8, b: u8, c: u8, d: u8) -> (u32, u32) {
    (
        SOURCE_A,
        (u32::from(a) & 0x3f)
            | (u32::from(b) & 3) << 6
            | (u32::from(c) & 0x3f) << 8
            | (u32::from(d) & 3) << 14,
    )
}

/// `DevSystemTPU_SetSourceB`: same packing into `SOURCE_B`.
pub fn source_b(a: u8, b: u8, c: u8, d: u8) -> (u32, u32) {
    (
        SOURCE_B,
        (u32::from(a) & 0x3f)
            | (u32::from(b) & 3) << 6
            | (u32::from(c) & 0x3f) << 8
            | (u32::from(d) & 3) << 14,
    )
}

/// `DevSystemTPU_SetTriggerA` @0x2afac4: event in bits 24-26 of `SOURCE_A`, then the two times in
/// milliseconds (low 32 bits at `TIME_A[0..2]`, the high halves packed at `TIME_A[2]`).
pub fn trigger_a(event: u8, t1_ns: u64, t2_ns: u64) -> [(u32, u32); 4] {
    let lo = |v: u64| (v / 1_000_000) as u32;
    [
        (SOURCE_A, (u32::from(event) & 7) << 24),
        (TIME_A[0], lo(t1_ns)),
        (TIME_A[1], lo(t2_ns)),
        (TIME_A[2], ((lo(t2_ns) as u64 >> 32) as u32) << 16),
    ]
}

/// `DevSystemTPU_SetTriggerMode` @0x2afce0: bits 0-4 of `MODE` (0 edge rising/falling path,
/// 1 either-edge, see `DrvBaseTrigger_applyCfg`).
pub fn trigger_mode(mode: u8) -> (u32, u32) {
    (MODE, u32::from(mode & 0x1f))
}

/// `DevSystemTPU_SetTriggerHoldoffTime` @0x2afef4: time in 4 ms units, high 16 bits at
/// `HOLD_TIME[1]` bits 0-15; `HOLD_TIME[0]` takes the low half.
pub fn holdoff_time(t_ns: u64) -> [(u32, u32); 2] {
    let ticks = t_ns / 4_000_000;
    [
        (HOLD_TIME[0], ticks as u32),
        (HOLD_TIME[1], ((ticks >> 32) as u32) & 0xffff),
    ]
}

/// `DevSystemTPU_SetTriggerHoldoffType`: bits 16-18 of `HOLD_TIME[1]`.
pub fn holdoff_type(kind: u8) -> (u32, u32) {
    (HOLD_TIME[1], u32::from(kind & 7) << 16)
}

/// `DevSystemTPU_SetTriggerIirset` @0x2aff64: bits 0-1, 2-3 and 4-17 of `IIR`.
pub fn iir(a: u8, b: u8, width: u16) -> (u32, u32) {
    (
        IIR,
        (u32::from(a) & 3) | (u32::from(b) & 3) << 2 | (u32::from(width) & 0x3fff) << 4,
    )
}

/// The stock software's edge composition for an analog channel (`DrvBaseTrigger_applyCfg`
/// @0x2e7724). `rising` selects the comparator order and the pattern; `either` (slope "either")
/// additionally programs the B side and sets mode 1.
///
/// Reconstructed, **not** verified on hardware: the fields of the source word are reproduced
/// exactly, the meaning of the pattern word (`SetSourcePatternA(2,1,2,1)`) is not decoded.
pub fn edge(
    ch: usize,
    level_lo: u16,
    level_hi: u16,
    rising: bool,
    either: bool,
) -> Vec<(u32, u32)> {
    let (a, b) = (cmp_a(ch), cmp_b(ch));
    let mut w = Vec::with_capacity(10);
    // reset both events (DevSystemTPU_SetTriggerA(0,0,0), SetTriggerB(0,0,0))
    w.push((SOURCE_A, 0));
    w.push((TIME_A[0], 0));
    w.push((TIME_A[1], 0));
    w.push((TIME_A[2], 0));
    w.push((SOURCE_B, 0));
    w.push((TIME_B[0], 0));
    w.push((TIME_B[1], 0));
    w.push((TIME_B[2], 0));
    if rising && !either {
        w.push(source_a(a, 0, b, 1));
    } else {
        w.push(source_a(b, 1, a, 0));
    }
    w.push(set_level_a(ch, level_lo, level_hi));
    if either {
        // SetSourceB(2,1,2,1) side and the "either" mode
        w.push(source_b(a, 1, b, 1));
        w.push(trigger_mode(1));
    } else {
        w.push(trigger_mode(0));
    }
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparator_pairs_follow_the_stock_mapping() {
        assert_eq!((cmp_a(1), cmp_b(1)), (0, 1));
        assert_eq!((cmp_a(2), cmp_b(2)), (2, 3));
        assert_eq!(
            (cmp_a(4), cmp_b(4)),
            (6, 7),
            "a 4-channel model reaches input 7"
        );
    }

    #[test]
    fn level_code_matches_the_formula() {
        // 1 V/div, 0 V offset, unity gain, level 0 V -> mid code
        assert_eq!(level_code(0.0, 0.0, 1e9, 1.0), 32768);
        // half a division up: 7500 * 0.5 = 3750 codes
        assert_eq!(level_code(500e6, 0.0, 1e9, 1.0), 36518);
        // clamps at both ends
        assert_eq!(level_code(1e15, 0.0, 1e9, 1.0), 0xfffe);
        assert_eq!(level_code(-1e15, 0.0, 1e9, 1.0), 1);
    }

    #[test]
    fn noise_reject_splits_the_thresholds() {
        let (lo, hi) = level_codes(0.0, 0.0, 1e9, 1.0, 100e6);
        assert!(lo < 32768 && hi > 32768);
        assert_eq!(hi - lo, 2 * (level_code(0.0, 0.0, 1e9, 1.0) - lo));
    }

    #[test]
    fn level_and_source_registers_match_the_tables() {
        assert_eq!(set_level_a(1, 0x1234, 0xabcd), (0x2004, 0xabcd1234));
        assert_eq!(set_level_a(4, 1, 2), (0x20c0, 0x00020001));
        let (r, v) = source_a(0, 0, 1, 1);
        assert_eq!(r, SOURCE_A);
        assert_eq!(v, 0x4100, "a=0 b=0 c=1 d=1");
    }

    #[test]
    fn edge_rising_programs_the_channel_comparators_and_mode_zero() {
        let w = edge(2, 32000, 33000, true, false);
        assert!(
            w.contains(&(SOURCE_A, 2u32 | 3 << 8 | 1 << 14)),
            "a=2, c=3, d=1"
        );
        assert!(w.contains(&(LEVEL_A[1], 33000u32 << 16 | 32000)));
        assert_eq!(*w.last().unwrap(), (MODE, 0));
        assert!(w.iter().any(|&(a, _)| a == TIME_A[0]));
    }

    #[test]
    fn edge_falling_swaps_the_comparators() {
        let w = edge(1, 1, 2, false, false);
        assert!(w.contains(&(SOURCE_A, 1u32 | 1 << 6)));
        assert_eq!(*w.last().unwrap(), (MODE, 0));
    }

    #[test]
    fn either_edge_sets_mode_one_and_the_b_side() {
        let w = edge(1, 1, 2, true, true);
        assert!(w.iter().any(|&(a, _)| a == SOURCE_B));
        assert_eq!(*w.last().unwrap(), (MODE, 1));
    }

    #[test]
    fn holdoff_and_iir_pack_their_bits() {
        let w = holdoff_time(8_000_000);
        assert_eq!(w[0], (HOLD_TIME[0], 2), "8 ms = 2 x 4 ms");
        assert_eq!(w[1], (HOLD_TIME[1], 0));
        assert_eq!(holdoff_type(3), (HOLD_TIME[1], 3 << 16));
        assert_eq!(iir(1, 2, 0x800), (IIR, 1 | 2 << 2 | 0x800 << 4));
    }
}
