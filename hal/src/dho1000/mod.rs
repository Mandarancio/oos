// Copyright (C) 2026 Julian Decker
// SPDX-License-Identifier: GPL-2.0-or-later

//! DHO1000 series (DHO1074/DHO1104/DHO1204): the reconstruction for a port.
//!
//! This module is a **seed for a port**, not a working board. Measured on the unit (`acq`, `afe`,
//! `export`'s handshake); reconstructed from the stock `libscope.so` (`avg`, `counter`, `display`,
//! `dvm`, `mask`, `record`, `serialtrig`, `spu`, `trigger`, `zone`) or read from the stock
//! firmware (`clock`); rated from the datasheet (`series`). Nothing here is wired into
//! `target::ACTIVE`; the stock family gate (`dho::target::family`) still keeps a DHO10xx from
//! driving hardware.
//!
//! What is known on the unit (details and measurements in `docs/protocols/dho1000.md`):
//! - the board is the same RK3399 + Zynq over PCIe/XDMA, the same stock kernel module list and the
//!   same register vocabulary as the DHO800/DHO900;
//! - a from-scratch acquisition init + per-batch record/play handshake works without the stock
//!   software and yields clean 1 Mpts captures at 10 MSa/s, and the app-free export handshake
//!   (`export`) reads record pages of 1 M frames at 12-20 MB/s;
//! - roll mode streams at `depth / (100 * s_per_div)` and tops out around 200 kSa/s: 10 MSa/s is
//!   not reachable in roll mode;
//! - the front end takes the 14-range table in `afe`, and its absolute gain was verified app-free;
//! - the sample clock is an LMK04821, initialised by the stock `spi2pll` boot step from the table
//!   in `clock`.
//!
//! What is missing and must be measured on the unit before a DHO1000 image drives hardware: the
//! FPGA start and bitstream path, the ADC/lane set-up (deskew, eye scan), the front panel, the
//! calibration files in `/rigol/data`, and on-unit validation of every reconstructed sequence.

/// One register write: (BAR1 offset, value). The builders return these; applying them is the bus's
/// job, as in `crate::dho`.
pub type Write = (u32, u32);

pub mod acq;
pub mod afe;
pub mod avg;
pub mod clock;
pub mod counter;
pub mod display;
pub mod dvm;
pub mod export;
pub mod mask;
pub mod record;
pub mod series;
pub mod serialtrig;
pub mod spu;
pub mod trigger;
pub mod zone;
