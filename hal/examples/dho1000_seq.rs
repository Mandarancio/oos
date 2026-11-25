// Copyright (C) 2026 Julian Decker
// SPDX-License-Identifier: GPL-2.0-or-later

//! Turns a DHO1000 acquisition sequence into `oos-hw seq` lines (`w <off> <val>`, `r <off> [n]`,
//! `poll <off> <mask> <want> <ms>`, `sleep <ms>`, `c2h <bytes>`), so the register programming stays in
//! `oos_hal::dho1000` (host-tested) and the device only applies it.
//!
//! ```text
//! cargo run --example dho1000_seq -- init 1000000 | adb ... > /tmp/seq.txt
//! oos-hw seq /tmp/seq.txt
//! ```
//!
//! The write-only registers are seeded with the values measured on the unit while the stock software
//! streamed (`0x4004=0x00000809`, `0x4008=0x08801101`, `0x400c=0xa2008000`, `0x1004=0x04000304`,
//! `0x1010=0xa0000300`, `0x105c=0x5555`); override any of them with `--s4004 0x...` etc.

use std::env;

use oos_hal::dho1000::{acq, avg, display, dvm, export, record, spu, trigger, zone};

fn emit(w: &[(u32, u32)]) {
    for (off, val) in w {
        println!("w 0x{off:04x} 0x{val:08x}");
    }
}

fn usage() -> ! {
    eprintln!(
        "usage: dho1000_seq <init <depth> | arm | idle | roll <mask> | passthrough | edge <ch> <lo> <hi> <rising|falling|either> | avg <count> <channels> | dvm <on|off> <mode> <ch> | zone <a> <b> <c> <d> | mask <index> <a> <b> <c>>"
    );
    std::process::exit(2);
}

fn main() {
    let mut sh = acq::Shadow {
        s4004: 0x0000_0809,
        s4008: 0x0880_1101,
        s400c: 0xa200_8000,
        s1004: 0x0400_0304,
        s1010: 0xa000_0300,
        s105c: 0x0000_5555,
    };
    let args: Vec<String> = env::args().skip(1).collect();
    // shadow overrides: --s4004 0x... (any of the six)
    let mut rest = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if let Some(k) = a.strip_prefix("--s") {
            let v = args
                .get(i + 1)
                .and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok());
            match (k, v) {
                ("4004", Some(v)) => sh.s4004 = v,
                ("4008", Some(v)) => sh.s4008 = v,
                ("400c", Some(v)) => sh.s400c = v,
                ("1004", Some(v)) => sh.s1004 = v,
                ("1010", Some(v)) => sh.s1010 = v,
                ("105c", Some(v)) => sh.s105c = v,
                _ => {
                    eprintln!("bad shadow override {a}");
                    std::process::exit(2);
                }
            }
            i += 2;
        } else {
            rest.push(a.clone());
            i += 1;
        }
    }
    let a: Vec<&str> = rest.iter().map(String::as_str).collect();
    if a.is_empty() {
        usage();
    }
    println!("# dho1000_seq {}", a.join(" "));
    match a[0] {
        "init" => {
            let depth: u32 = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(1_000_000);
            emit(&acq::init(&mut sh, depth, 1));
            emit(&acq::roll_start(&mut sh, 1));
            println!("# depth {depth}, roll started");
        }
        "arm" => {
            emit(&acq::arm_batch(&mut sh));
            println!("poll 0x4004 0x00080000 0x00000000 50");
            println!("w 0x1010 0x{:08x}", sh.s1010 & !0x1000_0000);
            println!("w 0x1010 0x{:08x}", sh.s1010 | 0x1000_0000);
            println!("w 0x1010 0x{:08x}", sh.s1010 & !0x1000_0000);
            println!("r 0x11e8");
            println!("c2h 15384");
            emit(&acq::batch_idle(&mut sh));
        }
        "idle" => emit(&acq::batch_idle(&mut sh)),
        "roll" => {
            let mask: u8 = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
            emit(&acq::roll_start(&mut sh, mask));
        }
        "passthrough" => emit(&display::passthrough()),
        "edge" => {
            let ch: usize = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
            let lo: u16 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(32768);
            let hi: u16 = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(lo);
            let when = a.get(4).copied().unwrap_or("rising");
            let (rising, either) = match when {
                "falling" => (false, false),
                "either" => (true, true),
                _ => (true, false),
            };
            emit(&trigger::edge(ch, lo, hi, rising, either));
        }
        "avg" => {
            let count: u32 = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(16);
            let chans: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);
            emit(&avg::set_avg_para(&mut sh, count, chans));
            emit(&avg::set_avg_sinc(&mut sh, count > 1));
        }
        "dvm" => {
            let on = a.get(1).copied() != Some("off");
            let mode: u8 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
            let ch: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(1);
            emit(&[dvm::set_dvm(0, on, mode, ch)]);
            println!("sleep 50");
            println!("r 0x224c 2");
        }
        "zone" => {
            let g = |k: usize| a.get(k).map(|s| *s != "0").unwrap_or(false);
            emit(&[zone::zone_trig_ctrl(g(1), g(2), g(3), g(4))]);
        }
        "spu" => {
            let rate: u32 = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(1_000_000);
            let depth: u32 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(1_000_000);
            emit(&spu::set_sample_rate(rate, 2_000_000_000));
            emit(&[spu::set_record_depth(depth), spu::set_mem_size(depth)]);
            emit(&[spu::set_anti_alias(1), spu::set_pre_sa_comp(false)]);
            println!("# rate {rate} Sa/s depth {depth}");
        }
        "record" => {
            let count: u32 = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(240);
            let interval: u64 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
            let mut ctrl = sh.s400c;
            emit(&record::set_record_seq(&mut ctrl, interval, 0, count, 0));
            println!("# record sequence: {count} frames, interval {interval} fs");
        }
        "export" => {
            let frames: u32 = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(4096);
            // export::Shadow overlays the acquisition ones with the display chain registers
            let mut esh = export::Shadow {
                s4004: sh.s4004,
                s4008: sh.s4008,
                s1170: 0,
                s1174: 0,
                s1154: 0,
                s1004: sh.s1004,
                s400c: sh.s400c,
            };
            emit(&export::init(&mut esh));
            emit(&export::window(0, frames));
            emit(&export::arm(&mut esh));
            println!("poll 0x4004 0x00080000 0x00000000 50");
            println!("r 0x11e8");
            println!("c2h {}", export::read_bytes(frames, 1).max(8));
        }
        "mask" => {
            let idx: u32 = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
            let g = |k: usize| a.get(k).map(|s| *s != "0").unwrap_or(false);
            emit(&[zone::mask_output(0, idx, g(2), g(3), g(4))]);
        }
        _ => usage(),
    }
}
