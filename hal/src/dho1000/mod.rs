// Copyright (C) 2026 Julian Decker
// SPDX-License-Identifier: GPL-2.0-or-later

//! DHO1000 series (DHO1074/DHO1104/DHO1204): what was measured on a DHO1204 for a port.
//!
//! This module is a **seed for a port**, not a working board. It holds only what was established at
//! the bench: the front-end range table (`afe`) and the acquisition register sequences (`acq`).
//! Nothing here is wired into `target::ACTIVE`; the stock family gate (`dho::target::family`) still
//! keeps a DHO10xx from driving hardware.
//!
//! What is known on the unit (details and measurements in `docs/protocols/dho1000.md`):
//! - the board is the same RK3399 + Zynq over PCIe/XDMA, the same stock kernel module list and the
//!   same register vocabulary as the DHO800/DHO900;
//! - a from-scratch acquisition init + per-batch record/play handshake works without the stock
//!   software and yields clean 1 Mpts captures at 10 MSa/s;
//! - roll mode streams at `depth / (100 * s_per_div)` and tops out around 200 kSa/s: 10 MSa/s is
//!   not reachable in roll mode;
//! - the front end takes the 14-range table in `afe`.
//!
//! What is missing and must be measured before a DHO1000 image drives hardware: FPGA start and
//! bitstream, the ADC/lane set-up, the sample-clock chip and its tables, trigger and LA blocks,
//! generator, front panel, and the calibration files in `/rigol/data`.

/// One register write: (BAR1 offset, value). The builders return these; applying them is the bus's
/// job, as in `crate::dho`.
pub type Write = (u32, u32);

pub mod acq;
pub mod afe;
pub mod avg;
pub mod counter;
pub mod display;
pub mod dvm;
pub mod export;
pub mod mask;
pub mod record;
pub mod series;
pub mod serialtrig;
pub mod trigger;
pub mod zone;
