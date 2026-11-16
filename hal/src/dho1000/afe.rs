// Copyright (C) 2026 Julian Decker
// SPDX-License-Identifier: GPL-2.0-or-later

//! DHO1000 series: front-end range table as measured on a DHO1204 (docs/protocols/dho1000.md).
//!
//! Unlike the DHO800/DHO900 front end (`crate::dho::afe`, calibrated from `/rigol/data`), the table
//! below was recovered from the stock software and applied to the hardware: writing the 13 register
//! values of a range, then `OFF_DAC_REG` = `base_dac + lsb_per_v * volts`, sets V/div and offset.
//! The absolute gain was **not** verified against a reference: on the unit this table made a +-1 V
//! signal 13 px high at 1 V/div where the stock software shows 189 px. Treat the values as a
//! starting point for a calibration, not as verified output levels.

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
    fn offset_dac_clamps() {
        let r = range_for(1.0);
        assert_eq!(offset_dac(r, 1e6), 65535);
        assert_eq!(offset_dac(r, -1e6), 0);
        assert_eq!(offset_dac(r, 0.0), r.base_dac);
    }
}
