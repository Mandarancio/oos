// Copyright (C) 2026 Julian Decker
// SPDX-License-Identifier: GPL-2.0-or-later

//! DHO1000 hardware averaging, reconstructed from the stock library.
//!
//! Evidence: `DevAcquireSpu_setAvgPara` @0x2a55bc, `DevAcquireSpu_SetAvgSincMode` @0x2a559c,
//! `DevAcquireSpu_setAvgClr` (decompiled in `docs/decomp/libscope`). Not yet verified on hardware.

use crate::dho1000::acq::Shadow;

/// SPU acquisition mode register (sinc averaging bit is bit 2).
pub const ACQ_MODE: u32 = 0x1004;
/// Averaging count - 1 (the stock code writes at least 1).
pub const AVG_COUNT: u32 = 0x10b0;
/// Averaging clear.
pub const AVG_CLEAR: u32 = 0x10b4;
/// Burst length, `0x20 + 0x280 / channels`.
pub const AVG_BURST: u32 = 0x1148;

/// Hardware average of `count` frames over `channels` channels on: the count register and the
/// per-channel burst length.
pub fn set_avg_para(sh: &mut Shadow, count: u32, channels: usize) -> Vec<(u32, u32)> {
    let n = if count <= 1 { 1 } else { count - 1 };
    let burst = 0x20
        + if channels > 0 {
            0x280 / channels as u32
        } else {
            0
        };
    let _ = sh;
    vec![(AVG_COUNT, n), (AVG_BURST, burst)]
}

/// Sinc averaging on/off: bit 2 of `0x1004`, the other bits kept.
pub fn set_avg_sinc(sh: &mut Shadow, on: bool) -> Vec<(u32, u32)> {
    sh.s1004 = (sh.s1004 & !(1 << 2)) | (u32::from(on) << 2);
    vec![(ACQ_MODE, sh.s1004)]
}

/// Drop the accumulated average.
pub fn avg_clear() -> Vec<(u32, u32)> {
    vec![(AVG_CLEAR, 1)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn averages_count_and_burst_scale_with_channels() {
        let mut sh = Shadow::default();
        assert_eq!(
            set_avg_para(&mut sh, 16, 1),
            vec![(AVG_COUNT, 15), (AVG_BURST, 0x2a0)]
        );
        assert_eq!(
            set_avg_para(&mut sh, 2, 4),
            vec![(AVG_COUNT, 1), (AVG_BURST, 0x20 + 0xa0)]
        );
        assert_eq!(
            set_avg_para(&mut sh, 1, 1)[0],
            (AVG_COUNT, 1),
            "at least one"
        );
    }

    #[test]
    fn sinc_bit_keeps_the_rest_of_the_mode() {
        let mut sh = Shadow {
            s1004: 0x0400_0300,
            ..Default::default()
        };
        assert_eq!(set_avg_sinc(&mut sh, true), vec![(ACQ_MODE, 0x0400_0304)]);
        assert_eq!(set_avg_sinc(&mut sh, false), vec![(ACQ_MODE, 0x0400_0300)]);
    }
}
