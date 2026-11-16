// Copyright (C) 2026 Julian Decker
// SPDX-License-Identifier: GPL-2.0-or-later

//! DHO1000 series: model table, rated limits and menu data.
//!
//! The numbers here are the **datasheet** values (DHO1000 Data Sheet, overview tables), not
//! measured on a unit, except where marked. They are what the neutral code needs from a board before
//! a port has measured its own: sample rates and memory per number of channels on, the depth menu,
//! and the thresholds where the stock software leaves YT for roll and scan.

use crate::model::{AfgMax, Caps, Limits};

/// Series name as a person says it.
pub const SERIES: &str = "DHO1000";

/// (model, OOS bandwidth code, channels). 70 MHz = code 5, 100 MHz = 6, 200 MHz = 9.
const MODELS: [(&str, u32, usize); 6] = [
    ("DHO1072", 5, 2),
    ("DHO1074", 5, 4),
    ("DHO1102", 6, 2),
    ("DHO1104", 6, 4),
    ("DHO1202", 9, 2),
    ("DHO1204", 9, 4),
];

/// Vertical divisions of the graticule (datasheet: 10 x 8).
pub const DIVISIONS_X: usize = 10;
pub const DIVISIONS_Y: usize = 8;
/// ADC resolution in bits (12; 16 in high-resolution mode).
pub const RESOLUTION_BITS: u32 = 12;

/// A model name of the DHO1000 family: "DHO1" + four digits (DHO1074, DHO1104, DHO1204, ...).
pub fn family(name: &str) -> bool {
    let n = name.trim().to_ascii_uppercase();
    let b = n.as_bytes();
    n.starts_with("DHO1") && b.len() == 7 && b[4..].iter().all(u8::is_ascii_digit)
}

/// Capabilities of a model name (case-insensitive), from the datasheet table.
pub fn caps(name: &str) -> Option<Caps> {
    let n = name.trim().to_ascii_uppercase();
    let &(tn, code, channels) = MODELS.iter().find(|e| e.0 == n)?;
    Some(Caps {
        name: tn.to_string(),
        series: SERIES,
        channels,
        rated_bw_mhz: crate::model::bw_code_mhz(code).unwrap_or(0),
        bw_code: code,
        la: false,
        generators: 0,
        ext_trig: channels == 2,
        edu: false,
    })
}

/// Rated limits of a DHO1000 of `channels` analog inputs (datasheet, standard memory).
///
/// Sampling: 2 GSa/s with one channel on, 1 GSa/s with two; a 4-channel model drops to 500 MSa/s
/// with all four on. Memory (standard): 50 Mpts / 25 Mpts / 12.5 Mpts for 1 / 2 / 3-4 channels on,
/// so a 2-channel model keeps 25 Mpts with both on. The optional 100 Mpts is not assumed.
/// There is no logic analyser and no generator on this series: the `mem_la` columns repeat the
/// analog ones and the generator maxima are zero. `adc_clock_hz` is the rated 2 GSa/s ceiling,
/// **not** measured: the sample clock chip has not been identified on this board.
pub fn limits(channels: usize) -> Limits {
    let (srate, mem) = if channels <= 2 {
        (
            [2.0e9, 1.0e9, 1.0e9],
            [50_000_000u64, 25_000_000, 25_000_000],
        )
    } else {
        (
            [2.0e9, 1.0e9, 0.5e9],
            [50_000_000u64, 25_000_000, 12_500_000],
        )
    };
    Limits {
        max_srate: srate,
        max_mem: mem,
        max_mem_la: mem,
        afg_max_hz: AfgMax {
            sine: 0.0,
            square: 0.0,
            ramp: 0.0,
            pulse: 0.0,
            noise: 0.0,
            dc: 0.0,
            arb: 0.0,
        },
        adc_clock_hz: 2.0e9,
        afg_clock_hz: 0.0,
    }
}

/// The depth menu. Datasheet standard depths are 50 Mpts (one channel on), 25 Mpts (two,
/// 4-channel models: half) and 12.5 Mpts (all four); 100 Mpts is an option and left out.
pub const DEPTHS: [(&str, u64); 7] = [
    ("Auto", 0),
    ("1k", 1_000),
    ("10k", 10_000),
    ("100k", 100_000),
    ("1M", 1_000_000),
    ("10M", 10_000_000),
    ("25M", 25_000_000),
];

/// Time base with which the stock software leaves YT for roll (datasheet: >= 50 ms/div, or
/// >= 100 ms/div selectable). Measured on a DHO1204: roll streams at 50 ms/div.
pub const ROLL_MIN_S_PER_DIV: f64 = 0.05;
/// The selectable second roll threshold (datasheet: >= 100 ms/div).
pub const ROLL_MIN_S_PER_DIV_SLOW: f64 = 0.1;
/// Scan mode from this time base (datasheet: >= 200 ms/div).
pub const SCAN_MIN_S_PER_DIV: f64 = 0.2;

/// Time base range (s/div): 2 ns .. 1 ks on the 200 MHz models, 5 ns .. 1 ks on the others.
pub fn tdiv_range(bw_mhz: u32) -> (f64, f64) {
    if bw_mhz >= 200 {
        (2e-9, 1e3)
    } else {
        (5e-9, 1e3)
    }
}

/// Input impedance: 1 MOhm only (datasheet), i.e. no 50 Ohm path.
pub const HAS_50OHM: bool = false;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn family_matches_only_dho10xx() {
        for m in ["DHO1204", "dho1074", " DHO1102 "] {
            assert!(family(m), "{m}");
        }
        for m in ["DHO924S", "DHO804", "DHO4000", "MHO98", "DHO120", ""] {
            assert!(!family(m), "{m}");
        }
    }

    #[test]
    fn model_table_matches_the_datasheet() {
        let c = caps("DHO1204").unwrap();
        assert_eq!(
            (c.channels, c.rated_bw_mhz, c.la, c.generators),
            (4, 200, false, 0)
        );
        let c = caps("DHO1072").unwrap();
        assert_eq!((c.channels, c.rated_bw_mhz, c.ext_trig), (2, 70, true));
        assert!(caps("DHO1204-EDU").is_none());
    }

    #[test]
    fn rated_rates_and_memory_per_channels_on() {
        let l = limits(4);
        assert_eq!(l.srate(1), 2.0e9);
        assert_eq!(l.srate(2), 1.0e9);
        assert_eq!(l.srate(4), 0.5e9);
        assert_eq!(
            (l.mem(1, false), l.mem(2, false), l.mem(4, false)),
            (50_000_000, 25_000_000, 12_500_000)
        );
        let l = limits(2);
        assert_eq!(
            l.srate(2),
            1.0e9,
            "a 2-channel model keeps 1 GSa/s with both on"
        );
        assert_eq!(l.mem(2, false), 25_000_000);
        assert_eq!(l.afg_max_hz.sine, 0.0, "no generator on this series");
    }

    #[test]
    fn measured_roll_threshold_matches_the_datasheet() {
        // our DHO1204 streamed in roll at 50 ms/div, the datasheet threshold
        assert_eq!(ROLL_MIN_S_PER_DIV, 0.05);
        assert!(SCAN_MIN_S_PER_DIV >= ROLL_MIN_S_PER_DIV);
        assert!(
            tdiv_range(200).0 < tdiv_range(100).0,
            "200 MHz models reach 2 ns/div"
        );
    }

    #[test]
    fn depths_are_ascending_and_within_the_standard_max() {
        let mut last = 0;
        for (_, d) in DEPTHS.iter().filter(|(_, d)| *d != 0) {
            assert!(*d > last);
            last = *d;
        }
        assert!(last <= 25_000_000, "optional 100 Mpts is not assumed");
    }
}
