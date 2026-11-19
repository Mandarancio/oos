// Copyright (C) 2026 Julian Decker
// SPDX-License-Identifier: GPL-2.0-or-later

//! DHO1000 sample-processing unit configuration, reconstructed from the stock library and the
//! app-free measurements.
//!
//! Evidence: `DevAcquireSPU_SetSampleRate` @0x2a56a8, `SetAcqMode` @0x2a5558, `SetMemSize`
//! @0x2a57fc, `SetRecordDepth` @0x2a599c, `SetSampleRange` @0x2a5670, `SetPreSaComp` @0x2a569c,
//! `SetAcqAntiAlias` @0x2a4f34, `GetWaveRange` @0x2a5b94 (decompiled in
//! `docs/decomp/libscope`); the divider values were also read live on the unit
//! (`0x10d8 = 25` while streaming at 10 MSa/s, `0x10d0 = 0x8004` at 500 MSa/s).
//! **Nothing here has run on hardware as OOS code.**
//!
//! The stock `ConfigAcquire` bundle applies these in order: channel mode, ADC order, avg-sinc
//! (on below one eighth of the maximum rate), per-channel sample delay, pre-processor reset,
//! channel filter (invert), sample rate, acquire mode, average parameters, pre-sample
//! compensation (5, plus 10 in average mode), anti-alias, record depth, sample range, memory
//! size.

use crate::dho1000::Write;

/// Acquire mode word: bits 1 (average), 2 (sinc, `avg::set_avg_sinc`), 3 (high resolution) and
/// 12 are cleared and set by `SetAcqMode`.
pub const ACQ: u32 = 0x1004;
/// Fast-path rate divider (`0x10d0`): the low byte carries `max_rate / rate`, bit 15 marks the
/// fast path.
pub const RATE_FAST: u32 = 0x10d0;
/// Written zero by both paths of `SetSampleRate` (`0x10d4`).
pub const RATE_MID: u32 = 0x10d4;
/// Slow-path rate divider (`0x10d8`): `250 MSa/s / rate`, 1 on the fast path.
pub const RATE_SLOW: u32 = 0x10d8;
/// Memory size (`0x10e0`); the from-scratch init writes `0x0040_0000` for a 10 Mpts record.
pub const MEM_SIZE: u32 = 0x10e0;
/// Record depth in points (`0x10e4`).
pub const RECORD_DEPTH: u32 = 0x10e4;
/// Anti-alias filter (`0x10dc`).
pub const ANTI_ALIAS: u32 = 0x10dc;
/// Pre-sample compensation (`0x404c`): 5, plus 10 in average mode.
pub const PRE_SA_COMP: u32 = 0x404c;
/// Sample range (`0x4050`/`0x4054`): the record window the SPU fills.
pub const SAMPLE_RANGE: [u32; 2] = [0x4050, 0x4054];
/// Memory status (`0x1008`): bit 0 idle, bit 25 busy.
pub const MEM_STATUS: u32 = 0x1008;
/// Record-sum status (`0x1208`): bit 31 valid, bits 0-29 the accumulated frame count.
pub const RECORD_SUM: u32 = 0x1208;

/// `DevAcquireSPU_SetSampleRate` @0x2a56a8: below 250 MSa/s the slow divider
/// `250 MSa/s / rate` does the work; at or above it the fast divider carries the low byte of
/// `max_rate / rate` with bit 15 set and the slow divider parks at 1. `max_rate` is the unit's
/// interleaved ceiling (2 GSa/s on this board).
pub fn set_sample_rate(rate: u32, max_rate: u32) -> [Write; 3] {
    let (fast, slow) = if u64::from(rate) < 0xee6b281 {
        (0, if rate == 0 { 0 } else { 250_000_000 / rate })
    } else {
        (
            if rate == 0 {
                0x8000
            } else {
                (max_rate / rate) & 0xff | 0x8000
            },
            1,
        )
    };
    [(RATE_FAST, fast), (RATE_SLOW, slow), (RATE_MID, 0)]
}

/// The acquire modes of `DevAcquireSpu_SetAcqMode` @0x2a5558.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcqMode {
    Normal,
    Average,
    HighRes,
}

impl AcqMode {
    /// The stock code stores the mode in bits 1 and 3 of `ACQ`.
    pub fn word(self, sh: u32) -> u32 {
        let mode = match self {
            AcqMode::Normal => 0,
            AcqMode::Average => 2,
            AcqMode::HighRes => 8,
        };
        (sh & 0xffff_eff5) | mode
    }
}

/// High-resolution mode engages automatically below one eighth of the maximum rate
/// (`ConfigAcquire` in the stock library: `SetAvgSincMode(rate < maxRate / 8)`).
pub fn hires_engaged(rate: u32, max_rate: u32) -> bool {
    rate < max_rate / 8
}

/// `DevAcquireSpu_SetRecordDepth` @0x2a599c.
pub fn set_record_depth(depth: u32) -> Write {
    (RECORD_DEPTH, depth)
}

/// `DevAcquireSpu_SetMemSize` @0x2a57fc.
pub fn set_mem_size(size: u32) -> Write {
    (MEM_SIZE, size)
}

/// `DevAcquireSPU_SetAcqAntiAlias` @0x2a4f34 (16-bit value).
pub fn set_anti_alias(value: u16) -> Write {
    (ANTI_ALIAS, u32::from(value))
}

/// `DevAcquireSPU_SetPreSaComp` @0x2a569c: 5, or 15 in average mode.
pub fn set_pre_sa_comp(average_mode: bool) -> Write {
    (PRE_SA_COMP, u32::from(average_mode) * 10 + 5)
}

/// `DevAcquireSPU_SetSampleRange` @0x2a5670: the record window. The sidecar's from-scratch
/// configure writes `depth - 1` and 1 for a plain record.
pub fn sample_range(end: u32, tail: u32) -> [Write; 2] {
    [(SAMPLE_RANGE[0], end), (SAMPLE_RANGE[1], tail)]
}

/// `DevAcquireSPU_GetWaveRange` @0x2a5b94: the valid flag and the accumulated frame count from
/// the record-sum register.
pub fn wave_range(reg: u32) -> (bool, u32) {
    (reg >> 31 == 1, reg & 0x3fff_ffff)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slow_path_divides_two_hundred_fifty_megahertz() {
        // the live reading: 10 MSa/s -> 0x10d8 = 25
        let w = set_sample_rate(10_000_000, 2_000_000_000);
        assert_eq!(w[0], (RATE_FAST, 0));
        assert_eq!(w[1], (RATE_SLOW, 25));
        assert_eq!(w[2], (RATE_MID, 0));
        // roll-rate reading: 1 MSa/s -> 250
        assert_eq!(set_sample_rate(1_000_000, 2_000_000_000)[1], (RATE_SLOW, 250));
        assert_eq!(set_sample_rate(0, 2_000_000_000)[1], (RATE_SLOW, 0), "no divide by zero");
    }

    #[test]
    fn fast_path_marks_the_low_byte() {
        // the live reading: 500 MSa/s -> 0x10d0 = 0x8004
        let w = set_sample_rate(500_000_000, 2_000_000_000);
        assert_eq!(w[0], (RATE_FAST, 0x8004));
        assert_eq!(w[1], (RATE_SLOW, 1));
        // 2 GSa/s full rate -> divider 1
        assert_eq!(set_sample_rate(2_000_000_000, 2_000_000_000)[0], (RATE_FAST, 0x8001));
    }

    #[test]
    fn acquire_modes_use_their_bits() {
        assert_eq!(AcqMode::Normal.word(0x1_100a), 0x1_0000, "bits 1, 3 and 12 cleared");
        assert_eq!(AcqMode::Average.word(0xffff_ffff), 0xffff_eff7, "bit 1 set, the clear mask kept");
        assert_eq!(AcqMode::HighRes.word(0), 8, "bit 3 set");
    }

    #[test]
    fn hires_engages_below_one_eighth_of_the_ceiling() {
        assert!(hires_engaged(249_999_999, 2_000_000_000));
        assert!(!hires_engaged(250_000_000, 2_000_000_000));
        assert!(hires_engaged(10_000_000, 2_000_000_000), "the measured 10 MSa/s runs hires");
    }

    #[test]
    fn depth_memsize_antialias_and_compensation_write_their_registers() {
        assert_eq!(set_record_depth(25_000_000), (RECORD_DEPTH, 25_000_000));
        assert_eq!(set_mem_size(0x0040_0000), (MEM_SIZE, 0x0040_0000));
        assert_eq!(set_anti_alias(1), (ANTI_ALIAS, 1));
        assert_eq!(set_pre_sa_comp(false), (PRE_SA_COMP, 5));
        assert_eq!(set_pre_sa_comp(true), (PRE_SA_COMP, 15));
        assert_eq!(sample_range(999_999, 1)[0], (SAMPLE_RANGE[0], 999_999));
    }

    #[test]
    fn wave_range_splits_flag_and_count() {
        assert_eq!(wave_range(1 << 31 | 240), (true, 240));
        assert_eq!(wave_range(19), (false, 19));
    }
}
