// Copyright (C) 2026 Julian Decker
// SPDX-License-Identifier: GPL-2.0-or-later

//! One setting must not change another (eTobey in the forum, after a scope whose trigger types changed each other).
//! Each test steps one setting through all its values and back, with steps of the screen in between, and checks that
//! the setup (every saved setting) is what it was.

use super::*;
use crate::wave::Source;

fn ui() -> Ui {
    // the hardware source: the demo source keeps presets that suit its signals
    let mut u = Ui::new(Source::Hw { shared: Default::default(), seen: 0 });
    u.m.ch[1].on = true;
    u.m.meas = vec![(Meas::Vpp, WaveId::Ch(0)), (Meas::Freq, WaveId::Ch(1))];
    u.step(0.05);
    u
}

/// The keys whose values differ between two setups.
fn changed(a: &str, b: &str) -> Vec<String> {
    let map = |t: &str| t.lines().filter_map(|l| l.split_once(" = ")).map(|(k, v)| (k.to_string(), v.to_string())).collect::<std::collections::BTreeMap<_, _>>();
    let (a, b) = (map(a), map(b));
    a.keys().chain(b.keys()).filter(|k| a.get(*k) != b.get(*k)).cloned().collect::<std::collections::BTreeSet<_>>().into_iter().collect()
}

/// Parameters that belong to one trigger type only: a type sets its own when it is chosen.
const TYPE_OWN: [&str; 14] = ["trig_t1", "trig_when", "trig_level2", "trig_pol", "pat", "setup", "hold", "win_mode", "trig_src2", "trig_slope2", "nth_idle", "nth_n", "trig_bus", "trig_bus_val"];

/// Steps `it` once round all `n` of its values with steps of the screen between, and checks the setup after (but
/// for the keys of `own`).
fn round(u: &mut Ui, it: Item, n: usize, own: &[&str]) {
    let before = crate::store::setup_text(&u.m);
    for _ in 0..n {
        u.adjust(it, 1);
        u.step(0.05);
    }
    let after = crate::store::setup_text(&u.m);
    let bad: Vec<String> = changed(&before, &after).into_iter().filter(|k| !own.contains(&k.as_str()) && !k.starts_with("ser.")).collect();
    assert!(bad.is_empty(), "{:?} round: {:?} changed", it, bad);
}

#[test]
fn every_trigger_type_and_back_leaves_the_rest() {
    let mut u = ui();
    round(&mut u, Item::TrigType, TRIG_TYPES.len(), &TYPE_OWN);
}

#[test]
fn every_acquisition_mode_and_back_leaves_the_rest() {
    let mut u = ui();
    round(&mut u, Item::AcqMode, 4, &[]);
}

#[test]
fn every_analyser_page_and_back_leaves_the_rest() {
    let mut u = ui();
    let cv = Canvas::new(u.scr.w, u.scr.h);
    let mut scpi = crate::scpi::Scpi::open(0);
    let before = crate::store::setup_text(&u.m);
    for page in ["SPECtrum", "EYE", "POWer", "BODE", "MAGnetic", "INVerter", "DPT", "SCOPe"] {
        scpi.exec_line(&mut u, &cv, &format!(":VIEW {}", page));
        u.step(0.05);
    }
    let after = crate::store::setup_text(&u.m);
    assert!(changed(&before, &after).is_empty(), "pages round: {:?} changed", changed(&before, &after));
}

#[test]
fn draw_path_bottom_bar_and_features_leave_the_acquisition() {
    let mut u = ui();
    let keep = |m: &Model| (m.tscale, m.trig_type, m.trig_level.to_bits(), m.acq, m.ch.iter().map(|c| (c.on, c.vscale)).collect::<Vec<_>>(), m.meas.clone());
    let before = keep(&u.m);
    for it in [Item::AcqDrawPath, Item::BottomBar] {
        u.adjust(it, 1);
        u.step(0.05);
        u.adjust(it, 1);
        u.step(0.05);
    }
    for i in 0..features::features().len() {
        u.adjust(Item::FeatOn(i), 1);
        u.step(0.05);
        u.adjust(Item::FeatOn(i), 1);
    }
    assert_eq!(keep(&u.m), before);
}
