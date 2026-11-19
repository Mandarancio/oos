// Copyright (C) 2026 Julian Decker
// SPDX-License-Identifier: GPL-2.0-or-later

//! DHO1000 series: front-end range table as measured on a DHO1204 (docs/protocols/dho1000.md).
//!
//! Unlike the DHO800/DHO900 front end (`crate::dho::afe`, calibrated from `/rigol/data`), the table
//! below was recovered from the stock software and applied to the hardware: writing the 13 register
//! values of a range, then `OFF_DAC_REG` = `base_dac + lsb_per_v * volts`, sets V/div and offset.
//! The absolute gain was verified app-free on the unit (sidecar notes): a +-1 V signal reads
//! +-7480 counts at 1 V/div, 0.913 of the nominal 8192 counts/V. The measured DAC step is about
//! 8.77 ADC counts per LSB, consistent with the table's `lsb_per_v` within calibration spread.
//!
//! Transport and neighbours, from the same notes: the AFE speaks over `/dev/spi_simulate`
//! (`ioctl(fd, 0x7800, ch)` selects the 0-based channel, then a 3-byte write `{reg, hi, lo}` or a
//! read with `reg | 0x80` returning `{hi, lo}`); the ADC chips are programmed over the FPGA BAR
//! register `0x1034` (`adc_word`); the 20 MHz band limit additionally pokes ADC registers
//! `0x54 = 0x4210` and `0x55 = 8` on both chips and writes the AFE filter register `0x2e`
//! (`band_limit_word`). The RT1642 has a second register bank beyond this map (0x20-0x37):
//! reg 0x03 input gain, 0x05 power, 0x07 bit 15 AC coupling and bits 13-14 the path select,
//! 0x08 DAC value, 0x09-0x0c the 1 MOhm chain, 0x0e filters/mux, 0x27/0x28 DAC enable and value,
//! 0x29-0x2e the 50 ohm-path mirrors (unused here: no 50 ohm path on this series).

/// AFE registers written for one range, in this order (same map as the DHO800/DHO900 front end).
pub const REG_MAP: [u8; 13] = [
    0x00, 0x01, 0x03, 0x04, 0x05, 0x06, 0x08, 0x09, 0x0a, 0x0c, 0x0e, 0x11, 0x12,
];

/// Offset DAC register (base code at 0 V offset, plus `lsb_per_v` per volt).
pub const OFF_DAC_REG: u8 = 0x28;

/// One entry: V/div, the 13 AFE registers, the DAC code at 0 V, and the DAC LSB per volt.
pub struct Range {
    pub vdiv: f64,
    pub regs: [u16; 13],
    pub base_dac: u16,
    pub lsb_per_v: i32,
}

/// The measured range table (0.5 mV/div .. 10 V/div).
pub static RANGES: [Range; 14] = [
    Range {
        vdiv: 0.0005,
        regs: [
            0xa038, 0x03f3, 0x2813, 0x0235, 0x01db, 0x3474, 0x7fbd, 0x7842, 0x0c44, 0xcc0c, 0x1bdb,
            0x9757, 0x95aa,
        ],
        base_dac: 0x7fbd,
        lsb_per_v: -16451,
    },
    Range {
        vdiv: 0.001,
        regs: [
            0xa038, 0x03f3, 0x2813, 0x0235, 0x01db, 0x3474, 0x7fbd, 0x7882, 0x0c44, 0xcc0c, 0x9bdb,
            0x9757, 0x95aa,
        ],
        base_dac: 0x7fbd,
        lsb_per_v: -16451,
    },
    Range {
        vdiv: 0.002,
        regs: [
            0xa038, 0x03f3, 0x2813, 0x0235, 0x01db, 0x3474, 0x7fbd, 0x7842, 0x0c44, 0xcc0c, 0x9bdb,
            0x9757, 0x95aa,
        ],
        base_dac: 0x7fbd,
        lsb_per_v: -16451,
    },
    Range {
        vdiv: 0.005,
        regs: [
            0xa038, 0x03f3, 0x2812, 0x0235, 0x01db, 0x3474, 0x7fca, 0x7982, 0x0c44, 0xcc0c, 0x1bdb,
            0x9757, 0x95aa,
        ],
        base_dac: 0x7fca,
        lsb_per_v: -16451,
    },
    Range {
        vdiv: 0.01,
        regs: [
            0xa038, 0x03f3, 0x2812, 0x0235, 0x01db, 0x3474, 0x7fca, 0x7982, 0x0c44, 0xcc0c, 0x9bdb,
            0x9757, 0x95aa,
        ],
        base_dac: 0x7fca,
        lsb_per_v: -16451,
    },
    Range {
        vdiv: 0.02,
        regs: [
            0xa038, 0x03f3, 0x2812, 0x0235, 0x01db, 0x3474, 0x7fd1, 0x7942, 0x0c44, 0xcc0c, 0x9bdb,
            0x9757, 0x95aa,
        ],
        base_dac: 0x7fd1,
        lsb_per_v: -16451,
    },
    Range {
        vdiv: 0.05,
        regs: [
            0xa038, 0x03f3, 0x2812, 0x0235, 0x01db, 0x3474, 0x7fe3, 0x7942, 0x1c44, 0xcc0c, 0x1bdb,
            0x9757, 0x95aa,
        ],
        base_dac: 0x7fe3,
        lsb_per_v: -16451,
    },
    Range {
        vdiv: 0.1,
        regs: [
            0xa038, 0x03f3, 0x1012, 0x0235, 0x01db, 0x3474, 0x7fd5, 0x7a82, 0x3c44, 0xcc0c, 0x1bdb,
            0x9757, 0x95aa,
        ],
        base_dac: 0x7fd5,
        lsb_per_v: 1305,
    },
    Range {
        vdiv: 0.2,
        regs: [
            0xa038, 0x03f3, 0x1012, 0x0235, 0x01db, 0x3474, 0x7fd6, 0x7a42, 0x3c44, 0xcc0c, 0x1bdb,
            0x9757, 0x95aa,
        ],
        base_dac: 0x7fd6,
        lsb_per_v: 1305,
    },
    Range {
        vdiv: 0.5,
        regs: [
            0xa03c, 0x03f3, 0x1012, 0x0235, 0x01db, 0x3474, 0x7fd4, 0x7a82, 0x3c44, 0xcc0c, 0x1bdb,
            0x9757, 0x95aa,
        ],
        base_dac: 0x7fd4,
        lsb_per_v: 853,
    },
    Range {
        vdiv: 1.0,
        regs: [
            0xa03c, 0x03f3, 0x1012, 0x0235, 0x01db, 0x3474, 0x7fd4, 0x7a42, 0x3c44, 0xcc0c, 0x1bdb,
            0x9757, 0x95aa,
        ],
        base_dac: 0x7fd4,
        lsb_per_v: 853,
    },
    Range {
        vdiv: 2.0,
        regs: [
            0xa03c, 0x03f3, 0x1012, 0x0235, 0x01db, 0x3474, 0x7fcc, 0x7a42, 0xbc44, 0xcc0c, 0x5bdb,
            0x9757, 0x95aa,
        ],
        base_dac: 0x7fcc,
        lsb_per_v: 853,
    },
    Range {
        vdiv: 5.0,
        regs: [
            0xa03e, 0x03f3, 0x1012, 0x0235, 0x01db, 0x3474, 0x7fcc, 0x7a42, 0x2c44, 0xcc0c, 0x13db,
            0x9757, 0x95aa,
        ],
        base_dac: 0x7fcc,
        lsb_per_v: 102,
    },
    Range {
        vdiv: 10.0,
        regs: [
            0xa03e, 0x03f3, 0x1012, 0x0235, 0x01db, 0x3474, 0x7fd2, 0x7a42, 0x3c44, 0xcc0c, 0x5bdb,
            0x9757, 0x95aa,
        ],
        base_dac: 0x7fd2,
        lsb_per_v: 102,
    },
];

/// Nearest range for a requested V/div (ratio distance, as the stock software picks it).
pub fn range_for(vdiv: f64) -> &'static Range {
    let mut best = &RANGES[0];
    let mut bd = f64::INFINITY;
    for r in RANGES.iter() {
        let d = if r.vdiv > vdiv {
            r.vdiv / vdiv
        } else {
            vdiv / r.vdiv
        };
        if d < bd {
            bd = d;
            best = r;
        }
    }
    best
}

/// DAC code for an offset in volts on a range.
pub fn offset_dac(r: &Range, volts: f64) -> u16 {
    let d = r.base_dac as f64 + volts * r.lsb_per_v as f64;
    if d < 0.0 {
        0
    } else if d > 65535.0 {
        65535
    } else {
        (d + 0.5) as u16
    }
}

/// Input couplings in menu order (datasheet: DC, AC, GND). There is no 50 Ohm path: the input
/// impedance is 1 MOhm only, so `Coupling::Dc50` must not be offered for this board.
pub const COUPLINGS: [&str; 3] = ["DC", "AC", "GND"];

/// The verified absolute gain at 1 V/div: a +-1 V signal reads +-7480 counts, 0.913 of the
/// nominal 8192 counts/V (measured app-free on the unit; sidecar notes, "AFE vertical path").
pub const COUNTS_PER_V_1VDIV: f64 = 7480.0;

/// The gain factor of the front end versus the nominal counts-per-volt of the ADC range.
pub fn gain_factor() -> f64 {
    COUNTS_PER_V_1VDIV / 8192.0
}

/// The ADC SPI word for the FPGA register `0x1034` (sidecar notes, from the stock driver):
/// chip select in bits 24-28, the 7-bit register in bits 16-22, the value XOR the register's
/// mask in the low half. The idle register reads `0x0255_0000` (the last write).
pub fn adc_word(second_chip: bool, reg: u8, mask: u16, value: u16) -> u32 {
    let sel = if second_chip { 0x0200_0000 } else { 0x0100_0000 };
    sel | (u32::from(reg) & 0x7f) << 16 | u32::from(mask ^ value)
}

/// The ADC power-on pair the band-limit change starts from: registers 0x54 and 0x55 on both
/// chips, written through `adc_word` with the stock mask of 0 (`0x54 = 0x4210`, `0x55 = 8`).
pub fn adc_band_limit_on() -> [u32; 4] {
    [
        adc_word(false, 0x54, 0, 0x4210),
        adc_word(false, 0x55, 0, 8),
        adc_word(true, 0x54, 0, 0x4210),
        adc_word(true, 0x55, 0, 8),
    ]
}

/// The 20 MHz band-limit filter word for AFE register `0x2e` (sidecar notes, from the stock
/// `CHANNEL_CONFIG` analysis): three trim values from the unit's 20 MHz calibration triple,
/// v3 in bits 0-5, v1 in 6-7 and v2 in 9-10.
pub fn band_limit_word(v1: u8, v2: u8, v3: u8) -> u16 {
    (u16::from(v1) & 3) << 6 | (u16::from(v2) & 3) << 9 | (u16::from(v3) & 0x3f)
}

/// The coupling word for AFE register `0x07`: bit 15 set for AC, clear for DC (the stock
/// sequence leaves the other bits alone).
pub fn coupling_word(sh: u16, ac: bool) -> u16 {
    (sh & 0x7fff) | u16::from(ac) << 15
}

/// Vertical sensitivity range in V/div (datasheet: 500 uV/div .. 10 V/div).
pub const VSCALE_MIN_DIV: f64 = 500e-6;
pub const VSCALE_MAX_DIV: f64 = 10.0;

/// The fine V/div steps of the front end in nV: the same 1-2-5 ladder as the DHO800/DHO900
/// (`crate::dho::afe::scale_ladder`), bounded by this series' 500 uV .. 10 V. The AFE snaps to the
/// nearest entry of `RANGES` (`range_for`), so every step here is a UI value, not a range.
pub fn scale_ladder() -> Vec<i64> {
    let mut v = Vec::with_capacity(801);
    for d in 0..5 {
        let b = 100_000 * 10i64.pow(d);
        v.extend((0..50).map(|k| b + k * b / 50));
        v.extend((0..60).map(|k| 2 * b + k * b / 20));
        v.extend((0..50).map(|k| 5 * b + k * b / 10));
    }
    v.push(10_000_000_000);
    let lo = (VSCALE_MIN_DIV * 1e9) as i64;
    let hi = (VSCALE_MAX_DIV * 1e9) as i64;
    v.retain(|&x| (lo..=hi).contains(&x));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_ordered_and_complete() {
        assert_eq!(RANGES.len(), 14);
        for w in RANGES.windows(2) {
            assert!(w[0].vdiv < w[1].vdiv, "ranges must ascend");
        }
        assert_eq!(RANGES[0].vdiv, 0.0005);
        assert_eq!(RANGES[13].vdiv, 10.0);
        assert_eq!(REG_MAP.len(), 13);
    }

    #[test]
    fn nearest_range_snaps_like_the_stock_software() {
        assert_eq!(range_for(0.9).vdiv, 1.0);
        assert_eq!(range_for(1.4).vdiv, 1.0);
        assert_eq!(range_for(1.6).vdiv, 2.0);
        assert_eq!(range_for(100.0).vdiv, 10.0);
    }

    #[test]
    fn ladder_is_bounded_ascending_and_snaps() {
        let l = scale_ladder();
        assert!(l.len() > 600, "ladder has {} steps", l.len());
        assert!(l.windows(2).all(|w| w[0] < w[1]));
        let lo = (VSCALE_MIN_DIV * 1e9) as i64;
        let hi = (VSCALE_MAX_DIV * 1e9) as i64;
        for &nv in &l {
            assert!(nv >= lo && nv <= hi, "{nv} nV outside the range");
            let r = range_for(nv as f64 / 1e9);
            assert!(r.vdiv >= VSCALE_MIN_DIV && r.vdiv <= VSCALE_MAX_DIV);
        }
        assert_eq!(l[0], lo, "first step is the 500 uV/div limit");
        assert_eq!(*l.last().unwrap(), hi, "last step is 10 V/div");
    }

    #[test]
    fn couplings_match_the_datasheet() {
        assert_eq!(COUPLINGS, ["DC", "AC", "GND"]);
        assert!(
            !COUPLINGS.contains(&"DC50"),
            "no 50 ohm path on this series"
        );
    }

    #[test]
    fn verified_gain_and_adc_words() {
        assert!((gain_factor() - 0.913).abs() < 0.001);
        assert_eq!(
            adc_word(false, 0x54, 0, 0x4210),
            0x0100_0000 | 0x54 << 16 | 0x4210
        );
        assert_eq!(adc_word(true, 0, 0, 0), 0x0200_0000, "the second chip selects itself");
        let on = adc_band_limit_on();
        assert_eq!(on[0] & 0xffff, 0x4210);
        assert_eq!(on[3] & 0xffff, 8);
    }

    #[test]
    fn band_limit_and_coupling_pack_their_bits() {
        assert_eq!(band_limit_word(2, 1, 0x15), 2 << 6 | 1 << 9 | 0x15);
        assert_eq!(coupling_word(0xa780, true), 0xa780 | 0x8000);
        assert_eq!(coupling_word(0xa780, false), 0x2780);
    }

    #[test]
    fn offset_dac_clamps() {
        let r = range_for(1.0);
        assert_eq!(offset_dac(r, 1e6), 65535);
        assert_eq!(offset_dac(r, -1e6), 0);
        assert_eq!(offset_dac(r, 0.0), r.base_dac);
    }
}
