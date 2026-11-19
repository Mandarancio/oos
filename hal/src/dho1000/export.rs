// Copyright (C) 2026 Julian Decker
// SPDX-License-Identifier: GPL-2.0-or-later

//! DHO1000 SPU memory export: the app-free raw read-out of a record window.
//!
//! Evidence: **measured on a DHO1204, app-free** — this is the sequence the sidecar library
//! (`rigol_sidecar` `rs_export`) validated with the stock software dead: freeze, the raw-playback
//! chain, the window registers, the record-play handshake with two constant SCU bundles, then one
//! blocking stream read per page. Pages of 1 M frames per channel read out at 12-20 MB/s over
//! localhost SCPI on the same handshake; the batch loop of `acq` manages ~1 MB/s, so this is the
//! path a port should drive for deep memory. Reconstructed as write lists; the bus applies them.
//!
//! The stream delivers little-endian 16-bit samples, channel-interleaved in ascending channel
//! order; `read_bytes` gives the exact read size for one page.

use crate::dho1000::acq::{CTRL, MODE, PLAY, SPU};
use crate::dho1000::Write;

/// The first SCU bundle of the handshake (`rs_export`): seven constant words after the
/// record-play arm.
pub const SCU_BUNDLE_1: [Write; 7] = [
    (0x4024, 0x0008_1001),
    (0x4028, 0x0080_1601),
    (0x402c, 0x0008_0605),
    (0x4030, 0x04c4_a907),
    (0x4034, 0x0000_0007),
    (0x4038, 0x020a_0601),
    (0x403c, 0x020a_8601),
];
/// The second SCU bundle, written after the control pulse.
pub const SCU_BUNDLE_2: [Write; 7] = [
    (0x4024, 0x8008_1001),
    (0x4028, 0x0080_1701),
    (0x402c, 0x0008_0705),
    (0x4030, 0xcf04_a90e),
    (0x4034, 0x0000_000e),
    (0x4038, 0x228c_0701),
    (0x403c, 0x028c_8701),
];

/// Shadow words the export read-modify-writes; fill them by reading the registers first.
#[derive(Clone, Copy, Debug, Default)]
pub struct Shadow {
    pub s4004: u32,
    pub s4008: u32,
    pub s1170: u32,
    pub s1174: u32,
    pub s1154: u32,
    pub s1004: u32,
    pub s400c: u32,
}

/// Window registers (`SetWaveRange`/`SetTxInfo`): `start` is the first frame, `frames` the page
/// length in frames per channel. `0x1100` carries the stock `+0x200` tail padding and `0x11e8`
/// the TX size in 8-byte units.
pub const WINDOW_START: u32 = 0x1108;
pub const WINDOW_LEN: u32 = 0x1100;
pub const TX_LEN: u32 = 0x11f0;
pub const TX_WORDS: u32 = 0x11e8;
/// Sinc configuration the raw path parks at (`SetSinc` pass-through).
pub const SINC: [u32; 2] = [0x1130, 0x1134];
/// Interpolation phase words, main and zoom (`SetLinearInterp` off).
pub const INTERP: [u32; 4] = [0x1158, 0x115c, 0x1160, 0x1164];
/// The compressors, main and zoom (`display::compress` words).
pub const COMPRESS: [u32; 2] = [0x1170, 0x1174];
/// Interpolator enables (`display::INTERP_EN`).
pub const INTERP_EN: u32 = 0x1154;

/// Freeze the acquisition and park the display chain in raw-playback mode: zoom off, both
/// compressors pass-through, sinc parked, both interpolators off with a full-scale phase, then
/// the two decisive writes — bit 25 of the SPU mode off and `0x1000 = 0x0400_0000`.
pub fn init(sh: &mut Shadow) -> Vec<Write> {
    let mut w = Vec::with_capacity(17);
    // freeze: the SCU auto-trigger bit and the run bit off
    w.push((crate::dho1000::zone::SCU_CTRL, 0));
    w.push((CTRL, sh.s4004 & !1));
    sh.s4008 &= !0x0200_0000;
    w.push((MODE, sh.s4008));
    sh.s1170 = (sh.s1170 & 0x3000_0000) | 0x4000_0001;
    w.push((COMPRESS[0], sh.s1170));
    sh.s1174 = (sh.s1174 & 0x3000_0000) | 0x4000_0001;
    w.push((COMPRESS[1], sh.s1174));
    w.push((SINC[0], 0x0001_0101));
    w.push((SINC[1], 0x0001_0101));
    sh.s1154 = (sh.s1154 & 0xfffe) | 1;
    w.push((INTERP_EN, sh.s1154));
    w.push((INTERP[0], 0));
    w.push((INTERP[1], 0xffff_ffff));
    sh.s1154 = (sh.s1154 & 0xfffd) | 2;
    w.push((INTERP_EN, sh.s1154));
    w.push((INTERP[2], 0));
    w.push((INTERP[3], 0xffff_ffff));
    sh.s1004 &= !0x0200_0000;
    w.push((0x1004, sh.s1004));
    w.push((SPU, 0x0400_0000));
    w
}

/// The read-out window for one page: `frames` per channel from `start`.
pub fn window(start: u32, frames: u32) -> [Write; 4] {
    [
        (WINDOW_START, start),
        (WINDOW_LEN, frames + 0x200),
        (TX_LEN, frames),
        (TX_WORDS, frames / 8),
    ]
}

/// The record-play handshake (`setRecordPlay` positions 0, then the arm pulse, the two SCU
/// bundles and the one-batch arm bit 19). Returns the writes and updates the shadows.
pub fn arm(sh: &mut Shadow) -> Vec<Write> {
    let mut w = Vec::with_capacity(20);
    sh.s400c = (sh.s400c & 0x89ff_ffff) | 0x3000_0000;
    w.push((PLAY, sh.s400c));
    w.push((0x4010, 0));
    w.push((0x4014, 0));
    w.push((0x4018, 0));
    sh.s400c &= !0x1000_0000;
    w.push((PLAY, sh.s400c));
    sh.s400c = (1 << 29) | (sh.s400c & 0x9bff_ffff) | 0x4000_0000;
    w.push((PLAY, sh.s400c));
    sh.s4004 = 0x0008_0a00;
    w.push((CTRL, sh.s4004));
    sh.s4004 = 0x0000_0809;
    w.push((CTRL, sh.s4004));
    w.extend_from_slice(&SCU_BUNDLE_1);
    sh.s4004 = 0x0010_0809;
    w.push((CTRL, sh.s4004));
    w.extend_from_slice(&SCU_BUNDLE_2);
    w.push((CTRL, sh.s4004 | (1 << 19)));
    sh.s4004 |= 1 << 19;
    w
}

/// Re-arm for the next page read: the stock loop sets bit 19 before every read.
pub fn re_arm(sh: &Shadow) -> Write {
    (CTRL, sh.s4004 | (1 << 19))
}

/// The exact read size of one page: 2 bytes per sample, all channels interleaved.
pub fn read_bytes(frames: u32, channels: u32) -> usize {
    frames as usize * channels as usize * 2
}

/// Restore the pre-export register state (the stock code saves and puts back the control and
/// SCU bundle registers).
pub fn restore(s4004: u32, scu: &[Write; 7]) -> Vec<Write> {
    let mut w = Vec::with_capacity(8);
    w.push((CTRL, s4004));
    w.extend_from_slice(scu);
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_freezes_and_parks_the_display_chain() {
        let mut sh = Shadow {
            s4004: 1,
            s4008: 0x0200_0000,
            s1170: 0x3000_0000,
            s1174: 0,
            s1154: 0,
            s1004: 0x0200_0000,
            s400c: 0,
        };
        let w = init(&mut sh);
        assert!(w.contains(&(CTRL, 0)), "run bit cleared");
        assert!(w.contains(&(MODE, 0)), "zoom bit cleared");
        assert!(w.contains(&(COMPRESS[0], 0x7000_0001)), "shadow top bits kept");
        assert!(w.contains(&(SINC[0], 0x0001_0101)));
        assert!(w.contains(&(INTERP[1], 0xffff_ffff)));
        assert!(w.contains(&(0x1004, 0)), "memory-playback bit cleared");
        assert_eq!(*w.last().unwrap(), (SPU, 0x0400_0000), "the decisive last write");
        assert_eq!(sh.s1154, 3, "both interpolator disable bits");
    }

    #[test]
    fn window_pads_the_length_and_scales_the_tx_words() {
        let w = window(1_000_000, 1_000_000);
        assert_eq!(w[0], (WINDOW_START, 1_000_000));
        assert_eq!(w[1], (WINDOW_LEN, 1_000_000 + 0x200), "the stock tail padding");
        assert_eq!(w[2], (TX_LEN, 1_000_000));
        assert_eq!(w[3], (TX_WORDS, 125_000), "1M frames = 125k 8-byte units");
    }

    #[test]
    fn arm_composes_the_handshake() {
        let mut sh = Shadow::default();
        let w = arm(&mut sh);
        assert_eq!(w[0], (PLAY, 0x3000_0000));
        assert!(w.contains(&(0x4010, 0)));
        assert!(w.contains(&(PLAY, 0x6000_0000)), "dir bit 29 + commit on the armed word");
        assert!(w.contains(&(CTRL, 0x0008_0a00)));
        assert!(w.contains(&SCU_BUNDLE_1[3]));
        assert!(w.contains(&SCU_BUNDLE_2[6]));
        assert_eq!(*w.last().unwrap(), (CTRL, 0x0010_0809 | 1 << 19));
        assert_eq!(re_arm(&sh), (CTRL, 0x0010_0809 | 1 << 19));
    }

    #[test]
    fn one_page_is_two_bytes_per_sample_per_channel() {
        assert_eq!(read_bytes(1_000_000, 1), 2_000_000);
        assert_eq!(read_bytes(250_000, 4), 2_000_000, "4-ch pages are a quarter");
    }

    #[test]
    fn restore_puts_the_saved_words_back() {
        let w = restore(0x1234, &SCU_BUNDLE_1);
        assert_eq!(w[0], (CTRL, 0x1234));
        assert_eq!(w.len(), 8);
    }
}
