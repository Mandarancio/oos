// Copyright (C) 2026 Julian Decker
// SPDX-License-Identifier: GPL-2.0-or-later

//! DHO1000 record/sequence engine (SCU side), reconstructed from the stock library and the
//! app-free FSM analysis.
//!
//! Evidence: `DevSystemScu_SetRecordSeq` @0x2aea98, `setRecordPlay` @0x2aeb8c,
//! `DevSystemSCU_SetRecord` @0x2aef7c, `SetRecordSeqEn` @0x2af040, `setSeqPlay` @0x2af068,
//! `setPlayInfoClr` @0x2af228, `getPlayInfo` @0x2af17c, `getTimeStamp` @0x2af31c,
//! `SetAutoTrigger` @0x2af280 (decompiled in `docs/decomp/libscope`); the auto-trigger
//! watchdog clamps are from the stock FSM analysis (`docs/ACQUISITION_FSM.md` in the sidecar
//! notes: without them the engine records once and stalls). The DHO800/900 counterpart is
//! `crate::dho::{scu,seg}`. **Nothing here has run on hardware yet** except the stall symptom.
//!
//! Register map:
//!
//! | Register | Use |
//! |---|---|
//! | `CTRL` | record control: arm/commit patterns, sequence enable, direction, mode |
//! | `START`/`CURRENT`/`END` | frame positions: one arm records `count` frames self-re-arming |
//! | `INTERVAL` / `TIMER` | frame interval and FSM timer, 4 ns ticks (bit 31 of the timer overflows) |
//! | `PLAY` | play state (bit 30) and clear pulse (bit 31) |
//! | `AUTO_TIMEOUT` / `AUTO_HOLD` | the auto-trigger watchdog, 4 ns ticks |
//! | `TIME_STAMP` / `STAMP_L`/`STAMP_H` | per-record trigger timestamp, 64-bit |
//! | `STATUS` | bit 13: stopped |

use crate::dho1000::zone::SCU_CTRL;
use crate::dho1000::Write;

/// Record control (`0x400c`): bit 25 sequence enable, bit 26 mode, bits 28-31 the arm/commit
/// patterns the setters below compose.
pub const CTRL: u32 = 0x400c;
/// Frame positions: start, current, end (`getPlayInfo` counts `START - CURRENT + 1` done).
pub const START: u32 = 0x4010;
pub const CURRENT: u32 = 0x4014;
pub const END: u32 = 0x4018;
/// Frame interval in 4 ns ticks (`0x4068`).
pub const INTERVAL: u32 = 0x4068;
/// FSM timer in 4 ns ticks, bit 31 set on overflow (`0x406c`).
pub const TIMER: u32 = 0x406c;
/// Play state: bit 30 = a play is running, bit 31 = the clear pulse (`0x4080`).
pub const PLAY: u32 = 0x4080;
/// Auto-trigger timeout and holdoff in 4 ns ticks (`SetAutoTrigger` writes `0x4060`/`0x4064`).
pub const AUTO_TIMEOUT: u32 = 0x4060;
pub const AUTO_HOLD: u32 = 0x4064;
/// Per-record trigger timestamp: strobe `0x40b8` bit 0, value at `0x40b0`/`0x40b4`.
pub const STAMP_CTRL: u32 = 0x40b8;
pub const STAMP_L: u32 = 0x40b0;
pub const STAMP_H: u32 = 0x40b4;
/// FSM status: bit 13 = stopped (the free-running record engine clears it between records).
pub const STATUS: u32 = 0x4034;

/// The timer word of `SetRecordSeq`/`setRecordPlay`: 4 ns ticks in bits 0-30, bit 31 on overflow.
pub fn timer(timer_fs: u64) -> u32 {
    let ticks = timer_fs / 4_000_000;
    ((ticks as u32) & 0x7fff_ffff) | u32::from(ticks > 0x7fff_ffff) << 31
}

/// `DevSystemScu_SetRecordSeq` @0x2aea98: arm the sequence recorder for `count` frames from
/// `start`, the FPGA re-arming itself per frame. The interval goes in as femtoseconds.
pub fn set_record_seq(
    sh: &mut u32,
    interval_fs: u64,
    start: u32,
    count: u32,
    timer_fs: u64,
) -> Vec<Write> {
    let mut w = Vec::with_capacity(10);
    w.push((TIMER, timer(timer_fs)));
    w.push((INTERVAL, (interval_fs / 4_000_000) as u32));
    *sh = (*sh & 0x89ff_ffff) | 0x3000_0000;
    w.push((CTRL, *sh));
    w.push((START, start));
    w.push((CURRENT, start));
    w.push((END, start + count - 1));
    *sh &= 0xefff_ffff;
    w.push((CTRL, *sh));
    *sh |= 0x2200_0000;
    w.push((CTRL, *sh));
    w
}

/// `DevSystemSCU_SetRecord` @0x2aef7c: plain recording to frame `end`.
pub fn set_record(sh: &mut u32, enable: bool, interval_fs: u64, end: u32) -> Vec<Write> {
    let mut w = Vec::with_capacity(9);
    w.push((INTERVAL, (interval_fs / 4_000_000) as u32));
    *sh = (*sh & 0x89ff_ffff) | 0x3000_0000;
    w.push((CTRL, *sh));
    w.push((START, 0));
    w.push((CURRENT, 0));
    w.push((END, end));
    *sh &= 0xefff_ffff;
    w.push((CTRL, *sh));
    *sh = (*sh & 0xfbff_ffff) | u32::from(enable) << 26 | 0xa000_0000;
    w.push((CTRL, *sh));
    w
}

/// `DevSystemScu_setRecordPlay` @0x2aeb8c: play frames `p5..p7` back through the read-out chain.
pub fn set_record_play(
    sh: &mut u32,
    direction: bool,
    mode: bool,
    interval_fs: u64,
    timer_fs: u64,
    p5: u32,
    p6: u32,
    p7: u32,
) -> Vec<Write> {
    let mut w = Vec::with_capacity(11);
    w.push((INTERVAL, (interval_fs / 4_000_000) as u32));
    w.push((TIMER, timer(timer_fs)));
    *sh = (*sh & 0x89ff_ffff) | 0x3000_0000;
    w.push((CTRL, *sh));
    w.push((START, p5));
    w.push((CURRENT, p6));
    w.push((END, p7));
    *sh &= 0xefff_ffff;
    w.push((CTRL, *sh));
    *sh = u32::from(direction) << 29
        | u32::from(mode) << 26
        | (*sh & 0x9bff_ffff)
        | 0x4000_0000;
    w.push((CTRL, *sh));
    w
}

/// `DevSystemScu_setSeqPlay` @0x2af068: like `set_record_play` but the interval is written raw
/// (already in ticks) and the control word takes the sequence-play bit pattern.
pub fn set_seq_play(
    sh: &mut u32,
    flag: bool,
    mode: bool,
    interval_ticks: u32,
    start: u32,
    current: u32,
    end: u32,
) -> Vec<Write> {
    let mut w = Vec::with_capacity(10);
    w.push((INTERVAL, interval_ticks));
    *sh = (*sh & 0x0fff_ffff) | 0x3000_0000;
    w.push((CTRL, *sh));
    w.push((START, start));
    w.push((CURRENT, current));
    w.push((END, end));
    *sh &= 0xefff_ffff;
    w.push((CTRL, *sh));
    *sh = u32::from(flag) << 29 | u32::from(mode) << 26 | (*sh & 0xdaff_ffff) | 0x0100_0000;
    w.push((CTRL, *sh));
    w
}

/// `DevSystemScu_SetRecordSeqEn` @0x2af040: bit 25 of the record control.
pub fn set_record_seq_en(sh: &mut u32, on: bool) -> Write {
    *sh = (*sh & 0xfdff_ffff) | u32::from(on) << 25;
    (CTRL, *sh)
}

/// `DevSystemScu_setPlayInfoClr` @0x2af228: the bit-31 clear pulse of `PLAY`.
pub fn play_info_clr() -> [Write; 2] {
    [(PLAY, 0x8000_0000), (PLAY, 0)]
}

/// `DevSystemScu_getPlayInfo` @0x2af17c: the play-done flag from bit 30 of `PLAY` and the number
/// of frames recorded, `START - CURRENT + 1`.
pub fn play_info(play_reg: u32, start: u32, current: u32) -> (bool, u32) {
    (play_reg >> 30 & 1 == 1, start.wrapping_sub(current) + 1)
}

/// `DevSystemSCU_getTimeStamp` @0x2af31c: the bit-0 strobe of `STAMP_CTRL` (cleared, set,
/// cleared), then the 64-bit timestamp from `STAMP_L`/`STAMP_H`.
pub fn time_stamp_strobe() -> [Write; 3] {
    [(STAMP_CTRL, 0), (STAMP_CTRL, 1), (STAMP_CTRL, 0)]
}

pub fn time_stamp(low: u32, high: u32) -> u64 {
    u64::from(high) << 32 | u64::from(low)
}

/// `DevSystemSCU_SetAutoTrigger` @0x2af280: the auto-trigger bit of the SCU control and the
/// watchdog pair in 4 ns ticks. **This is the missing piece for a standalone record flow**: the
/// stock FSM derives the two times from the time base (see `auto_watchdog`) and without them the
/// engine produces one record and stalls.
pub fn set_auto_trigger(sh: u32, auto_trigger: bool, timeout_fs: u64, hold_fs: u64) -> [Write; 3] {
    [
        (SCU_CTRL, (sh & 0xfffe) | u32::from(auto_trigger)),
        (AUTO_TIMEOUT, (timeout_fs / 4_000_000) as u32),
        (AUTO_HOLD, (hold_fs / 4_000_000) as u32),
    ]
}

/// The stock FSM's auto-trigger watchdog times (`ACQUISITION_FSM.md`): the timeout is the
/// time-per-division setting times 20, clamped to 10..200 s; the holdoff is the greater of the
/// trigger holdoff and half the timeout, at least 5 s. Returns (timeout, holdoff) in fs.
pub fn auto_watchdog(tdiv_fs: u64, holdoff_fs: u64) -> (u64, u64) {
    let tmo = (tdiv_fs * 20).clamp(10_000_000_000_000_000, 200_000_000_000_000_000);
    let hold = (holdoff_fs).max(tmo / 2).max(5_000_000_000_000_000);
    (tmo, hold)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timer_packs_ticks_and_the_overflow_bit() {
        assert_eq!(timer(4_000_000), 1);
        assert_eq!(timer(0x7fff_ffff * 4_000_000), 0x7fff_ffff);
        assert_eq!(timer(0x8000_0000 * 4_000_000), 0x8000_0000, "overflow bit");
    }

    #[test]
    fn record_seq_arms_start_to_start_plus_count() {
        let mut sh = 0u32;
        let w = set_record_seq(&mut sh, 4_000_000, 5, 3, 0);
        assert_eq!(w[0], (TIMER, 0));
        assert_eq!(w[1], (INTERVAL, 1));
        assert!(w.contains(&(START, 5)));
        assert!(w.contains(&(CURRENT, 5)));
        assert!(w.contains(&(END, 7)), "start + count - 1");
        assert_eq!(*w.last().unwrap(), (CTRL, 0x2200_0000));
        assert_eq!(sh, 0x2200_0000);
    }

    #[test]
    fn record_and_seq_play_compose_their_patterns() {
        let mut sh = 0u32;
        let w = set_record(&mut sh, true, 4_000_000, 9);
        assert_eq!(*w.last().unwrap(), (CTRL, 1 << 26 | 0xa000_0000));
        let mut sh = 0u32;
        let w = set_record_play(&mut sh, true, false, 4_000_000, 0, 1, 2, 3);
        assert!(w.contains(&(START, 1)));
        assert_eq!(*w.last().unwrap(), (CTRL, 1 << 29 | 0x4000_0000));
        let mut sh = 0u32;
        let w = set_seq_play(&mut sh, true, true, 42, 1, 2, 3);
        assert_eq!(w[0], (INTERVAL, 42), "the interval goes in raw");
        assert_eq!(*w.last().unwrap(), (CTRL, 1 << 29 | 1 << 26 | 0x0100_0000));
    }

    #[test]
    fn seq_enable_pulses_bit_25() {
        let mut sh = 0u32;
        assert_eq!(set_record_seq_en(&mut sh, true), (CTRL, 1 << 25));
        assert_eq!(set_record_seq_en(&mut sh, false), (CTRL, 0));
    }

    #[test]
    fn play_info_and_clear_behave_like_the_stock_code() {
        assert_eq!(play_info(1 << 30, 10, 4), (true, 7), "start - current + 1");
        assert_eq!(play_info(0, 10, 4), (false, 7));
        assert_eq!(play_info_clr(), [(PLAY, 0x8000_0000), (PLAY, 0)]);
    }

    #[test]
    fn time_stamp_strobe_and_join() {
        assert_eq!(time_stamp_strobe(), [(STAMP_CTRL, 0), (STAMP_CTRL, 1), (STAMP_CTRL, 0)]);
        assert_eq!(time_stamp(0xffff_ffff, 1), 0x1_ffff_ffff);
    }

    #[test]
    fn auto_trigger_writes_the_watchdog_pair() {
        let w = set_auto_trigger(0, true, 40_000_000, 4_000_000);
        assert_eq!(w[0], (SCU_CTRL, 1));
        assert_eq!(w[1], (AUTO_TIMEOUT, 10), "40 ns = 10 x 4 ns");
        assert_eq!(w[2], (AUTO_HOLD, 1));
    }

    #[test]
    fn watchdog_clamps_match_the_fsm() {
        // a fast time base clamps up to the 10 s floor, holdoff to 5 s
        assert_eq!(auto_watchdog(1_000_000, 0), (10_000_000_000_000_000, 5_000_000_000_000_000));
        // a slow one clamps to the 200 s ceiling
        assert_eq!(auto_watchdog(1e16 as u64, 0).0, 200_000_000_000_000_000);
        // the holdoff wins when it is larger than half the timeout
        let (tmo, hold) = auto_watchdog(1e15 as u64, 60_000_000_000_000_000);
        assert_eq!(tmo, 20_000_000_000_000_000);
        assert_eq!(hold, 60_000_000_000_000_000);
    }
}
