// Copyright (C) 2026 Julian Decker
// SPDX-License-Identifier: GPL-2.0-or-later

//! DHO1000 trigger block (TPU), reconstructed from the stock library.
//!
//! Evidence: `libscope.so` decompiled (`docs/decomp/libscope/full-decomp`), function addresses in
//! the doc comments. Every setter below mirrors one decompiled function one-to-one; the composed
//! functions return the writes the stock code performs for that trigger type. **Nothing here has
//! run on hardware yet**: the sequences are the stock software's, not a measurement.
//!
//! Register map (TPU window 0x2000..0x22ff):
//!
//! | Register | Use |
//! |---|---|
//! | `LEVEL_A[4]` / `LEVEL_B[4]` | per-channel thresholds, `lo \| hi << 16` |
//! | `SOURCE_A` / `SOURCE_B` / `SOURCE_R` | comparator sources (bits 0-15), pattern word (16-23), event (24-26) |
//! | `TIME_A[3]` / `TIME_B[3]` / `TIME_R[3]` | trigger A/B/R times, 1 ns ticks |
//! | `LOGIC_A[2]` / `LOGIC_B[2]` / `LOGIC_R[2]` | logic operation terms and output op |
//! | `MODE` | trigger mux (bits 0-4), force, ABR FSM, SPU bypass |
//! | `HOLD_EVENT` / `HOLD_TIME[2]` | holdoff event number, time and type |
//! | `IIR` / `DELAY[2]` / `R_TIMEOUT[2]` / `EXT_HYS` | filters, delay, timeout, EXT hysteresis |
//! | `VIDEO[3]` | video trigger configuration |
//!
//! The stock setters read-modify-write the three event registers and the mode register through
//! software shadows, so the setters here take the shadow word and return the full register value;
//! the compositions thread the shadows, and applying their write lists to fresh registers leaves
//! the stock final state.
//!
//! The pattern word (`SetSourcePatternA/B/R`, bits 16-23 of the event register) selects the two
//! pattern-term slots of an event: a 3-bit source code and an invert bit each. The stock code
//! passes fixed slot codes per trigger type (2 for an edge term, 3/6 for setup/hold terms, 0/1/2
//! for pattern terms); the per-type values are reproduced in the compositions below.

use crate::dho1000::Write;

/// Per-channel trigger-A thresholds, from the 4-entry table the stock code indexes with the
/// channel number (`DevSystemTPU_SetLevelA` @0x2af4c8; table at file 0x00a0af20).
pub const LEVEL_A: [u32; 4] = [0x2004, 0x200c, 0x20b8, 0x20c0];
/// Per-channel trigger-B thresholds (`DevSystemTPU_SetLevelB` @0x2af4f0; table at file 0x00a0af30).
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
/// Trigger mux (bits 0-4), force, ABR FSM, SPU bypass (`SetTriggerMode` @0x2afce0 and the
/// `SetTriggerForce`/`SetTriggerABRFSM`/`SetTrigSPUBypass` siblings share this register).
pub const MODE: u32 = 0x205c;
/// Trigger delay in 4 ns ticks (`SetTriggerDelaytime` @0x2afe68).
pub const DELAY: [u32; 2] = [0x2060, 0x2064];
/// Reset-event timeout in 4 ns ticks (`SetTriggerRTimeout` @0x2afe18).
pub const R_TIMEOUT: [u32; 2] = [0x2068, 0x206c];
/// Number of the A and B events the ABR machine waits for.
pub const ABR_EVENT_A: u32 = 0x2070;
pub const ABR_EVENT_B: u32 = 0x2074;
pub const HOLD_EVENT: u32 = 0x2080;
pub const HOLD_TIME: [u32; 2] = [0x2084, 0x2088];
pub const IIR: u32 = 0x208c;
/// External-trigger hysteresis (`DevSystemTPU_SetExtTrigHys` @0x2affd4).
pub const EXT_HYS: u32 = 0x2264;
/// Video trigger: standard and line (`SetVideoCfg1` @0x2b0814), per-standard pair
/// (`SetVideoCfg2` @0x2b0828), line timing (`SetVideoCfg3` @0x2b083c).
pub const VIDEO: [u32; 3] = [0x21a4, 0x21a8, 0x21ac];

/// Trigger-mux codes of `MODE` bits 0-4, one per trigger family (`DrvBaseTrigger_SetType`
/// @0x2e759c dispatches to the `applyCfg` that writes each code).
pub const MODE_EDGE: u8 = 0;
pub const MODE_A_B: u8 = 1;
pub const MODE_NTH: u8 = 3;
pub const MODE_UART: u8 = 4;
pub const MODE_IIS: u8 = 5;
pub const MODE_LIN: u8 = 6;
pub const MODE_CAN: u8 = 7;
pub const MODE_FLEXRAY: u8 = 8;
pub const MODE_1553B: u8 = 0xa;
pub const MODE_SENT: u8 = 0xb;
pub const MODE_SPI: u8 = 0xc;
pub const MODE_I2C: u8 = 0xd;
pub const MODE_TV: u8 = 0xe;

/// Comparator input number of the A side of analog channel `ch` (1-4): `2ch - 2`
/// (`DrvBaseTrigger_applyCfg` @0x2e7724: `iVar8 = src * 2 - 2`).
pub fn cmp_a(ch: usize) -> u8 {
    (2 * ch - 2) as u8
}

/// Comparator input number of the B side of analog channel `ch` (1-4): `2ch - 1`.
pub fn cmp_b(ch: usize) -> u8 {
    (2 * ch - 1) as u8
}

/// `DrvTrigger_ToCmpASrc` @0x2e86ec: the comparator source word of a UI source. Channels 1-4 map
/// to their A-side input, 9-0x18 (serial/digital) to `src + 0x17`, 0x19 (AC line) to 0x30,
/// 0x1a (EXT) to 0x31, anything else to 0x33.
pub fn to_cmp_src(src: u8) -> u8 {
    match src {
        1..=4 => 2 * src - 2,
        9..=0x18 => src + 0x17,
        0x19 => 0x30,
        0x1a => 0x31,
        _ => 0x33,
    }
}

/// `DrvTrigger_ToTpuWhen` @0x2e87ec: the TPU time-condition code. UI when 1-4 map to 0-3, 0 (no
/// condition) to 5, anything above 4 to 6.
pub fn to_tpu_when(when: u8) -> u8 {
    match when {
        0 => 5,
        1..=4 => when - 1,
        _ => 6,
    }
}

/// `DrvTrigger_ToLogicSel` @0x2e880c: the 2-bit term code of a UI pattern term, from the 5-entry
/// table at file 0x00a0ee0c (recovered from the stock binary).
pub fn to_logic_sel(term: u8) -> u8 {
    const LUT: [u8; 5] = [2, 3, 0, 0, 0];
    if term < 5 { LUT[term as usize] } else { 0 }
}

/// `toTpuLogicFun` @0x2e88b0: the output operation of a pattern/duration trigger: AND (1) stays 1,
/// OR (3) becomes 2, everything else 0.
pub fn to_tpu_logic_fun(logic: u8) -> u8 {
    match logic {
        3 => 2,
        1 => 1,
        _ => 0,
    }
}

/// `selectPatternEdge` @0x2e8850 over the 21 pattern terms: counts the rise/fall terms (values 3
/// and 4) and returns their number and the index of the first one.
pub fn select_pattern_edge(terms: &[u8; 21]) -> (usize, Option<usize>) {
    let mut count = 0;
    let mut first = None;
    for (i, &t) in terms.iter().enumerate() {
        if t == 3 || t == 4 {
            if first.is_none() {
                first = Some(i);
            }
            count += 1;
        }
    }
    (count, first)
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

/// `DevSystemTPU_SetSourceA` @0x2af94c: `a | b << 6 | c << 8 | d << 14`, the pattern and event
/// bits (16-31 of `sh`) kept.
pub fn source_a(sh: u32, a: u8, b: u8, c: u8, d: u8) -> (u32, u32) {
    (
        SOURCE_A,
        (u32::from(a) & 0x3f)
            | (u32::from(b) & 3) << 6
            | (u32::from(c) & 0x3f) << 8
            | (u32::from(d) & 3) << 14
            | (sh & 0xffff_0000),
    )
}

/// `DevSystemTPU_SetSourceB` @0x2af97c: same packing into `SOURCE_B`.
pub fn source_b(sh: u32, a: u8, b: u8, c: u8, d: u8) -> (u32, u32) {
    (
        SOURCE_B,
        (u32::from(a) & 0x3f)
            | (u32::from(b) & 3) << 6
            | (u32::from(c) & 0x3f) << 8
            | (u32::from(d) & 3) << 14
            | (sh & 0xffff_0000),
    )
}

/// `DevSystemTPU_SetSourceR`: same packing into `SOURCE_R`.
pub fn source_r(sh: u32, a: u8, b: u8, c: u8, d: u8) -> (u32, u32) {
    (
        SOURCE_R,
        (u32::from(a) & 0x3f)
            | (u32::from(b) & 3) << 6
            | (u32::from(c) & 0x3f) << 8
            | (u32::from(d) & 3) << 14
            | (sh & 0xffff_0000),
    )
}

/// `DevSystemTPU_SetSourcePatternA` @0x2af9dc: slot-A source in bits 16-18, slot-A invert in 19,
/// slot-B source in 20-22, slot-B invert in 23; the rest of `sh` kept.
pub fn pattern_a(sh: u32, s1: u8, i1: bool, s2: u8, i2: bool) -> (u32, u32) {
    (
        SOURCE_A,
        (u32::from(s1) & 7) << 16
            | u32::from(i1) << 19
            | (u32::from(s2) & 7) << 20
            | u32::from(i2) << 23
            | (sh & 0xff00_ffff),
    )
}

/// `DevSystemTPU_SetSourcePatternB` @0x2afa64: same packing into `SOURCE_B`.
pub fn pattern_b(sh: u32, s1: u8, i1: bool, s2: u8, i2: bool) -> (u32, u32) {
    (
        SOURCE_B,
        (u32::from(s1) & 7) << 16
            | u32::from(i1) << 19
            | (u32::from(s2) & 7) << 20
            | u32::from(i2) << 23
            | (sh & 0xff00_ffff),
    )
}

/// `DevSystemTPU_SetSourcePatternR` @0x2afa94: same packing into `SOURCE_R`.
pub fn pattern_r(sh: u32, s1: u8, i1: bool, s2: u8, i2: bool) -> (u32, u32) {
    (
        SOURCE_R,
        (u32::from(s1) & 7) << 16
            | u32::from(i1) << 19
            | (u32::from(s2) & 7) << 20
            | u32::from(i2) << 23
            | (sh & 0xff00_ffff),
    )
}

/// `DevSystemTPU_SetTriggerA` @0x2afac4: event in bits 24-26 of `SOURCE_A`, then the two times as
/// 1 ns ticks (low 32 bits at `TIME_A[0..2]`, the two 16-bit high halves packed at `TIME_A[2]`).
pub fn trigger_a(sh: u32, event: u8, t1_ns: u64, t2_ns: u64) -> [Write; 4] {
    [
        (SOURCE_A, (sh & !0x0700_0000) | (u32::from(event) & 7) << 24),
        (TIME_A[0], t1_ns as u32),
        (TIME_A[1], t2_ns as u32),
        (
            TIME_A[2],
            ((t1_ns >> 32) as u32 & 0xffff) | (((t2_ns >> 32) as u32 & 0xffff) << 16),
        ),
    ]
}

/// `DevSystemTPU_SetTriggerB` @0x2afb78: `trigger_a` for the B event.
pub fn trigger_b(sh: u32, event: u8, t1_ns: u64, t2_ns: u64) -> [Write; 4] {
    [
        (SOURCE_B, (sh & !0x0700_0000) | (u32::from(event) & 7) << 24),
        (TIME_B[0], t1_ns as u32),
        (TIME_B[1], t2_ns as u32),
        (
            TIME_B[2],
            ((t1_ns >> 32) as u32 & 0xffff) | (((t2_ns >> 32) as u32 & 0xffff) << 16),
        ),
    ]
}

/// `DevSystemTPU_SetTriggerR` @0x2afc2c: `trigger_a` for the R event.
pub fn trigger_r(sh: u32, event: u8, t1_ns: u64, t2_ns: u64) -> [Write; 4] {
    [
        (SOURCE_R, (sh & !0x0700_0000) | (u32::from(event) & 7) << 24),
        (TIME_R[0], t1_ns as u32),
        (TIME_R[1], t2_ns as u32),
        (
            TIME_R[2],
            ((t1_ns >> 32) as u32 & 0xffff) | (((t2_ns >> 32) as u32 & 0xffff) << 16),
        ),
    ]
}

/// `DevSystemTPU_SetLogicOperationA` @0x2af518: the 2-bit term code of source `index` (0-20) in
/// `LOGIC_A[0]` (sources 0-15) or `LOGIC_A[1]` bits 0-9 (16-20), and the output operation in
/// `LOGIC_A[1]` bits 10-11. Returns the writes of one call with the pair shadow updated.
pub fn logic_a(pair: &mut (u32, u32), index: usize, term: u8, op: u8) -> Vec<Write> {
    let mut w = Vec::with_capacity(2);
    if index < 0x10 {
        let shift = index << 1;
        pair.0 = (pair.0 & !(3 << shift)) | (u32::from(term) & 3) << shift;
        w.push((LOGIC_A[0], pair.0));
    } else if index < 0x15 {
        let shift = (index - 0x10) << 1;
        pair.1 = (pair.1 & !(3 << shift)) | (u32::from(term) & 3) << shift;
    }
    pair.1 = (pair.1 & !0x0c00) | (u32::from(op) & 3) << 10;
    w.push((LOGIC_A[1], pair.1));
    w
}

/// `DevSystemTPU_SetLogicOperationB` @0x2af70c: `logic_a` for the B event.
pub fn logic_b(pair: &mut (u32, u32), index: usize, term: u8, op: u8) -> Vec<Write> {
    let mut w = logic_a(pair, index, term, op);
    for x in &mut w {
        x.0 += LOGIC_B[0] - LOGIC_A[0];
    }
    w
}

/// `DevSystemTPU_SetLogicOperationR` @0x2af82c: `logic_a` for the R event.
pub fn logic_r(pair: &mut (u32, u32), index: usize, term: u8, op: u8) -> Vec<Write> {
    let mut w = logic_a(pair, index, term, op);
    for x in &mut w {
        x.0 += LOGIC_R[0] - LOGIC_A[0];
    }
    w
}

/// `DevSystemTPU_SetTriggerMode` @0x2afce0: bits 0-4 of `MODE`, the rest of `sh` kept.
pub fn trigger_mode(sh: u32, mode: u8) -> (u32, u32) {
    (MODE, (sh & !0x1f) | u32::from(mode & 0x1f))
}

/// `DevSystemTPU_SetTrigSPUBypass` @0x2afd94: bit 12 of `MODE`. The stock code sets it for every
/// serial trigger type, for the pattern trigger, and for a coupling other than DC.
pub fn spu_bypass(sh: u32, on: bool) -> (u32, u32) {
    (MODE, (sh & !0x1000) | u32::from(on) << 12)
}

/// `DevSystemTPU_SetTriggerForce` @0x2afdbc: bit 5 of `MODE` set, then cleared.
pub fn force(sh: u32) -> [Write; 2] {
    [(MODE, sh | 0x20), (MODE, sh & !0x20)]
}

/// `DevSystemTPU_SetTriggerABRRst` @0x2afd00: bit 11 of `MODE` set, then cleared.
pub fn abr_rst(sh: u32) -> [Write; 2] {
    [(MODE, sh | 0x800), (MODE, sh & !0x800)]
}

/// `DevSystemTPU_SetTriggerABRFSM` @0x2afd5c: five single-bit settings in bits 6-10 of `MODE`.
pub fn abr_fsm(sh: u32, a: bool, b: bool, c: bool, d: bool, e: bool) -> (u32, u32) {
    (
        MODE,
        u32::from(a) << 6
            | u32::from(b) << 7
            | u32::from(c) << 8
            | u32::from(d) << 9
            | u32::from(e) << 10
            | (sh & 0xf83f),
    )
}

/// `DevSystemTPU_SetTriggerABREventANum` @0x2afec0: the A-event count of the ABR machine.
pub fn abr_event_a(n: u32) -> (u32, u32) {
    (ABR_EVENT_A, n)
}

/// `DevSystemTPU_SetTriggerABREventBNum` @0x2afecc.
pub fn abr_event_b(n: u32) -> (u32, u32) {
    (ABR_EVENT_B, n)
}

/// `DevSystemTPU_SetTriggerDelaytime` @0x2afe68: 4 ns ticks, full 64-bit split.
pub fn delaytime(t_ns: u64) -> [Write; 2] {
    let ticks = t_ns / 4;
    [(DELAY[0], ticks as u32), (DELAY[1], (ticks >> 32) as u32)]
}

/// `DevSystemTPU_SetTriggerRTimeout` @0x2afe18: 4 ns ticks, full 64-bit split.
pub fn r_timeout(t_ns: u64) -> [Write; 2] {
    let ticks = t_ns / 4;
    [(R_TIMEOUT[0], ticks as u32), (R_TIMEOUT[1], (ticks >> 32) as u32)]
}

/// `DevSystemTPU_SetTriggerHoldoffTime` @0x2afef4: time in 4 ms units, high 16 bits at
/// `HOLD_TIME[1]` bits 0-15; `HOLD_TIME[0]` takes the low half.
pub fn holdoff_time(t_ns: u64) -> [Write; 2] {
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

/// `DevSystemTPU_SetVideoCfg1` @0x2b0814: standard bits 0-1, line number bits 2-12.
pub fn video1(standard: u8, line: u32) -> (u32, u32) {
    (VIDEO[0], (u32::from(standard) & 3) | (line & 0x7ff) << 2)
}

/// `DevSystemTPU_SetVideoCfg2` @0x2b0828: two 16-bit halves.
pub fn video2(lo: u32, hi: u32) -> (u32, u32) {
    (VIDEO[1], (lo & 0xffff) | ((hi & 0xffff) << 16))
}

/// `DevSystemTPU_SetVideoCfg3` @0x2b083c: two 11-bit fields and a flag.
pub fn video3(a: u32, b: u32, flag: bool) -> (u32, u32) {
    (
        VIDEO[2],
        (a & 0x7ff) | (b & 0x7ff) << 0xb | u32::from(flag) << 0x16,
    )
}

/// The stock software's edge composition for an analog channel (`DrvBaseTrigger_applyCfg`
/// @0x2e7724). `rising` selects the comparator order and the pattern; `either` (slope "either")
/// additionally programs the B side and sets mode 1. Reconstructed, not verified on hardware.
pub fn edge(ch: usize, level_lo: u16, level_hi: u16, rising: bool, either: bool) -> Vec<Write> {
    let (a, b) = (cmp_a(ch), cmp_b(ch));
    let mut wa = 0u32;
    let mut wb = 0u32;
    let mut w = Vec::with_capacity(14);
    // reset both events (DevSystemTPU_SetTriggerA(0,0,0), SetTriggerB(0,0,0))
    w.extend_from_slice(&trigger_a(wa, 0, 0, 0));
    w.extend_from_slice(&trigger_b(wb, 0, 0, 0));
    if rising && !either {
        wa = source_a(wa, a, 0, b, 1).1;
        w.push((SOURCE_A, wa));
        wa = pattern_a(wa, 2, true, 2, true).1;
        w.push((SOURCE_A, wa));
        w.push(trigger_mode(0, MODE_EDGE));
    } else if either {
        wa = source_a(wa, b, 1, a, 0).1;
        w.push((SOURCE_A, wa));
        wa = pattern_a(wa, 2, false, 2, false).1;
        w.push((SOURCE_A, wa));
        wb = source_b(wb, a, 1, b, 1).1;
        w.push((SOURCE_B, wb));
        wb = pattern_b(wb, 2, true, 2, true).1;
        w.push((SOURCE_B, wb));
        w.push(trigger_mode(0, MODE_A_B));
    } else {
        wa = source_a(wa, b, 1, a, 0).1;
        w.push((SOURCE_A, wa));
        wa = pattern_a(wa, 2, false, 2, false).1;
        w.push((SOURCE_A, wa));
        w.push(trigger_mode(0, MODE_EDGE));
    }
    w.push(set_level_a(ch, level_lo, level_hi));
    w
}

/// The edge composition for the AC-line (0x19) and EXT (0x1a) sources: both comparator slots take
/// the line source code, pattern (2, 1, 2, 1), mode 0 (`DrvBaseTrigger_applyCfg`).
pub fn edge_line(line: bool, negative: bool) -> Vec<Write> {
    let src = if line { 0x30 } else { 0x31 };
    let mut wa = 0u32;
    let mut w = Vec::with_capacity(8);
    w.extend_from_slice(&trigger_a(wa, 0, 0, 0));
    wa = source_a(wa, src, 0, src, 0).1;
    w.push((SOURCE_A, wa));
    wa = pattern_a(wa, 2, true, 2, true).1;
    w.push((SOURCE_A, wa));
    w.push(trigger_mode(0, MODE_EDGE));
    w.push((EXT_HYS, u32::from(negative) << 16));
    w
}

/// Pulse-width trigger (`DrvPulseTrigger_applyCfg` @0x302024): `positive` picks the polarity, the
/// when-code and the two width limits go to the A event, pattern slots (2, inv, 1, 0), mode 0.
pub fn pulse(ch: usize, positive: bool, when: u8, t1_ns: u64, t2_ns: u64) -> Vec<Write> {
    let (a, b) = (cmp_a(ch), cmp_b(ch));
    let mut wa = 0u32;
    let mut w = Vec::with_capacity(12);
    w.push(iir(0, ch as u8, 0x800));
    w.extend_from_slice(&trigger_a(wa, when, t1_ns, t2_ns));
    wa = (wa & !0x0700_0000) | (u32::from(when) & 7) << 24;
    if positive {
        wa = source_a(wa, a, 0, b, 1).1;
    } else {
        wa = source_a(wa, b, 1, a, 0).1;
    }
    w.push((SOURCE_A, wa));
    wa = pattern_a(wa, 2, !positive, 1, false).1;
    w.push((SOURCE_A, wa));
    w.push(trigger_mode(0, MODE_EDGE));
    w
}

/// Slope trigger (`DrvSlopeTrigger_applyCfg` @0x3034b8 and `SetWhen` @0x303634): A-side/B-side
/// order per slope, pattern (2, inv, 1, 0), the when-code and the two slope-time limits in the A
/// event, mode 0.
pub fn slope(ch: usize, rising: bool, when: u8, t1_ns: u64, t2_ns: u64) -> Vec<Write> {
    let (a, b) = (cmp_a(ch), cmp_b(ch));
    let mut wa = 0u32;
    let mut w = Vec::with_capacity(12);
    w.push(iir(0, ch as u8, 0x800));
    wa = if rising {
        source_a(wa, a, 0, b, 0).1
    } else {
        source_a(wa, b, 1, a, 1).1
    };
    w.push((SOURCE_A, wa));
    wa = pattern_a(wa, 2, rising, 1, false).1;
    w.push((SOURCE_A, wa));
    w.extend_from_slice(&trigger_a(wa, when, t1_ns, t2_ns));
    w.push(trigger_mode(0, MODE_EDGE));
    w
}

/// Runt trigger (`DrvRuntTrigger_applyCfg` @0x3022ac). `polarity` 0 = positive, 1 = negative,
/// 2 = either (both events, mode 1). A when-code of 5 (no condition) zeroes the A event words.
pub fn runt(ch: usize, polarity: u8, when: u8, t1_ns: u64, t2_ns: u64) -> Vec<Write> {
    let (a, b) = (cmp_a(ch), cmp_b(ch));
    let mut wa = 0u32;
    let mut wb = 0u32;
    let mut w = Vec::with_capacity(16);
    w.push(iir(0, ch as u8, 0x800));
    let (event, s1, s2) = if when != 5 { (when, t1_ns, t2_ns) } else { (0, 0, 0) };
    match polarity {
        0 => {
            wa = source_a(wa, a, 0, b, 0).1;
            w.push((SOURCE_A, wa));
            wa = pattern_a(wa, 3, false, 6, false).1;
            w.push((SOURCE_A, wa));
            w.extend_from_slice(&trigger_a(wa, event, s1, s2));
            w.push(trigger_mode(0, MODE_EDGE));
        }
        1 => {
            wa = source_a(wa, b, 1, a, 1).1;
            w.push((SOURCE_A, wa));
            wa = pattern_a(wa, 3, true, 6, false).1;
            w.push((SOURCE_A, wa));
            w.extend_from_slice(&trigger_a(wa, event, s1, s2));
            w.push(trigger_mode(0, MODE_EDGE));
        }
        _ => {
            wa = source_a(wa, a, 0, b, 0).1;
            w.push((SOURCE_A, wa));
            wa = pattern_a(wa, 3, false, 1, false).1;
            w.push((SOURCE_A, wa));
            w.extend_from_slice(&trigger_a(wa, event, s1, s2));
            wb = source_b(wb, a, 1, b, 1).1;
            w.push((SOURCE_B, wb));
            wb = pattern_b(wb, 3, true, 1, false).1;
            w.push((SOURCE_B, wb));
            w.extend_from_slice(&trigger_b(wb, event, s1, s2));
            w.push(trigger_mode(0, MODE_A_B));
        }
    }
    w
}

/// Window trigger (`DrvOverTriger_applyCfg` @0x301908). `direction` 0 = enter, 1 = exit, 3 =
/// either; `when` 0 = no time condition, 1 = timed (event 0), 2 = time-limited (event 4, the
/// same time twice). The B-side source order flips for exit; "either" programs both events.
pub fn window(ch: usize, direction: u8, when: u8, t_ns: u64) -> Vec<Write> {
    let (a, b) = (cmp_a(ch), cmp_b(ch));
    let mut wa = 0u32;
    let mut wb = 0u32;
    let mut w = Vec::with_capacity(16);
    w.push(iir(0, ch as u8, 0x800));
    if direction == 1 {
        wa = source_a(wa, b, 0, a, 0).1;
        wb = source_b(wb, b, 0, a, 0).1;
    } else {
        wa = source_a(wa, a, 0, b, 0).1;
        wb = source_b(wb, b, 0, a, 0).1;
    }
    w.push((SOURCE_A, wa));
    let (event, t1, t2) = if when == 2 { (4, t_ns, t_ns) } else { (0, 0, 0) };
    w.extend_from_slice(&trigger_a(wa, event, t1, t2));
    let (i1, i2) = match (direction, when) {
        (1, 1) => (true, true),
        (1, 2) => (true, true),
        (3, 1) => (true, true),
        (3, 2) => (false, false),
        (3, 0) => (true, true),
        _ => (false, false),
    };
    wa = pattern_a(wa, 2, i1, 2, i2).1;
    w.push((SOURCE_A, wa));
    let mode = if direction == 3 { MODE_A_B } else { MODE_EDGE };
    if direction == 3 {
        w.push((SOURCE_B, wb));
        w.extend_from_slice(&trigger_b(wb, event, t1, t2));
        wb = pattern_b(wb, 2, i1, 2, i2).1;
        w.push((SOURCE_B, wb));
    }
    w.push(trigger_mode(0, mode));
    w
}

/// Timeout/dropout trigger (`DrvTimeoutTrigger_applyCfg` @0x303ee0): both events get event code 4
/// and the timeout as the first time; `slope` 0 = rising, 1 = falling, 3 = either (mode 1).
pub fn timeout(ch: usize, slope: u8, t_ns: u64) -> Vec<Write> {
    let (a, b) = (cmp_a(ch), cmp_b(ch));
    let mut wa = 0u32;
    let mut wb = 0u32;
    let mut w = Vec::with_capacity(16);
    w.push(iir(0, ch as u8, 0x800));
    if slope == 1 {
        wa = source_a(wa, b, 0, a, 0).1;
        wb = source_b(wb, a, 0, b, 0).1;
    } else {
        wa = source_a(wa, a, 0, b, 0).1;
        wb = source_b(wb, b, 0, a, 0).1;
    }
    w.push((SOURCE_A, wa));
    w.push((SOURCE_B, wb));
    w.extend_from_slice(&trigger_a(wa, 4, t_ns, 0));
    w.extend_from_slice(&trigger_b(wb, 4, t_ns, 0));
    if slope == 3 {
        wa = pattern_a(wa, 2, false, 2, false).1;
        w.push((SOURCE_A, wa));
        wb = pattern_b(wb, 2, true, 2, true).1;
        w.push((SOURCE_B, wb));
        w.push(trigger_mode(0, MODE_A_B));
    } else {
        let inv = slope == 1;
        wa = pattern_a(wa, 2, inv, 2, inv).1;
        w.push((SOURCE_A, wa));
        w.push(trigger_mode(0, MODE_EDGE));
    }
    w
}

/// Delay trigger (`DrvDelayTrigger_applyCfg` @0x2ff088): event A on the B-side comparator of
/// channel `ch_b` and the A-side of `ch_a`, the four slope combinations each with pattern
/// (1, 0, 1, 0), mode 0. The A event carries the when-code and the delay window.
pub fn delay(
    ch_a: usize,
    ch_b: usize,
    slope_a: u8,
    slope_b: u8,
    when: u8,
    t1_ns: u64,
    t2_ns: u64,
) -> Vec<Write> {
    let (aa, ab) = (cmp_a(ch_a), cmp_b(ch_a));
    let (ba, bb) = (cmp_a(ch_b), cmp_b(ch_b));
    let mut wa = 0u32;
    let mut w = Vec::with_capacity(14);
    w.push(iir(0, ch_a as u8, 0x800));
    w.push(iir(0, ch_b as u8, 0x800));
    w.extend_from_slice(&trigger_a(wa, when, t1_ns, t2_ns));
    wa = (wa & !0x0700_0000) | (u32::from(when) & 7) << 24;
    wa = match (slope_a, slope_b) {
        (0, 0) => source_a(wa, ba, 0, aa, 0).1,
        (1, 1) => source_a(wa, bb, 1, ab, 1).1,
        (0, 1) => source_a(wa, bb, 1, aa, 0).1,
        (1, 0) => source_a(wa, ba, 0, ab, 1).1,
        _ => return w,
    };
    w.push((SOURCE_A, wa));
    wa = pattern_a(wa, 1, false, 1, false).1;
    w.push((SOURCE_A, wa));
    w.push(trigger_mode(0, MODE_EDGE));
    w
}

/// Pattern trigger (`DrvPatternTrigger_applyCfg` @0x301dd4): the 21 term codes (0 = don't care,
/// 1 = low, 2 = high, 3 = rising, 4 = falling) with the logic operation, the first rise/fall term
/// as the event edge. `logic` 1 = AND, 3 = OR (the stock UI values).
pub fn pattern(terms: &[u8; 21], logic: u8) -> Vec<Write> {
    let mut pair = (0u32, 0u32);
    let mut wa = 0u32;
    let mut w = Vec::with_capacity(21 * 2 + 8);
    for ch in 1..=4u8 {
        w.push(iir(0, ch, 0x800));
    }
    let op = to_tpu_logic_fun(logic);
    for (i, &t) in terms.iter().enumerate() {
        let term = to_logic_sel(t);
        w.extend(logic_a(&mut pair, i, term, op));
    }
    let (_, edge) = select_pattern_edge(terms);
    if let Some(i) = edge {
        let src = to_cmp_src((i + 1) as u8);
        wa = source_a(wa, src, u8::from(terms[i] != 3), src, 2).1;
        w.push((SOURCE_A, wa));
        wa = pattern_a(wa, 2, terms[i] == 3, 0, false).1;
        w.push((SOURCE_A, wa));
    } else {
        wa = pattern_a(wa, 0, false, 0, false).1;
        w.push((SOURCE_A, wa));
    }
    w.extend_from_slice(&trigger_a(wa, 0, 0, 0));
    w.push(trigger_mode(0, MODE_EDGE));
    w
}

/// Duration trigger (`DrvDurationTrigger_applyCfg` @0x2ff774): the pattern composition with the
/// when-code and the two duration limits in the A event, pattern slots (0, 0, edge-slot).
pub fn duration(terms: &[u8; 21], logic: u8, when: u8, t1_ns: u64, t2_ns: u64) -> Vec<Write> {
    let mut pair = (0u32, 0u32);
    let mut wa = 0u32;
    let mut w = Vec::with_capacity(21 * 2 + 8);
    for ch in 1..=4u8 {
        w.push(iir(0, ch, 0x800));
    }
    let op = to_tpu_logic_fun(logic);
    for (i, &t) in terms.iter().enumerate() {
        let term = to_logic_sel(t);
        w.extend(logic_a(&mut pair, i, term, op));
    }
    let (_, edge) = select_pattern_edge(terms);
    if let Some(i) = edge {
        let src = to_cmp_src((i + 1) as u8);
        wa = source_a(wa, src, u8::from(terms[i] != 3), src, 2).1;
        w.push((SOURCE_A, wa));
        wa = pattern_a(wa, 0, false, 2, terms[i] == 3).1;
        w.push((SOURCE_A, wa));
    } else {
        wa = pattern_a(wa, 0, false, 0, false).1;
        w.push((SOURCE_A, wa));
    }
    w.extend_from_slice(&trigger_a(wa, when, t1_ns, t2_ns));
    w.push(trigger_mode(0, MODE_EDGE));
    w
}

/// Nth-edge trigger (`DrvNthTrigger_applyCfg` @0x301508): the ABR machine counts `count` edges of
/// the channel, `idle_ns` arms the reset-event timeout, mode 3.
pub fn nth(ch: usize, rising: bool, count: u32, idle_ns: u64) -> Vec<Write> {
    let (a, b) = (cmp_a(ch), cmp_b(ch));
    let mut wa = 0u32;
    let mut w = Vec::with_capacity(16);
    w.push(iir(0, ch as u8, 0x800));
    if rising {
        wa = source_a(wa, a, 0, b, 0).1;
        wa = pattern_a(wa, 2, true, 2, true).1;
    } else {
        wa = source_a(wa, b, 0, a, 0).1;
        wa = pattern_a(wa, 2, false, 2, false).1;
    }
    w.push((SOURCE_A, wa));
    w.extend_from_slice(&trigger_a(wa, 0, 0, 0));
    w.extend_from_slice(&trigger_b(0, 0, 0, 0));
    w.push(abr_fsm(0, true, true, true, false, true));
    w.push(abr_event_a(count));
    w.extend_from_slice(&r_timeout(idle_ns));
    w.push(trigger_mode(0x5c0, MODE_NTH));
    w
}

/// Setup/hold trigger (`DrvSetupHoldTrigger_applyCfg` @0x302ed8): clock on `ch_clk`, data on
/// `ch_data`, `mode` 0 = setup, 1 = hold, 2 = both (mode 1, both events). The setup and hold
/// times are the second A/B event time.
pub fn setup_hold(
    ch_clk: usize,
    ch_data: usize,
    clk_rising: bool,
    data_rising: bool,
    mode: u8,
    t_ns: u64,
) -> Vec<Write> {
    let (ca, cb) = (cmp_a(ch_clk), cmp_b(ch_clk));
    let (da, db) = (cmp_a(ch_data), cmp_b(ch_data));
    let mut wa = 0u32;
    let mut wb = 0u32;
    let mut w = Vec::with_capacity(16);
    w.push(iir(0, ch_clk as u8, 0x800));
    w.push(iir(0, ch_data as u8, 0x800));
    let (clk_lo, clk_hi) = if clk_rising { (ca, cb) } else { (cb, ca) };
    let (data_lo, data_hi) = if data_rising { (da, db) } else { (db, da) };
    match mode {
        0 => {
            wa = source_a(wa, clk_lo, u8::from(!clk_rising), data_hi, u8::from(!data_rising)).1;
            w.push((SOURCE_A, wa));
            wa = pattern_a(wa, 2, clk_rising, 3, data_rising).1;
            w.push((SOURCE_A, wa));
            w.extend_from_slice(&trigger_a(wa, 1, 0, t_ns));
            w.push(trigger_mode(0, MODE_EDGE));
        }
        1 => {
            wa = source_a(wa, data_lo, u8::from(data_rising), clk_hi, u8::from(clk_rising)).1;
            w.push((SOURCE_A, wa));
            wa = pattern_a(wa, 2, !data_rising, 6, false).1;
            w.push((SOURCE_A, wa));
            w.extend_from_slice(&trigger_a(wa, 1, 0, t_ns));
            w.push(trigger_mode(0, MODE_EDGE));
        }
        _ => {
            wa = source_a(wa, clk_lo, u8::from(!clk_rising), data_hi, u8::from(!data_rising)).1;
            w.push((SOURCE_A, wa));
            wa = pattern_a(wa, 2, clk_rising, 3, data_rising).1;
            w.push((SOURCE_A, wa));
            w.extend_from_slice(&trigger_a(wa, 1, 0, t_ns));
            wb = source_b(wb, data_lo, u8::from(data_rising), clk_hi, u8::from(clk_rising)).1;
            w.push((SOURCE_B, wb));
            wb = pattern_b(wb, 2, !data_rising, 6, false).1;
            w.push((SOURCE_B, wb));
            w.extend_from_slice(&trigger_b(wb, 1, 0, t_ns));
            w.push(trigger_mode(0, MODE_A_B));
        }
    }
    w
}

/// Video/TV trigger (`DrvTvTrigger_applyCfg` @0x30422c): the channel on the R event, mode 0xe,
/// and the per-standard `SetVideoCfg2/3` constants the stock code hardcodes. `standard` 0 = NTSC,
/// 1 = PAL, 2-3 = 480p/576p, 4-8 = the 720p family, 9-0xf = the 1080p family; `line` 0 = any.
pub fn video(ch: usize, negative: bool, standard: u8, line: u32) -> Vec<Write> {
    let (a, b) = (cmp_a(ch), cmp_b(ch));
    let mut wa = 0u32;
    let mut wr = 0u32;
    let mut w = Vec::with_capacity(14);
    w.push(iir(0, ch as u8, 0x800));
    w.push(video1(0, line));
    wr = source_r(wr, a, 2, b, 2).1;
    w.push((SOURCE_R, wr));
    w.extend_from_slice(&trigger_r(wr, 0, 0, 0));
    wa = pattern_a(wa, 2, negative, 2, negative).1;
    w.push((SOURCE_A, wa));
    w.push(trigger_mode(0, MODE_TV));
    let (cfg2, cfg3) = video_constants(standard);
    w.push(video2(cfg2.0, cfg2.1));
    w.push(video3(cfg3.0, cfg3.1, cfg3.2));
    let std_bits = if standard > 1 { 2 } else { 0 };
    w.push(video1(std_bits | u32::from(negative) as u8, line));
    w
}

/// The per-standard `SetVideoCfg2`/`SetVideoCfg3` values from `DrvTvTrigger_applyCfg`
/// @0x30422c: ((cfg2-lo, cfg2-hi), (cfg3-a, cfg3-b, cfg3-flag)).
pub fn video_constants(standard: u8) -> ((u32, u32), (u32, u32, bool)) {
    match standard {
        0 => ((0x1824_4f00, 0xb028_3580u32), (5, 0x20d, false)),
        1 => ((0x1824_4f00, 0xce8e_3100u32), (2, 0x271, false)),
        2 => ((0x1824_4f00, 0x4dea_3b40u32), (8, 0x20d, false)),
        3 => ((2_350_000_000u32, 0xe747_1880u32), (2, 0x271, false)),
        4..=8 => {
            let c = match standard {
                4 => 0x0c3a_8c00,
                5 => 0x1578_2880,
                6 => 0x3900_6480,
                7 => 0x4ad3_c4c0,
                _ => 0xcf44_cc40,
            };
            ((3_500_000_000, c), (2, 0x2ee, true))
        }
        _ => {
            let c = match standard {
                9 => 0x6166_5bc0,
                0xa => 0x1202_2680,
                0xb | 0xe => 0xc2bd_7540,
                0xd => 0x7c3b_4f00,
                _ => 0x23f5_0ac0,
            };
            ((0x69fa_2080, c), (2, 0x465, true))
        }
    }
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
    fn ui_sources_map_to_comparator_codes() {
        assert_eq!(to_cmp_src(1), 0);
        assert_eq!(to_cmp_src(4), 6);
        assert_eq!(to_cmp_src(9), 0x20, "serial source 9 -> 0x20");
        assert_eq!(to_cmp_src(0x18), 0x2f);
        assert_eq!(to_cmp_src(0x19), 0x30, "AC line");
        assert_eq!(to_cmp_src(0x1a), 0x31, "EXT");
        assert_eq!(to_cmp_src(0), 0x33, "error code");
    }

    #[test]
    fn when_codes_follow_the_stock_mapping() {
        assert_eq!(to_tpu_when(0), 5);
        assert_eq!(to_tpu_when(1), 0);
        assert_eq!(to_tpu_when(4), 3);
        assert_eq!(to_tpu_when(5), 6);
        assert_eq!(to_tpu_when(9), 6);
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
        let (r, v) = source_a(0x1234_0000, 0, 0, 1, 1);
        assert_eq!(r, SOURCE_A);
        assert_eq!(v, 0x1234_4100, "a=0 b=0 c=1 d=1, pattern bits kept");
    }

    #[test]
    fn pattern_words_pack_slots_and_inverts() {
        // SetSourcePatternA(2,1,2,1) with a cleared shadow
        let (_, v) = pattern_a(0, 2, true, 2, true);
        assert_eq!(v, 2 << 16 | 1 << 19 | 2 << 20 | 1 << 23);
        // the rest of the register is kept
        let (_, v) = pattern_a(0xff00_ffff, 1, false, 0, true);
        assert_eq!(v, 0xff00_ffff | 1 << 16 | 1 << 23, "s1 = 1 and i2 set");
    }

    #[test]
    fn trigger_times_pack_both_high_halves() {
        // t1 = 2^33 ns, t2 = 3 * 2^32 ns: TIME[2] packs both 16-bit high halves
        let w = trigger_a(0, 4, 1 << 33, 3 << 32);
        assert_eq!(w[1], (TIME_A[0], 0), "2^33 ns truncated to 32 bits");
        assert_eq!(w[2], (TIME_A[1], 0));
        let w = trigger_a(0, 4, (5 << 32) | 5, (7 << 32) | 7);
        assert_eq!(w[1], (TIME_A[0], 5));
        assert_eq!(w[2], (TIME_A[1], 7));
        assert_eq!(w[3], (TIME_A[2], 5 | 7 << 16), "both high halves packed");
        // the event goes to bits 24-26 and the rest is kept
        let w = trigger_a(0x1234_5678, 5, 0, 0);
        assert_eq!(w[0], (SOURCE_A, 0x1234_5678 & !0x0700_0000 | 5 << 24));
    }

    #[test]
    fn logic_operations_pack_terms_and_output() {
        let mut p = (0u32, 0u32);
        let w = logic_a(&mut p, 0, 2, 1);
        assert_eq!(w[0], (LOGIC_A[0], 2), "term 0 in bits 0-1");
        assert_eq!(w[1], (LOGIC_A[1], 1 << 10), "output op in bits 10-11");
        let w = logic_a(&mut p, 15, 3, 1);
        assert_eq!(w[0], (LOGIC_A[0], 2 | 3 << 30), "term 15 in bits 30-31");
        // terms 16-20 land in LOGIC_A[1] bits 0-9, the op moves to 2
        let w = logic_a(&mut p, 16, 1, 2);
        assert_eq!(w[0], (LOGIC_A[1], 1 | 2 << 10));
        let mut p2 = (0u32, 0u32);
        let w = logic_b(&mut p2, 0, 2, 1);
        assert_eq!(w[0].0, LOGIC_B[0]);
        assert_eq!(w[1].0, LOGIC_B[1]);
        let mut p3 = (0u32, 0u32);
        let w = logic_r(&mut p3, 0, 2, 1);
        assert_eq!(w[0].0, LOGIC_R[0]);
        assert_eq!(w[1].0, LOGIC_R[1]);
    }

    #[test]
    fn mode_helpers_set_their_bits() {
        assert_eq!(spu_bypass(0, true), (MODE, 0x1000));
        assert_eq!(force(0x20), [(MODE, 0x20), (MODE, 0)], "pulse bit 5");
        assert_eq!(abr_rst(0), [(MODE, 0x800), (MODE, 0)], "pulse bit 11");
        assert_eq!(abr_fsm(0, true, true, true, false, true), (MODE, 0x5c0));
        assert_eq!(trigger_mode(0x5c0, MODE_NTH), (MODE, 0x5c3));
        assert_eq!(delaytime(8), [(DELAY[0], 2), (DELAY[1], 0)], "4 ns ticks");
        assert_eq!(r_timeout(8), [(R_TIMEOUT[0], 2), (R_TIMEOUT[1], 0)]);
    }

    #[test]
    fn holdoff_and_iir_pack_their_bits() {
        let w = holdoff_time(8_000_000);
        assert_eq!(w[0], (HOLD_TIME[0], 2), "8 ms = 2 x 4 ms");
        assert_eq!(w[1], (HOLD_TIME[1], 0));
        assert_eq!(holdoff_type(3), (HOLD_TIME[1], 3 << 16));
        assert_eq!(iir(1, 2, 0x800), (IIR, 1 | 2 << 2 | 0x800 << 4));
    }

    #[test]
    fn edge_rising_programs_the_channel_comparators_and_mode_zero() {
        let w = edge(2, 32000, 33000, true, false);
        let rising_word = 2u32 | 3 << 8 | 1 << 14 | 2 << 16 | 1 << 19 | 2 << 20 | 1 << 23;
        assert!(
            w.contains(&(SOURCE_A, rising_word)),
            "a=2, c=3, d=1 plus pattern (2,1,2,1)"
        );
        assert!(w.contains(&(LEVEL_A[1], 33000u32 << 16 | 32000)));
        assert_eq!(*w.last().unwrap(), (LEVEL_A[1], 33000u32 << 16 | 32000));
        assert!(w.iter().any(|&(a, _)| a == TIME_A[0]));
        assert!(w.contains(&(MODE, MODE_EDGE as u32)));
    }

    #[test]
    fn edge_falling_swaps_the_comparators() {
        let w = edge(1, 1, 2, false, false);
        assert!(
            w.contains(&(SOURCE_A, 1u32 | 1 << 6 | 2 << 16 | 2 << 20)),
            "falling: b=1 a=0, pattern (2,0,2,0)"
        );
        assert!(w.contains(&(MODE, MODE_EDGE as u32)));
    }

    #[test]
    fn either_edge_sets_mode_one_and_the_b_side() {
        let w = edge(1, 1, 2, true, true);
        assert!(w.iter().any(|&(a, _)| a == SOURCE_B));
        assert!(w.contains(&(MODE, MODE_A_B as u32)));
    }

    #[test]
    fn ac_line_and_ext_edges_use_the_fixed_source_codes() {
        let w = edge_line(true, false);
        assert!(w.contains(&(SOURCE_A, 0x30u32 | 0x30 << 8 | 2 << 16 | 1 << 19 | 2 << 20 | 1 << 23)));
        let w = edge_line(false, true);
        assert!(w.contains(&(SOURCE_A, 0x31u32 | 0x31 << 8)));
        assert!(w.contains(&(EXT_HYS, 1 << 16)));
    }

    #[test]
    fn pulse_packs_polarity_when_and_widths() {
        let w = pulse(1, false, 2, 1000, 2000);
        assert!(w.contains(&iir(0, 1, 0x800)));
        // falling: source (1,1,0,0), pattern (2,1,1,0)
        assert!(w.contains(&(SOURCE_A, 2 << 24 | 1 | 1 << 6)), "source write keeps the event bits");
        assert!(w.contains(&(SOURCE_A, 2 << 24)), "event word from the reset state");
        assert!(w.contains(&(SOURCE_A, 2 << 24 | 1 | 1 << 6 | 2 << 16 | 1 << 19 | 1 << 20)));
        assert!(w.contains(&(TIME_A[0], 1000)));
        assert!(w.contains(&(TIME_A[1], 2000)));
        assert_eq!(*w.last().unwrap(), (MODE, MODE_EDGE as u32));
    }

    #[test]
    fn runt_either_programs_both_events() {
        let w = runt(3, 2, 1, 500, 600);
        // polarity 2: A (4,0,5,0) + pattern (3,0,1,0); B (4,1,5,1) + pattern (3,1,1,0)
        assert!(w.contains(&(SOURCE_A, 4u32 | 5 << 8 | 3 << 16 | 1 << 20)));
        assert!(w.contains(&(SOURCE_B, 4u32 | 1 << 6 | 5 << 8 | 1 << 14 | 3 << 16 | 1 << 19 | 1 << 20)));
        assert!(w.contains(&(SOURCE_A, 1 << 24 | 4 | 5 << 8 | 3 << 16 | 1 << 20)));
        assert!(w.contains(&(TIME_A[0], 500)));
        assert!(w.contains(&(MODE, MODE_A_B as u32)));
    }

    #[test]
    fn runt_no_condition_zeroes_the_event_words() {
        let w = runt(1, 0, 5, 500, 600);
        assert!(w.contains(&(TIME_A[0], 0)));
        let word = w.iter().find(|&&(r, _)| r == SOURCE_A).map(|&(_, v)| v).unwrap();
        assert_eq!(word & 0x0700_0000, 0, "event bits cleared");
    }

    #[test]
    fn window_timed_uses_event_four_with_the_same_time() {
        let w = window(1, 0, 2, 7000);
        assert!(w.contains(&(SOURCE_A, 1 << 8 | 4 << 24)));
        assert!(w.contains(&(TIME_A[0], 7000)));
        assert!(w.contains(&(TIME_A[1], 7000)));
        assert!(w.contains(&(MODE, MODE_EDGE as u32)));
        let w = window(1, 3, 0, 0);
        assert!(w.iter().any(|&(a, _)| a == SOURCE_B));
        assert!(w.contains(&(MODE, MODE_A_B as u32)));
    }

    #[test]
    fn timeout_sets_event_four_on_both_events() {
        let w = timeout(2, 3, 9000);
        assert!(w.contains(&(TIME_A[0], 9000)));
        assert!(w.contains(&(TIME_B[0], 9000)));
        assert!(w.contains(&(MODE, MODE_A_B as u32)));
        let w = timeout(2, 0, 9000);
        assert!(w.contains(&(MODE, MODE_EDGE as u32)));
    }

    #[test]
    fn delay_swaps_sources_per_slope_pair() {
        // slopes (0,0): B-side of ch2 first, A-side of ch1 second, pattern (1,0,1,0)
        let w = delay(1, 2, 0, 0, 1, 10, 20);
        assert!(w.contains(&(SOURCE_A, 2u32 | 1 << 16 | 1 << 20 | 1 << 24)));
        assert!(w.contains(&(TIME_A[0], 10)));
        // slopes (1,1): both B-sides, inverted
        let w = delay(1, 2, 1, 1, 1, 10, 20);
        assert!(w.contains(&(SOURCE_A, 3u32 | 1 << 6 | 1 << 8 | 1 << 14 | 1 << 16 | 1 << 20 | 1 << 24)));
    }

    #[test]
    fn pattern_terms_and_edge_selection_match_the_stock_code() {
        assert_eq!(to_logic_sel(0), 2);
        assert_eq!(to_logic_sel(1), 3);
        assert_eq!(to_logic_sel(2), 0);
        assert_eq!(to_tpu_logic_fun(1), 1);
        assert_eq!(to_tpu_logic_fun(3), 2);
        assert_eq!(to_tpu_logic_fun(0), 0);
        let mut terms = [0u8; 21];
        terms[2] = 3; // first rising edge on channel 3
        terms[5] = 4;
        assert_eq!(select_pattern_edge(&terms), (2, Some(2)));
        let w = pattern(&terms, 1);
        assert!(w.contains(&(MODE, MODE_EDGE as u32)));
        // 21 logic writes + 4 iir + source/pattern/trigger/mode
        assert!(w.len() >= 21 + 8);
        // the edge source is the A-side comparator of channel 3, d-slot 2, pattern (2,1,0,0)
        assert!(w.contains(&(SOURCE_A, 4u32 | 4 << 8 | 2 << 14 | 2 << 16 | 1 << 19)));
    }

    #[test]
    fn duration_keeps_the_pattern_edge_in_the_b_slot() {
        let mut terms = [0u8; 21];
        terms[0] = 4; // falling edge on channel 1
        let w = duration(&terms, 1, 2, 300, 400);
        assert!(w.contains(&(TIME_A[0], 300)));
        assert!(w.contains(&(SOURCE_A, 1 << 6 | 2 << 14 | 2 << 20 | 2 << 24)));
        assert!(w.contains(&(MODE, MODE_EDGE as u32)));
    }

    #[test]
    fn nth_edge_arms_the_abr_machine() {
        let w = nth(1, true, 7, 1_000_000);
        assert!(w.contains(&(MODE, 0x5c0)));
        assert!(w.contains(&(MODE, 0x5c3)), "mode 3 keeps the FSM bits");
        assert!(w.contains(&(ABR_EVENT_A, 7)));
        assert!(w.contains(&(R_TIMEOUT[0], 250_000)), "1 ms = 250k 4-ns ticks");
    }

    #[test]
    fn setup_hold_modes_pick_their_sources() {
        // setup: clock A-side, data B-side, pattern (2, clk_rising, 3, data_rising)
        let w = setup_hold(1, 2, true, true, 0, 500);
        assert!(w.contains(&(SOURCE_A, 3 << 8 | 2 << 16 | 1 << 19 | 3 << 20 | 1 << 23)));
        assert!(w.contains(&(TIME_A[1], 500)));
        assert!(w.contains(&(MODE, MODE_EDGE as u32)));
        let w = setup_hold(1, 2, true, true, 2, 500);
        assert!(w.iter().any(|&(a, _)| a == SOURCE_B));
        assert!(w.contains(&(MODE, MODE_A_B as u32)));
    }

    #[test]
    fn video_constants_match_the_stock_table() {
        assert_eq!(video_constants(0), ((0x1824_4f00, 0xb028_3580u32), (5, 0x20d, false)));
        assert_eq!(video_constants(1), ((0x1824_4f00, 0xce8e_3100u32), (2, 0x271, false)));
        assert_eq!(video_constants(6), ((3_500_000_000u32, 0x3900_6480u32), (2, 0x2ee, true)));
        assert_eq!(video_constants(9), ((0x69fa_2080u32, 0x6166_5bc0u32), (2, 0x465, true)));
    }

    #[test]
    fn video_composition_routes_the_channel_over_the_r_event() {
        let w = video(2, true, 0, 11);
        assert!(w.contains(&(SOURCE_R, 2u32 | 2 << 6 | 3 << 8 | 2 << 14)));
        assert!(w.contains(&(MODE, MODE_TV as u32)));
        assert!(w.contains(&(VIDEO[0], 1 | 11 << 2)), "polarity bit + line");
    }
}
