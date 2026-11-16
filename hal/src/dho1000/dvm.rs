// Copyright (C) 2026 Julian Decker
// SPDX-License-Identifier: GPL-2.0-or-later

//! DHO1000 hardware DVM, reconstructed from the stock library.
//!
//! Evidence: `DevAnalyzeDvm_SetDvm` @0x2b93b4 and `DevAnalyzeDvm_GetDvm` @0x2b9424 (decompiled in
//! `docs/decomp/libscope`). Not yet verified on hardware.
//!
//! The DVM accumulates in the FPGA: the configuration goes to 0x2244, a read latches 0x2248 and
//! the 48-bit sum is read from 0x224c (low) and 0x2250 (high, bits 16-31), then divided by
//! `1 << shift` for the average.

/// DVM configuration: bit 0 enable, bits 1-5 mode, bits 8-9 channel.
pub const DVM_CFG: u32 = 0x2244;
/// DVM read latch (bit 0 set, then cleared, to freeze the sum).
pub const DVM_LATCH: u32 = 0x2248;
/// DVM sum, low 32 bits.
pub const DVM_SUM_L: u32 = 0x224c;
/// DVM sum, high bits (16-31 of this word extend the sum).
pub const DVM_SUM_H: u32 = 0x2250;

/// `DevAnalyzeDvm_SetDvm(enable, mode, channel)`: bit 0 enable, bits 1-5 mode, bits 8-9 channel;
/// bits 6-7 and 10-15 are kept.
pub fn set_dvm(sh: u32, enable: bool, mode: u8, channel: usize) -> (u32, u32) {
    let mut v = (sh & 0xfcc0) | (u32::from(mode) & 0x1f) << 1 | (channel as u32 & 3) << 8;
    if enable {
        v |= 1;
    }
    (DVM_CFG, v)
}

/// Latch and decode one DVM reading: the sum is `(sum_h >> 16) << 32 | sum_l`, the average is that
/// over `1 << shift`.
pub fn dvm_average(sum_l: u32, sum_h: u32, shift: u32) -> f64 {
    let sum = (u64::from(sum_h >> 16) << 32) | u64::from(sum_l);
    sum as f64 / (1u64 << (shift & 0x1f)) as f64
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
}
