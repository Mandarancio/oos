// Copyright (C) 2026 Julian Decker
// SPDX-License-Identifier: GPL-2.0-or-later

//! DHO1000 hardware DVM, reconstructed from the stock library.
//!
//! Evidence: `DevAnalyzeDvm_SetDvm` @0x2b93b4 and `DevAnalyzeDvm_GetDvm` @0x2b9424 (decompiled in
//! `docs/decomp/libscope`), the gate-time handling of `DrvDvm_SetGateTime` @0x30b654 and the
//! software modes of `DvmData::getDcRmsMaxMin`. Not yet verified on hardware.
//!
//! The DVM integrates in the FPGA: the configuration goes to 0x2244 (the enable bit is written
//! first on its own, as a soft start), a read latches 0x2248 and both accumulators come out —
//! the 48-bit sum of the samples from 0x224c/0x2250 and the 64-bit sum of squares from
//! 0x2254/0x2258 — each divided by `1 << shift` samples. The AC/DC modes are computed in
//! software from the two averages.

use crate::dho1000::Write;

/// DVM configuration: bit 0 enable, bits 1-5 mode, bits 8-9 channel.
pub const DVM_CFG: u32 = 0x2244;
/// DVM read latch (bit 0 set, then cleared, to freeze the accumulators).
pub const DVM_LATCH: u32 = 0x2248;
/// DVM sum, low 32 bits.
pub const DVM_SUM_L: u32 = 0x224c;
/// DVM sum, high bits (16-31 of this word extend the sum).
pub const DVM_SUM_H: u32 = 0x2250;
/// Sum of squares, low and high 32 bits (`DevAnalyzeDvm_GetDvm` @0x2b9424).
pub const DVM_SQ_L: u32 = 0x2254;
pub const DVM_SQ_H: u32 = 0x2258;

/// `DevAnalyzeDvm_SetDvm(enable, channel, mode)`: bit 0 enable, bits 1-5 mode, bits 8-9 channel;
/// bits 6-7 and 10-15 are kept. The stock code writes the enable bit on its own first.
pub fn set_dvm(sh: u32, enable: bool, mode: u8, channel: usize) -> (u32, u32) {
    let mut v = (sh & 0xfcc0) | (u32::from(mode) & 0x1f) << 1 | (channel as u32 & 3) << 8;
    if enable {
        v |= 1;
    }
    (DVM_CFG, v)
}

/// The stock two-step configuration (`SetDvm` writes the enable bit alone before the full word),
/// so the block comes up softly. The mode field carries the channel mux in half-channel mode
/// (`DrvDvm_SetGateTime` passes 0 or 2 there); the gate length itself is the `shift` of the read.
pub fn set_dvm_seq(sh: u32, enable: bool, mode: u8, channel: usize) -> Vec<Write> {
    let mut w = Vec::with_capacity(2);
    if enable {
        w.push((DVM_CFG, (sh & 0xfcc0) | 1));
    }
    w.push(set_dvm(sh, enable, mode, channel));
    w
}

/// Latch and decode one DVM reading: the sum is `(sum_h >> 16) << 32 | sum_l`, the average is that
/// over `1 << shift`.
pub fn dvm_average(sum_l: u32, sum_h: u32, shift: u32) -> f64 {
    let sum = (u64::from(sum_h >> 16) << 32) | u64::from(sum_l);
    sum as f64 / (1u64 << (shift & 0x1f)) as f64
}

/// Both averages of one reading (`GetDvm` divides the sum and the sum of squares by the same
/// `1 << shift`): the sample mean and the mean of the squares.
pub fn dvm_sums(sum_l: u32, sum_h: u32, sq_l: u32, sq_h: u32, shift: u32) -> (f64, f64) {
    let n = (1u64 << (shift & 0x1f)) as f64;
    let sum = (u64::from(sum_h >> 16) << 32) | u64::from(sum_l);
    let sq = (u64::from(sq_h) << 32) | u64::from(sq_l);
    (sum as f64 / n, sq as f64 / n)
}

/// The three DVM modes from the two averages (`DvmData::getDcRmsMaxMin`): DC, AC rms (the square
/// root of the variance) and AC+DC rms.
pub fn dvm_modes(mean: f64, mean_sq: f64) -> (f64, f64, f64) {
    let variance = (mean_sq - mean * mean).max(0.0);
    (mean, variance.sqrt(), mean_sq.sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_packs_enable_mode_and_channel() {
        assert_eq!(set_dvm(0, true, 0, 0).1 & 1, 1, "enabled");
        assert_eq!(set_dvm(0, false, 0, 0).1 & 1, 0, "disabled");
        let (r, v) = set_dvm(0xfcc0, true, 5, 2);
        assert_eq!(r, DVM_CFG);
        assert_eq!(v & 0x3e, 5 << 1, "mode in bits 1-5");
        assert_eq!(v & 0x300, 2 << 8, "channel in bits 8-9");
        assert_eq!(v & 0xfcc0, 0xfcc0, "other bits kept");
    }

    #[test]
    fn soft_start_writes_the_enable_bit_alone() {
        let w = set_dvm_seq(0, true, 3, 1);
        assert_eq!(w.len(), 2);
        assert_eq!(w[0], (DVM_CFG, 1), "enable first, on its own");
        assert_eq!(w[1], set_dvm(0, true, 3, 1));
        let w = set_dvm_seq(0, false, 3, 1);
        assert_eq!(w.len(), 1, "no soft start when disabling");
    }

    #[test]
    fn average_joins_the_48_bit_sum() {
        // sum = 0x1_0000_0005 (high half 1, low 0x5), shift 4 -> 2^32+5 over 16
        let avg = dvm_average(5, 1 << 16, 4);
        assert!((avg - (4294967301.0 / 16.0)).abs() < 1e-3);
        assert_eq!(
            dvm_average(16, 0, 4),
            1.0,
            "shift 4 = average of 16 readings"
        );
    }

    #[test]
    fn sums_average_both_accumulators() {
        // 16 readings of value 2: sum 32, sum of squares 64
        let (mean, mean_sq) = dvm_sums(32, 0, 64, 0, 4);
        assert_eq!(mean, 2.0);
        assert_eq!(mean_sq, 4.0);
        // a 64-bit sum of squares needs the high word
        let (_, mean_sq) = dvm_sums(0, 0, 0, 1, 4);
        assert_eq!(mean_sq, (1u64 << 28) as f64, "2^32 over 2^4");
    }

    #[test]
    fn modes_split_dc_and_ac() {
        // DC 2 with 1 Vrms of AC: mean 2, mean of squares 5
        let (dc, ac, acdc) = dvm_modes(2.0, 5.0);
        assert_eq!(dc, 2.0);
        assert!((ac - 1.0).abs() < 1e-12);
        assert!((acdc - 5.0f64.sqrt()).abs() < 1e-12);
        // numerical noise below zero is clamped
        let (_, ac, _) = dvm_modes(2.0, 4.0 - 1e-9);
        assert_eq!(ac, 0.0);
    }
}
