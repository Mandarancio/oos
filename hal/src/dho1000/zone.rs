// Copyright (C) 2026 Julian Decker
// SPDX-License-Identifier: GPL-2.0-or-later

//! DHO1000 zone trigger and mask output, reconstructed from the stock library.
//!
//! Evidence: `DevAcquireSPU_SetZoneTrigCtrl` @0x2b9fd8, `DevAcquireSPU_SetMaskOutput` @0x2b9d9c,
//! `DevSystemSCU_SetMaskZoneTrig` @0x2ba080 (decompiled in `docs/decomp/libscope`). The zone
//! compare tables themselves are built by `DrvZoneTrigger_make{Single,Double}Tab`, which are not
//! decoded; these are the enable/route bits around them. Not verified on hardware.

/// Zone trigger: bits 24-27 of the SPU zone word (`0x11d0`), one bit per function.
pub const ZONE_CTRL: u32 = 0x11d0;
/// Mask output: `(index * 4 + 4)` in bits 2-23, enables in bits 29-31 (`0x119c`).
pub const MASK_OUTPUT: u32 = 0x119c;
/// SCU function control (`0x4020`): mask/zone trigger enables in bits 12-15, bit 1.
pub const SCU_CTRL: u32 = 0x4020;

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
}
