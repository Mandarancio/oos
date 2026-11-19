// Copyright (C) 2026 Julian Decker
// SPDX-License-Identifier: GPL-2.0-or-later

//! DHO1000 series: acquisition register sequences, measured on a DHO1204 (docs/protocols/dho1000.md).
//!
//! Two paths were brought up on the unit, both with the FPGA's own sequencer and no stock software:
//! a normal-mode record/play ("batches") and roll. The functions below return `(offset, value)` writes
//! in order and keep software shadows of the write-only registers, like the DHO800/DHO900 sequences
//! (`crate::dho::scu`); applying them is the bus's job.
//!
//! Status: the init + batch sequence was verified on the unit (clean 1 000 000-sample captures at
//! 10 MSa/s, no gaps, no duplicate batches). Roll was verified at 20..200 kSa/s; the rate is set by
//! the time base, not by the reader, and 10 MSa/s is **not** reachable in roll mode (measurements in
//! the doc). The decode assumes 2-byte samples, all channels interleaved.

/// Software copies of the write-only registers these sequences read-modify-write.
#[derive(Clone, Copy, Debug, Default)]
pub struct Shadow {
    pub s4004: u32,
    pub s4008: u32,
    pub s400c: u32,
    pub s1004: u32,
    pub s1010: u32,
    pub s105c: u32,
}

/// Register 0x4004: run (bit0), stop (bit31), abort (bit30), arm batch (bit19).
pub const CTRL: u32 = 0x4004;
/// Register 0x4008: sequencer mask (bits 0-3), roll (bit17), scan (bit22).
pub const MODE: u32 = 0x4008;
/// Register 0x400c: record/play control (bit28 trigger, bit29 play, bit30 run).
pub const PLAY: u32 = 0x400c;
/// SPU control block (reset pulses, rate, depth, mode).
pub const SPU: u32 = 0x1000;

/// Value of 0x400c in play mode during a batch.
pub const PLAY_BATCH: u32 = 0x6000_4000;
/// Value of 0x400c once a batch is read (idle; bit30 clear is what the stock software leaves).
pub const PLAY_IDLE: u32 = 0xa200_8000;
/// Record buffer size word written at 0x10e0.
pub const MEM_SIZE: u32 = 0x0040_0000;
/// Low byte written at 0x105c.
pub const S105C_LOW: u32 = 0x55;

/// Stop the engine, reset SPU/TX/preproc and set up normal (non-roll) mode for `depth` samples.
/// The sequence is the one that was verified: replaying the stock software's live registers from a
/// snapshot does **not** restart the stream, a from-scratch init does.
pub fn init(sh: &mut Shadow, depth: u32, mask: u8) -> Vec<(u32, u32)> {
    let mut w = Vec::with_capacity(32);
    sh.s4004 = (sh.s4004 & !1) | 0x8000_0000;
    w.push((CTRL, sh.s4004)); // stop
    sh.s4004 &= !4;
    w.push((CTRL, sh.s4004)); // abort clear
    for bits in [1u32, 1 << 4, 1 << 5, 1] {
        // pulse: clear, set, clear (init, TX reset, preproc reset, init)
        w.push((SPU, 0));
        w.push((SPU, bits));
        w.push((SPU, 0));
    }
    sh.s105c = (sh.s105c & !0xff) | S105C_LOW;
    w.push((0x105c, sh.s105c));
    sh.s1010 |= 0x2000_0000;
    w.push((0x1010, sh.s1010));
    w.push((0x10e0, MEM_SIZE));
    w.push((0x10e4, depth));
    sh.s1004 &= !3;
    w.push((0x1004, sh.s1004)); // normal acquisition mode
    sh.s4008 = (sh.s4008 & !0x0040_0000) & !(1 << 17) | (mask as u32 & 0x0f);
    w.push((MODE, sh.s4008));
    w.push((0x4068, 0));
    w.push((0x406c, 0));
    sh.s400c |= 0x3000_0000;
    w.push((PLAY, sh.s400c));
    w.push((0x4010, 0x0000_00f0));
    w.push((0x4014, 0));
    w.push((0x4018, 0x0000_00f0));
    sh.s400c = (sh.s400c & !(1 << 28)) | 0x6000_0000;
    w.push((PLAY, sh.s400c));
    sh.s4004 |= 1;
    w.push((CTRL, sh.s4004)); // run
    w
}

/// Arm one batch. The FPGA clears bit19 itself once the batch is in the FIFO; the caller must wait
/// for that (about 15 ms measured) before reading, and read exactly `tx * 8` bytes afterwards.
pub fn arm_batch(sh: &mut Shadow) -> Vec<(u32, u32)> {
    let mut w = Vec::with_capacity(5);
    sh.s4004 |= 1 << 19;
    w.push((CTRL, sh.s4004));
    w.push((PLAY, PLAY_BATCH));
    w.push((0x4010, 0x0000_00f0));
    w.push((0x4014, 0x0000_00f0));
    w.push((0x4018, 0x0000_00f0));
    w
}

/// Return to idle after a batch was read.
pub fn batch_idle(sh: &mut Shadow) -> Vec<(u32, u32)> {
    let mut w = Vec::with_capacity(2);
    w.push((PLAY, PLAY_IDLE));
    sh.s4004 &= !(1 << 19);
    w.push((CTRL, sh.s4004));
    w
}

/// Enable roll mode and start: mask, roll bit, scan off, auto-trigger, run.
pub fn roll_start(sh: &mut Shadow, mask: u8) -> Vec<(u32, u32)> {
    let mut w = Vec::with_capacity(4);
    sh.s4008 = (sh.s4008 & !0x0f) | (mask as u32 & 0x0f) | (1 << 17);
    sh.s4008 &= !(1 << 22);
    w.push((MODE, sh.s4008));
    w.push((0x4020, 1)); // auto-trigger
    sh.s4004 |= 1;
    w.push((CTRL, sh.s4004));
    w
}

/// Roll stream rate in samples/s for a record depth and a time base, from the measurements on the
/// unit with a 1 Mpts record: `depth / (100 * seconds_per_div)`, i.e. 20 kSa/s at 0.5 s/div and
/// 200 kSa/s at 0.05.
pub fn roll_rate(depth: u32, s_per_div: f64) -> f64 {
    depth as f64 / (100.0 * s_per_div)
}

/// A time base that implies a rate the record window cannot hold leaves the play positions
/// (`0x4010/0x4014/0x4018`) degenerate and the stream stops — measured with a 100 kpts record at
/// 1 MSa/s; the sidecar's roll drainer clamps shallow records to at least 1 Mpts and the measured
/// roll ceiling is 200 kSa/s (0.05 s/div at 1 Mpts). This is that guard.
pub fn roll_window_ok(depth: u32, s_per_div: f64) -> bool {
    depth >= 1_000_000 && roll_rate(depth, s_per_div) <= 200_000.0
}

/// Changing the channel mask while roll is running freezes the stream (measured app-free): the
/// sequencer re-arm does not survive it. A mask change needs a full stop and `roll_start` again.
pub const MASK_CHANGE_NEEDS_RESTART: bool = true;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_stops_then_runs_with_the_mask() {
        let mut sh = Shadow {
            s4004: 0x809,
            ..Default::default()
        };
        let w = init(&mut sh, 1_000_000, 0b0011);
        assert_eq!(w[0], (CTRL, 0x8000_0808), "stop: bit0 clear, bit31 set");
        assert_eq!(w.last().unwrap(), &(CTRL, 0x8000_0809), "run");
        assert!(
            w.iter()
                .any(|&(a, v)| a == MODE && v & 0x0f == 3 && v & (1 << 17) == 0),
            "normal mode, mask 3"
        );
        assert!(w.iter().any(|&(a, v)| a == 0x10e4 && v == 1_000_000));
        assert!(w.iter().any(|&(a, v)| a == PLAY && v == 0x6000_0000));
    }

    #[test]
    fn batch_arm_and_idle_round_trip() {
        let mut sh = Shadow {
            s4004: 0x809,
            ..Default::default()
        };
        let w = arm_batch(&mut sh);
        assert_eq!(w[0], (CTRL, 0x0008_0809), "arm bit19");
        assert_eq!(w[1], (PLAY, PLAY_BATCH));
        assert_eq!(w[2], (0x4010, 0xf0));
        assert_eq!(w[4], (0x4018, 0xf0));
        let w = batch_idle(&mut sh);
        assert_eq!(w[0], (PLAY, PLAY_IDLE));
        assert_eq!(w[1], (CTRL, 0x0000_0809), "arm bit cleared");
    }

    #[test]
    fn roll_start_sets_roll_and_clears_scan() {
        let mut sh = Shadow {
            s4008: 0x0880_1101,
            ..Default::default()
        };
        let w = roll_start(&mut sh, 0b0001);
        let (_, v) = w[0];
        assert_eq!(v & 0x0f, 1, "mask");
        assert_ne!(v & (1 << 17), 0, "roll");
        assert_eq!(v & (1 << 22), 0, "scan off");
    }

    #[test]
    fn roll_rate_matches_the_measurements() {
        // 20 kSa/s at 0.5 s/div, 50 k at 0.2, 200 k at 0.05 with a 1 Mpts record
        let near = |a: f64, b: f64| (a - b).abs() < 1e-6 * b;
        assert!(near(roll_rate(1_000_000, 0.5), 20_000.0));
        assert!(near(roll_rate(1_000_000, 0.2), 50_000.0));
        assert!(near(roll_rate(1_000_000, 0.1), 100_000.0));
        assert!(near(roll_rate(1_000_000, 0.05), 200_000.0));
        // slower time bases are slower streams, never faster
        assert!(roll_rate(1_000_000, 1.0) < roll_rate(1_000_000, 0.5));
    }

    #[test]
    fn roll_window_guard_matches_the_measured_limits() {
        // the verified combination: 1 Mpts from 0.05 s/div up
        assert!(roll_window_ok(1_000_000, 0.05));
        assert!(roll_window_ok(1_000_000, 1.0));
        // the degenerate measurement: 100 kpts
        assert!(!roll_window_ok(100_000, 1.0), "below the 1 Mpts floor");
        // faster than the measured ceiling
        assert!(!roll_window_ok(25_000_000, 0.05), "250 kSa/s is past the ceiling");
    }
}
