// Copyright (C) 2026 Julian Decker
// SPDX-License-Identifier: GPL-2.0-or-later

//! DHO1000 serial-protocol trigger receivers (TPU), reconstructed from the stock library.
//!
//! Evidence: `libscope.so` decompiled (`docs/decomp/libscope/full-decomp`), function addresses in
//! the doc comments. The register packings mirror the `DevSystemTPU_Set*` setters one-to-one; the
//! compositions return the writes the stock `applyCfg` performs per protocol. **Nothing here has
//! run on hardware yet.**
//!
//! Register map (TPU receiver blocks):
//!
//! | Block | Registers | Protocol |
//! |---|---|---|
//! | UART | `0x2100` cfg1, `0x2104` cfg2 | mode 4 |
//! | IIS | `0x2108`-`0x2114` | mode 5 (packing not reconstructed) |
//! | LIN | `0x2118`-`0x2138` | mode 6 |
//! | CAN | `0x213c`-`0x2164` | mode 7 |
//! | FlexRay | `0x2168`-`0x2174` | mode 8 (packing not reconstructed) |
//! | SPI | `0x2178`-`0x2188` | mode 0xc |
//! | I2C | `0x218c`-`0x21a0` | mode 0xd |
//! | 1553B / SENT | `0x21b0`-0x21bc / `0x21d0`-0x21f0 | modes 0xa / 0xb (not reconstructed) |
//!
//! Field meanings (condition, relation, parity, stop, width) follow the slot layout of the
//! DHO800/900 receivers (`hal/src/dho/uarttrig.rs`, `serialtrig.rs`): the same FPGA family keeps
//! the same field order; the stock settings offsets name them below where they were traced.
//! Not reconstructed: the field packings of IIS, FlexRay, 1553B and SENT.

use crate::dho1000::trigger::{
    iir, pattern_a, pattern_b, pattern_r, source_a, source_b, source_r, spu_bypass, to_cmp_src,
    trigger_mode, trigger_r, MODE_CAN, MODE_I2C, MODE_LIN, MODE_SPI, MODE_UART, SOURCE_A,
    SOURCE_B, SOURCE_R,
};
use crate::dho1000::Write;

// ---- UART (0x2100 block) ----------------------------------------------------------

pub const UART_CFG1: u32 = 0x2100;
pub const UART_CFG2: u32 = 0x2104;

/// `DevSystemTPU_SetUartClkDiv` @0x2afff0: bit-clock divider in bits 0-20 of `UART_CFG1`,
/// `1e15 / (baud * 4e6)` (a 4 ns bit-clock domain; 115200 baud -> 2170).
pub fn uart_clkdiv(baud: u32) -> u32 {
    let b = u64::from(baud) * 4_000_000;
    if b == 0 {
        0
    } else {
        (1_000_000_000_000_000 / b) as u32
    }
}

/// `DevSystemTPU_SetUartCtrl` @0x2b000c: condition (bits 21-22), relation (23-25), error type
/// (26-27), stop bits (28-29), parity (30-31); the divider in bits 0-20 of `sh` kept.
pub fn uart_ctrl(sh: u32, condition: u8, relation: u8, error: u8, stop: u8, parity: u8) -> Write {
    (
        UART_CFG1,
        u32::from(condition & 3) << 21
            | u32::from(relation & 7) << 23
            | u32::from(error & 3) << 26
            | u32::from(stop & 3) << 28
            | u32::from(parity) << 30
            | (sh & 0x001f_ffff),
    )
}

/// `DevSystemTPU_SetUarCtr2` @0x2b0040: data width in bits 0-2, the two compare bytes in 3-10 and
/// 11-18; bits 19-31 of `sh` kept.
pub fn uart_ctr2(sh: u32, width: u8, data_min: u8, data_max: u8) -> Write {
    (
        UART_CFG2,
        u32::from(width & 7)
            | (u32::from(data_min) & 0xff) << 3
            | (u32::from(data_max) & 0xff) << 11
            | (sh & 0xfff8_0000),
    )
}

/// UART trigger (`DrvUartTrigger_applyCfg` @0x3046bc): the R event carries the channel, pattern
/// (0, 0, 2, idle-level), mode 4, then the bit-clock and the two configuration words.
#[allow(clippy::too_many_arguments)]
pub fn uart(
    ch: usize,
    idle_level: bool,
    baud: u32,
    condition: u8,
    relation: u8,
    error: u8,
    stop: u8,
    parity: u8,
    width: u8,
    data_min: u8,
    data_max: u8,
) -> Vec<Write> {
    let src = to_cmp_src(ch as u8);
    let mut wr = 0u32;
    let mut wm = 0u32;
    let mut w = Vec::with_capacity(12);
    w.push(iir(0, ch as u8, 0x800));
    wr = source_r(wr, src, 0, src, 0).1;
    w.push((SOURCE_R, wr));
    wr = pattern_r(wr, 0, false, 2, idle_level).1;
    w.push((SOURCE_R, wr));
    w.extend_from_slice(&trigger_r(wr, 0, 0, 0));
    wm = spu_bypass(wm, true).1;
    w.push(trigger_mode(wm, MODE_UART));
    let div = uart_clkdiv(baud);
    w.push(uart_ctrl(u32::from(div), condition, relation, error, stop, parity));
    w.push(uart_ctr2(0, width, data_min, data_max));
    w
}

// ---- SPI (0x2178 block) -----------------------------------------------------------

pub const SPI_CFG1: u32 = 0x2178;
pub const SPI_TIMEOUT: u32 = 0x217c;
pub const SPI_DATA_MIN: u32 = 0x2180;
pub const SPI_DATA_MAX: u32 = 0x2184;

/// `DevSystemTPU_SetSpiCfg1` @0x2b061c: framing bit 0 (1 = idle timeout), relation bits 1-3,
/// timeout ticks bits 4-8.
pub fn spi_cfg1(framing_idle_timeout: bool, relation: u8, timeout: u8) -> Write {
    (
        SPI_CFG1,
        u32::from(framing_idle_timeout)
            | (u32::from(relation & 7) << 1)
            | (u32::from(timeout & 0x1f) << 4),
    )
}

/// `DevSystemTPU_SetSpiCfgCtrl` @0x2b0634: the three raw compare words.
pub fn spi_cmp(min: u32, max: u32, mask: u32) -> [Write; 3] {
    [(SPI_DATA_MIN, min), (SPI_DATA_MAX, max), (SPI_TIMEOUT, mask)]
}

/// SPI trigger (`DrvSpiTrigger_applyCfg` @0x303788): clock on event A, data on R, MISO on B,
/// mode 0xc. `timeout_ticks` is the pre-scaled idle-timeout the stock code carries in its
/// settings (fs / 4e6 in the applyCfg path; 0 when CS-framed).
#[allow(clippy::too_many_arguments)]
pub fn spi(
    clk_ch: usize,
    mosi_ch: usize,
    miso_ch: usize,
    clk_rising: bool,
    miso_rising: bool,
    framing_idle_timeout: bool,
    relation: u8,
    timeout_ticks: u8,
    min: u32,
    max: u32,
    mask: u32,
) -> Vec<Write> {
    let (ca, cb) = (to_cmp_src(clk_ch as u8), to_cmp_src(mosi_ch as u8));
    let da = to_cmp_src(miso_ch as u8);
    let mut wa = 0u32;
    let mut wb = 0u32;
    let mut wr = 0u32;
    let mut wm = 0u32;
    let mut w = Vec::with_capacity(16);
    for ch in [clk_ch, mosi_ch, miso_ch] {
        w.push(iir(0, ch as u8, 0x800));
    }
    wr = source_r(wr, cb, 0, cb, 0).1;
    w.push((SOURCE_R, wr));
    wr = pattern_r(wr, 2, false, 2, false).1;
    w.push((SOURCE_R, wr));
    wa = source_a(wa, ca, 0, ca, 0).1;
    w.push((SOURCE_A, wa));
    wa = pattern_a(wa, 2, clk_rising, 2, clk_rising).1;
    w.push((SOURCE_A, wa));
    wb = source_b(wb, da, 0, da, 0).1;
    w.push((SOURCE_B, wb));
    wb = pattern_b(wb, 2, miso_rising, 2, miso_rising).1;
    w.push((SOURCE_B, wb));
    wm = spu_bypass(wm, true).1;
    w.push(trigger_mode(wm, MODE_SPI));
    let timeout = if framing_idle_timeout { timeout_ticks } else { 0 };
    w.push(spi_cfg1(framing_idle_timeout, relation, timeout));
    w.extend_from_slice(&spi_cmp(min, max, mask));
    w
}

// ---- I2C (0x218c block) -----------------------------------------------------------

pub const I2C_CFG1: u32 = 0x218c;
pub const I2C_CFG2: u32 = 0x2190;
/// The 64-bit min/max/mask compare values (`SetIicCfgMinValue/MaxValue/MaskValue`); the stock
/// code packs the address and the data bytes into them (settings around +0x938..+0x944).
pub const I2C_DATA_MIN: u32 = 0x2194;
pub const I2C_DATA_MAX: u32 = 0x2198;
pub const I2C_DATA_MASK: u32 = 0x219c;
pub const I2C_CFG6: u32 = 0x21a0;

/// `DevSystemTPU_SetIicCfg1` @0x2b06a0: condition bits 0-2, address relation 3-5, data relation
/// 6-8, direction 9-10, 10-bit flag 11, data-byte count 12-14.
pub fn i2c_cfg1(
    sh: u32,
    condition: u8,
    addr_relation: u8,
    data_relation: u8,
    direction: u8,
    ten_bit: bool,
    data_bytes: u8,
) -> Write {
    (
        I2C_CFG1,
        u32::from(condition & 7)
            | u32::from(addr_relation & 7) << 3
            | u32::from(data_relation & 7) << 6
            | u32::from(direction & 3) << 9
            | u32::from(ten_bit) << 11
            | u32::from(data_bytes & 7) << 12
            | (sh & 0x8000),
    )
}

/// `DevSystemTPU_SetIicCfg2` @0x2b06f0: three 10-bit timing fields (start/stop/repeated-start
/// half-periods), halved in 7-bit-address mode by the caller.
pub fn i2c_cfg2(sh: u32, t1: u32, t2: u32, t3: u32) -> Write {
    (
        I2C_CFG2,
        (t1 & 0x3ff) | (t2 & 0x3ff) << 10 | (t3 & 0x3ff) << 20 | (sh & 0xc000_0000),
    )
}

/// `DevSystemTPU_SetIicCfgMinValue` @0x2b071c / `MaxValue` @0x2b076c / `MaskValue` @0x2b07c0: a
/// 40-bit compare value, the low word to the register and byte 4 bit 3 to `I2C_CFG6`.
pub fn i2c_value(reg: u32, v: u64) -> [Write; 2] {
    [
        (reg, v as u32),
        (I2C_CFG6, (((v >> 32) as u32 & 8) << 8) & 0x0800),
    ]
}

/// I2C trigger (`DrvIicTrigger_applyCfg` @0x2fffcc): SCL on event A, SDA on event B, both with
/// pattern (2, 0, 2, 0), mode 0xd. `addr7` halves the three timing fields, as the stock code
/// does in 7-bit-address mode. The 64-bit compare values carry the address and the data bytes
/// the way the stock code packs them.
#[allow(clippy::too_many_arguments)]
pub fn i2c(
    scl_ch: usize,
    sda_ch: usize,
    condition: u8,
    addr_relation: u8,
    data_relation: u8,
    direction: u8,
    ten_bit: bool,
    data_bytes: u8,
    addr7: bool,
    t1: u32,
    t2: u32,
    t3: u32,
    data_min: u64,
    data_max: u64,
    data_mask: u64,
) -> Vec<Write> {
    let (sa, sb) = (to_cmp_src(scl_ch as u8), to_cmp_src(sda_ch as u8));
    let mut wa = 0u32;
    let mut wb = 0u32;
    let mut wm = 0u32;
    let mut w = Vec::with_capacity(16);
    for ch in [scl_ch, sda_ch] {
        w.push(iir(0, ch as u8, 0x800));
    }
    wa = source_a(wa, sa, 0, sa, 0).1;
    w.push((SOURCE_A, wa));
    wa = pattern_a(wa, 2, false, 2, false).1;
    w.push((SOURCE_A, wa));
    wb = source_b(wb, sb, 0, sb, 0).1;
    w.push((SOURCE_B, wb));
    wb = pattern_b(wb, 2, false, 2, false).1;
    w.push((SOURCE_B, wb));
    wm = spu_bypass(wm, true).1;
    w.push(trigger_mode(wm, MODE_I2C));
    w.push(i2c_cfg1(0, condition, addr_relation, data_relation, direction, ten_bit, data_bytes));
    let h = if addr7 { 1 } else { 0 };
    w.push(i2c_cfg2(0, t1 >> h, t2 >> h, t3 >> h));
    w.extend_from_slice(&i2c_value(I2C_DATA_MIN, data_min));
    w.extend_from_slice(&i2c_value(I2C_DATA_MAX, data_max));
    w.extend_from_slice(&i2c_value(I2C_DATA_MASK, data_mask));
    w
}

// ---- CAN (0x213c block) -----------------------------------------------------------

pub const CAN_CFG1: u32 = 0x213c;
pub const CAN_RATE: u32 = 0x2140;
pub const CAN_ID_MIN: u32 = 0x2144;
pub const CAN_ID_MAX: u32 = 0x2148;
pub const CAN_ID_MASK: u32 = 0x214c;
pub const CAN_DAT_MIN: u32 = 0x2150;
pub const CAN_DAT_MAX: u32 = 0x2154;
pub const CAN_DAT_MASK: u32 = 0x2158;

/// `DevSystemTPU_SetCanCtrl1` @0x2b0318: six 2-3 bit condition fields in bits 0-16.
pub fn can_ctrl1(
    sh: u32,
    p1: u8,
    p2: u8,
    p3: u8,
    p4: u8,
    p5: u8,
    p6: u8,
) -> Write {
    (
        CAN_CFG1,
        u32::from(p1 & 3)
            | u32::from(p2 & 7) << 2
            | u32::from(p3 & 3) << 5
            | u32::from(p4 & 7) << 7
            | u32::from(p5 & 7) << 10
            | u32::from(p6 & 7) << 14
            | (sh & 0xfffe_2000),
    )
}

/// `DevSystemTPU_SetCanCtrl2` @0x2b0390: three fields in bits 18-24.
pub fn can_ctrl2(sh: u32, p1: u8, p2: bool, p3: u8) -> Write {
    (
        CAN_CFG1,
        u32::from(p1 & 3) << 18 | u32::from(p2) << 20 | u32::from(p3 & 0xf) << 21
            | (sh & 0xfe03_ffff),
    )
}

/// `DevSystemTPU_SetCanRateCfg` @0x2b03c0: the bit-clock divider `1e15 / (baud * 4e6)` in bits
/// 0-15 and the sample position (percent, scaled by 0.01) in bits 16-31.
pub fn can_rate(baud: u32, sample_percent: u32) -> Write {
    let div = uart_clkdiv(baud);
    let sample = (f64::from(sample_percent) * div as f64 * 0.01) as i32;
    (CAN_RATE, (div & 0xffff) | ((sample as u32) << 16))
}

/// `DevSystemTPU_SetCanIdCmpCfg` @0x2b03d4: the three raw ID compare words.
pub fn can_id_cmp(min: u32, max: u32, mask: u32) -> [Write; 3] {
    [(CAN_ID_MIN, min), (CAN_ID_MAX, max), (CAN_ID_MASK, mask)]
}

/// `DevSystemTPU_SetCanDatCmpCfg` @0x2b0434: the 64-bit data compare pair.
pub fn can_dat_cmp(min: u64, max: u64) -> [Write; 4] {
    [
        (CAN_DAT_MIN, min as u32),
        (CAN_DAT_MIN + 4, (min >> 32) as u32),
        (CAN_DAT_MAX, max as u32),
        (CAN_DAT_MAX + 4, (max >> 32) as u32),
    ]
}

/// CAN trigger (`DrvCanTrigger_applyCfg` @0x2fdf58): the R event carries the channel, pattern
/// (0, 0, 2, 0), mode 7, then the control, rate and compare words.
#[allow(clippy::too_many_arguments)]
pub fn can(
    ch: usize,
    baud: u32,
    sample_percent: u32,
    c1: [u8; 6],
    c2a: u8,
    c2b: bool,
    c2c: u8,
    id_min: u32,
    id_max: u32,
    id_mask: u32,
    dat_min: u64,
    dat_max: u64,
) -> Vec<Write> {
    let src = to_cmp_src(ch as u8);
    let mut wr = 0u32;
    let mut wm = 0u32;
    let mut w = Vec::with_capacity(14);
    w.push(iir(0, ch as u8, 0x800));
    wr = source_r(wr, src, 0, src, 0).1;
    w.push((SOURCE_R, wr));
    wr = pattern_r(wr, 0, false, 2, false).1;
    w.push((SOURCE_R, wr));
    wm = spu_bypass(wm, true).1;
    w.push(trigger_mode(wm, MODE_CAN));
    w.push(can_ctrl1(0, c1[0], c1[1], c1[2], c1[3], c1[4], c1[5]));
    w.push(can_ctrl2(0, c2a, c2b, c2c));
    w.push(can_rate(baud, sample_percent));
    w.extend_from_slice(&can_id_cmp(id_min, id_max, id_mask));
    w.extend_from_slice(&can_dat_cmp(dat_min, dat_max));
    w
}

// ---- LIN (0x2118 block) -----------------------------------------------------------

pub const LIN_CTRL1: u32 = 0x2118;
pub const LIN_RATE: u32 = 0x211c;
pub const LIN_DAT_MIN: u32 = 0x2124;
pub const LIN_DAT_MAX: u32 = 0x212c;

/// `DevSystemTPU_SetLinCtrl1` @0x2b00ec: four fields in bits 0-10.
pub fn lin_ctrl1(p1: u8, p2: u8, p3: u8, p4: u8) -> Write {
    (
        LIN_CTRL1,
        u32::from(p1 & 7)
            | u32::from(p2 & 7) << 3
            | u32::from(p3 & 7) << 6
            | u32::from(p4 & 3) << 9,
    )
}

/// `DevSystemTPU_SetLinRateCfg` @0x2b0220: the bit-clock divider and the sample position, as in
/// `can_rate`.
pub fn lin_rate(baud: u32, sample_percent: u32) -> Write {
    can_rate(baud, sample_percent)
}

/// `DevSystemTPU_SetLinDatCmpCfg` @0x2b0260: the 64-bit data compare pair.
pub fn lin_dat_cmp(min: u64, max: u64) -> [Write; 4] {
    [
        (LIN_DAT_MIN, min as u32),
        (LIN_DAT_MIN + 4, (min >> 32) as u32),
        (LIN_DAT_MAX, max as u32),
        (LIN_DAT_MAX + 4, (max >> 32) as u32),
    ]
}

/// LIN trigger (`DrvLinTrigger_applyCfg` @0x300d58): the R event carries the channel, pattern
/// (2, 0, 2, 0), mode 6.
#[allow(clippy::too_many_arguments)]
pub fn lin(
    ch: usize,
    baud: u32,
    sample_percent: u32,
    c1: [u8; 4],
    dat_min: u64,
    dat_max: u64,
) -> Vec<Write> {
    let src = to_cmp_src(ch as u8);
    let mut wr = 0u32;
    let mut wm = 0u32;
    let mut w = Vec::with_capacity(12);
    w.push(iir(0, ch as u8, 0x800));
    wr = source_r(wr, src, 0, src, 0).1;
    w.push((SOURCE_R, wr));
    wr = pattern_r(wr, 2, false, 2, false).1;
    w.push((SOURCE_R, wr));
    wm = spu_bypass(wm, true).1;
    w.push(trigger_mode(wm, MODE_LIN));
    w.push(lin_ctrl1(c1[0], c1[1], c1[2], c1[3]));
    w.push(lin_rate(baud, sample_percent));
    w.extend_from_slice(&lin_dat_cmp(dat_min, dat_max));
    w
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dho1000::trigger::MODE;

    #[test]
    fn uart_clkdiv_follows_the_stock_formula() {
        assert_eq!(uart_clkdiv(115_200), 2170, "1e15 / (115200 * 4e6)");
        assert_eq!(uart_clkdiv(9600), 26041);
        assert_eq!(uart_clkdiv(0), 0, "no divide by zero");
    }

    #[test]
    fn uart_cfg_words_pack_the_decoded_fields() {
        let (_, v) = uart_ctrl(0x001f_ffff, 2, 5, 1, 1, 1);
        assert_eq!(v & 0x001f_ffff, 0x001f_ffff, "divider kept");
        assert_eq!(v >> 21, 2 | 5 << 2 | 1 << 5 | 1 << 7 | 1 << 9);
        let (_, v) = uart_ctr2(0, 7, 0x55, 0xaa);
        assert_eq!(v, 7 | 0x55 << 3 | 0xaa << 11);
    }

    #[test]
    fn uart_composition_routes_over_the_r_event() {
        let w = uart(1, true, 115_200, 1, 0, 0, 0, 0, 7, 0x55, 0x55);
        assert!(w.contains(&(SOURCE_R, 0)));
        assert!(w.contains(&(SOURCE_R, 2 << 20 | 1 << 23)));
        assert!(w.contains(&(MODE, 0x1000 | MODE_UART as u32)));
        let (_, v) = *w.iter().find(|&&(r, _)| r == UART_CFG1).unwrap();
        assert_eq!(v & 0x001f_ffff, 2170, "divider in bits 0-20");
        assert_eq!(v >> 21 >> 9, 0, "parity in the two top bits");
    }

    #[test]
    fn spi_cfg1_packs_framing_relation_timeout() {
        assert_eq!(spi_cfg1(true, 3, 0x1f), (SPI_CFG1, 1 | 3 << 1 | 0x1f << 4));
        let w = spi_cmp(1, 2, 3);
        assert_eq!(w[0], (SPI_DATA_MIN, 1));
        assert_eq!(w[2], (SPI_TIMEOUT, 3));
    }

    #[test]
    fn spi_composition_places_the_three_sources() {
        let w = spi(1, 2, 3, true, false, true, 0, 0x10, 1, 2, 3);
        // clock (ch1) on A, data (ch2) on R, miso (ch3) on B, all pattern (2, x, 2, x)
        assert!(w.contains(&(SOURCE_A, 0u32 | 2 << 16 | 1 << 19 | 2 << 20 | 1 << 23)));
        assert!(w.contains(&(SOURCE_R, 2u32 | 2 << 8 | 2 << 16 | 2 << 20)));
        assert!(w.contains(&(SOURCE_B, 4u32 | 4 << 8 | 2 << 16 | 2 << 20)));
        assert!(w.contains(&(MODE, 0x1000 | MODE_SPI as u32)));
        // the pre-scaled idle-timeout byte lands in bits 4-8
        assert!(w.contains(&spi_cfg1(true, 0, 0x10)));
    }

    #[test]
    fn i2c_cfg1_and_cfg2_pack_their_fields() {
        let (_, v) = i2c_cfg1(0x8000, 4, 2, 1, 2, true, 3);
        assert_eq!(
            v,
            4 | 2 << 3 | 1 << 6 | 2 << 9 | 1 << 11 | 3 << 12 | 0x8000,
            "the shadow bit 15 kept"
        );
        let (_, v) = i2c_cfg2(0xc000_0000, 0x111, 0x222, 0x333);
        assert_eq!(v, 0x111 | 0x222 << 10 | 0x333 << 20 | 0xc000_0000);
    }

    #[test]
    fn i2c_values_split_the_forty_bits() {
        let w = i2c_value(I2C_DATA_MIN, 0x8_0000_0001);
        assert_eq!(w[0], (I2C_DATA_MIN, 1));
        assert_eq!(w[1], (I2C_CFG6, 0x0800), "byte-4 bit 3 in cfg6");
    }

    #[test]
    fn i2c_composition_halves_the_timing_in_seven_bit_mode() {
        let w = i2c(1, 2, 4, 0, 0, 0, false, 1, true, 100, 200, 300, 0x50, 0xff, 0xff);
        assert!(w.contains(&(SOURCE_A, 0u32 | 2 << 16 | 2 << 20)));
        assert!(w.contains(&(SOURCE_B, 2u32 | 2 << 8 | 2 << 16 | 2 << 20)));
        assert!(w.contains(&(MODE, 0x1000 | MODE_I2C as u32)));
        assert!(w.contains(&i2c_cfg2(0, 50, 100, 150)));
    }

    #[test]
    fn can_words_pack_ctrl_rate_and_compares() {
        let (_, v) = can_ctrl1(0xfffe_2000, 3, 7, 3, 7, 7, 7);
        assert_eq!(v, 3 | 7 << 2 | 3 << 5 | 7 << 7 | 7 << 10 | 7 << 14 | 0xfffe_2000);
        let (_, v) = can_ctrl2(0, 2, true, 0xf);
        assert_eq!(v, 2 << 18 | 1 << 20 | 0xf << 21);
        let (r, v) = can_rate(1_000_000, 75);
        assert_eq!(r, CAN_RATE);
        assert_eq!(v & 0xffff, 250, "1e15 / (1e6 * 4e6)");
        assert_eq!(v >> 16, (75.0 * 250.0 * 0.01) as u32);
    }

    #[test]
    fn can_composition_matches_the_stock_flow() {
        let w = can(1, 500_000, 75, [1, 2, 3, 4, 5, 6], 1, true, 2, 0x123, 0x456, 0x7ff, 1, 2);
        assert!(w.contains(&(SOURCE_R, 2 << 20)));
        assert!(w.contains(&(MODE, 0x1000 | MODE_CAN as u32)));
        assert!(w.contains(&(CAN_ID_MIN, 0x123)));
        assert!(w.contains(&(CAN_DAT_MIN + 4, 0)));
    }

    #[test]
    fn lin_words_and_composition() {
        assert_eq!(lin_ctrl1(5, 6, 7, 2), (LIN_CTRL1, 5 | 6 << 3 | 7 << 6 | 2 << 9));
        let w = lin(2, 19_200, 75, [1, 2, 3, 1], 0, 0xffffffff_ffffffff);
        assert!(w.contains(&(SOURCE_R, 2u32 | 2 << 8 | 2 << 16 | 2 << 20)));
        assert!(w.contains(&(MODE, 0x1000 | MODE_LIN as u32)));
        assert!(w.contains(&(LIN_DAT_MAX + 4, 0xffffffff)));
    }
}
