// Copyright (C) 2026 Julian Decker
// SPDX-License-Identifier: GPL-2.0-or-later

//! DHO1000 sample clock: an LMK04821 jitter cleaner, identified on the unit.
//!
//! Evidence: the boot chain on the unit runs `spi2pll` from `resource/conf/
//! init_all_clock.txt` of the stock firmware (recovered verbatim below; R0-R13 are the
//! global registers, R256 up are registers 0x0100 onward, R8189-R8191 the calibration
//! tail with the VCO calibrate word 0x53). The stock library reaches the chip through
//! `Dev_LmkWrite` ioctls: `DevBoardClock_SetAFEChanFrequency` @0x2ae56c divides the 2 GHz
//! VCO per AFE channel (`2e9 / freq` into register 0x130 or 0x100, LUT at file 0x00a0ada8),
//! `DevBoardClock_AFEChanOutput` @0x2ae4e0 turns an output on and off (0xf8/0xf0 into
//! 0x136 or 0x106, LUT at file 0x00a0ad78). The chip itself sits behind the `spi2pll`
//! kernel module, so driving it from OOS needs that interface, not FPGA register writes.
//! The table is stock data, not measured here; the identity of the chip is verified on
//! the unit (boot log).

/// The stock initialization table: (register, value) in write order.
/// The stock initialization table: (register, value) in write order. The file carries
/// 24-bit SPI words, `register << 8 | value`; the register numbers are the R-numbers of the
/// LMK04821 manual.
pub const INIT_TABLE: [(u16, u8); 129] = [
    (0x0000, 0x90), (0x0000, 0x10), (0x0002, 0x00), (0x0003, 0x06), (0x0004, 0xd0), (0x0005, 0x5b),
    (0x0006, 0x00), (0x000c, 0x51), (0x000d, 0x04), (0x0100, 0x19), (0x0101, 0x55), (0x0102, 0x55),
    (0x0103, 0x45), (0x0104, 0x00), (0x0105, 0x00), (0x0106, 0xf8), (0x0107, 0x11), (0x0108, 0x10),
    (0x0109, 0x55), (0x010a, 0x55), (0x010b, 0x41), (0x010c, 0x02), (0x010d, 0x00), (0x010e, 0xf0),
    (0x010f, 0x01), (0x0110, 0x0a), (0x0111, 0x55), (0x0112, 0x55), (0x0113, 0x41), (0x0114, 0x02),
    (0x0115, 0x00), (0x0116, 0xf0), (0x0117, 0x01), (0x0118, 0x01), (0x0119, 0x55), (0x011a, 0x55),
    (0x011b, 0x41), (0x011c, 0x20), (0x011d, 0x00), (0x011e, 0xf0), (0x011f, 0x16), (0x0120, 0x08),
    (0x0121, 0x55), (0x0122, 0x55), (0x0123, 0x00), (0x0124, 0x02), (0x0125, 0x00), (0x0126, 0xf8),
    (0x0127, 0x00), (0x0128, 0x01), (0x0129, 0x55), (0x012a, 0x55), (0x012b, 0x41), (0x012c, 0x20),
    (0x012d, 0x00), (0x012e, 0xf0), (0x012f, 0x16), (0x0130, 0x19), (0x0131, 0x55), (0x0132, 0x55),
    (0x0133, 0x41), (0x0134, 0x00), (0x0135, 0x00), (0x0136, 0xf8), (0x0137, 0x11), (0x0138, 0x05),
    (0x0139, 0x00), (0x013a, 0x00), (0x013b, 0x08), (0x013c, 0x00), (0x013d, 0x08), (0x013e, 0x03),
    (0x013f, 0x15), (0x0140, 0x8a), (0x0141, 0x00), (0x0142, 0x00), (0x0143, 0x10), (0x0144, 0x7f),
    (0x0145, 0x7f), (0x0146, 0x08), (0x0147, 0x18), (0x0148, 0x02), (0x0149, 0x42), (0x014a, 0x02),
    (0x014b, 0x16), (0x014c, 0x00), (0x014d, 0x00), (0x014e, 0xc0), (0x014f, 0x7f), (0x0150, 0x03),
    (0x0151, 0x02), (0x0152, 0x00), (0x0153, 0x00), (0x0154, 0x78), (0x0155, 0x00), (0x0156, 0x78),
    (0x0157, 0x00), (0x0158, 0x96), (0x0159, 0x00), (0x015a, 0x78), (0x015b, 0xd4), (0x015c, 0x20),
    (0x015d, 0x00), (0x015e, 0x00), (0x015f, 0x0b), (0x0160, 0x00), (0x0161, 0x02), (0x0162, 0x01),
    (0x0163, 0x00), (0x0164, 0x00), (0x0165, 0x19), (0x0171, 0xaa), (0x0172, 0x02), (0x0173, 0x00),
    (0x0174, 0x05), (0x017c, 0x15), (0x017d, 0x33), (0x0166, 0x00), (0x0167, 0x00), (0x0168, 0x19),
    (0x0169, 0x59), (0x016a, 0x20), (0x016b, 0x00), (0x016c, 0x00), (0x016d, 0xd8), (0x016e, 0x13),
    (0x1ffd, 0x00), (0x1ffe, 0x00), (0x1fff, 0x53),
];

/// The AFE clock divide registers per channel (`DevBoardClock_SetAFEChanFrequency`, LUT at
/// file 0x00a0ada8).
pub const AFE_DIV_REG: [u16; 2] = [0x130, 0x100];

/// The AFE clock output-control registers per channel and their on/off values
/// (`DevBoardClock_AFEChanOutput`, LUT at file 0x00a0ad78).
pub const AFE_OUT_REG: [u16; 2] = [0x136, 0x106];
pub const AFE_OUT_ON: u8 = 0xf8;
pub const AFE_OUT_OFF: u8 = 0xf0;

/// The VCO the dividers split: 2 GHz (`SetAFEChanFrequency` divides `2e9 / freq`).
pub const VCO_HZ: f64 = 2e9;

/// One divider write per AFE channel: `Dev_LmkWrite(AFE_DIV_REG[ch], 2e9 / freq_hz)`.
pub fn afe_chan_frequency(ch: usize, freq_hz: f64) -> Option<(u16, u8)> {
    if ch > 1 || freq_hz == 0.0 {
        return None;
    }
    Some((AFE_DIV_REG[ch], (VCO_HZ / freq_hz) as u8))
}

/// One output-enable write per AFE channel: `Dev_LmkWrite(AFE_OUT_REG[ch], 0xf8 or 0xf0)`.
pub fn afe_chan_output(ch: usize, on: bool) -> Option<(u16, u8)> {
    if ch > 1 {
        return None;
    }
    Some((AFE_OUT_REG[ch], if on { AFE_OUT_ON } else { AFE_OUT_OFF }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_the_stock_file_in_order() {
        assert_eq!(INIT_TABLE.len(), 129);
        // the write-and-wake header, then the global registers
        assert_eq!(INIT_TABLE[0], (0x0000, 0x90), "R0 (INIT)");
        assert_eq!(INIT_TABLE[1], (0x0000, 0x10));
        assert_eq!(INIT_TABLE[2], (0x0002, 0x00));
        // the register-0x0100 block follows (nine global rows come first)
        assert_eq!(INIT_TABLE[9], (0x0100, 0x19));
        // the calibration tail
        assert_eq!(INIT_TABLE[126], (0x1ffd, 0x00));
        assert_eq!(INIT_TABLE[127], (0x1ffe, 0x00));
        assert_eq!(INIT_TABLE[128], (0x1fff, 0x53), "the VCO calibrate word");
        // R256-R357 ascend, then six out-of-order rows, then R358-R366 again
        assert!(INIT_TABLE[9..111].windows(2).all(|w| w[0].0 < w[1].0));
        assert!(INIT_TABLE[117..126].windows(2).all(|w| w[0].0 < w[1].0));
        assert_eq!(INIT_TABLE[111].0, 369, "the first out-of-order row");
    }

    #[test]
    fn afe_channel_writes_pick_their_registers() {
        assert_eq!(afe_chan_frequency(0, 1e9), Some((0x130, 2)));
        assert_eq!(afe_chan_frequency(1, 1e9), Some((0x100, 2)));
        assert_eq!(afe_chan_frequency(0, 2e9), Some((0x130, 1)));
        assert_eq!(afe_chan_frequency(2, 1e9), None, "two AFE clock channels");
        assert_eq!(afe_chan_frequency(0, 0.0), None, "no divide by zero");
        assert_eq!(afe_chan_output(0, true), Some((0x136, 0xf8)));
        assert_eq!(afe_chan_output(1, false), Some((0x106, 0xf0)));
    }
}
