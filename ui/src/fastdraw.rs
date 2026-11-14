// Copyright (C) 2026 Julian Decker
// SPDX-License-Identifier: GPL-2.0-or-later

//! Fast display path for the hardware source: a frame's raw samples go straight into the phosphor map, one vertical
//! span per screen column (min..max of the column's samples, joined to the previous column's last sample), instead
//! of the screen record of `board::to_record` (software trigger over a tenth of a screen, then 3600 interpolated or
//! peak-detected points) for every frame. Used for dense views (at least one sample per column) with a plain edge
//! trigger; the hardware already put the edge at `pre` to within a sample or two, so the crossing is only refined
//! (sub-sample, by linear interpolation) near it.

/// The level crossing of the edge nearest to `pre` within +-`span` samples, as a fractional sample index (rising:
/// below -> at or above `level`; falling the other way). `either`: both directions. `hyst` > 0: like the hardware
/// comparators, only the first crossing after the signal was `hyst` on the other side of the level counts, so noise on
/// an edge of the other direction is no edge.
pub fn edge_near(v: &[f32], pre: usize, level: f32, hyst: f32, rising: bool, either: bool, span: usize) -> Option<f64> {
    if v.len() < 2 {
        return None;
    }
    let lo = pre.saturating_sub(span).max(1);
    let hi = (pre + span).min(v.len() - 1);
    // back from the crossing at k: armed when the signal went past the band before it came back over the level
    let look = span.max(4096);
    let armed = |k: usize, up: bool| {
        if hyst <= 0.0 {
            return true;
        }
        for j in (k.saturating_sub(look)..k).rev() {
            let x = v[j];
            if up && x < level - hyst || !up && x > level + hyst {
                return true;
            }
            if up && x >= level || !up && x <= level {
                return false;
            }
        }
        false
    };
    let mut best: Option<f64> = None;
    for k in lo..=hi {
        let (a, b) = (v[k - 1], v[k]);
        let up = a < level && b >= level && (rising || either) && armed(k, true);
        let down = a > level && b <= level && (!rising || either) && armed(k, false);
        if up || down {
            let t = (k - 1) as f64 + ((level - a) / (b - a)) as f64;
            if best.is_none_or(|x| (t - pre as f64).abs() < (x - pre as f64).abs()) {
                best = Some(t);
            }
        }
    }
    best
}

/// Where the edge of the set direction is, seen at the resolution of the screen: `v` around `pre` (+-`span` samples)
/// smoothed by a centred moving average of `l` samples (one screen column), then a comparator with a hysteresis band of
/// `hyst` (total width, as the hardware's), the edge of the set direction nearest to `pre`. Noise larger than the band
/// (a slow sine at a high rate) cannot make an edge of the other direction look right, which a test on single samples
/// cannot rule out. Only edges at times `fits` accepts count. Returns the edge and the nearest edge of either direction
/// (for the statistics).
#[allow(clippy::too_many_arguments)]
pub fn edge_smooth(v: &[f32], pre: usize, level: f32, hyst: f32, rising: bool, either: bool, span: usize, l: usize, fits: impl Fn(f64) -> bool) -> (Option<f64>, Option<oos_dsp::edges::Edge>) {
    let l = l.max(1);
    let a = pre.saturating_sub(span + l);
    let b = (pre + span + l).min(v.len());
    if b <= a + 2 * l {
        return (None, None);
    }
    // running sum over the window; NaN samples count as the level (none in hardware frames)
    let x = |i: usize| if v[i].is_finite() { v[i] as f64 } else { level as f64 };
    let mut s = Vec::with_capacity(b - a);
    let mut sum = 0.0;
    let half = l / 2;
    for i in a..(a + l).min(b) {
        sum += x(i);
    }
    // s[j] = mean of v[a + j - half .. a + j - half + l], defined from j = half
    for j in 0..(b - a) {
        let lo = a + j;
        if j >= half && lo + l - half <= b {
            s.push((sum / l as f64) as f32);
            if lo + l - half < b {
                sum += x(lo + l - half) - x(lo - half);
            }
        } else {
            s.push(f32::NAN);
        }
    }
    let edges = oos_dsp::edges::edges(&s, level, hyst);
    let d = |e: &oos_dsp::edges::Edge| (a as f64 + e.t - pre as f64).abs();
    let near = |ok: &dyn Fn(&oos_dsp::edges::Edge) -> bool| edges.iter().filter(|e| ok(e) && fits(a as f64 + e.t)).min_by(|p, q| d(p).total_cmp(&d(q))).copied();
    let want = near(&|e| either || e.rising == rising).map(|e| a as f64 + e.t);
    (want, near(&|_| true).map(|e| oos_dsp::edges::Edge { t: a as f64 + e.t, ..e }))
}

/// Helper threads the fast path spreads a batch over, besides the frame loop itself: one per core of
/// `helper_cores`; elsewhere all cores but two, at most four.
pub fn helpers() -> usize {
    match helper_cores().len() {
        0 => std::thread::available_parallelism().map_or(1, |n| n.get()).saturating_sub(2).min(4),
        n => n.min(4),
    }
}

/// The cores of the helpers: on a big.LITTLE SoC the frame loop and the acquisition thread keep the fast cores
/// (RK3399: two A72), the helpers take the small ones (four A53), but not the one the DMA completion poller spins
/// on (`dma_poller_cpu`): it gives the core up only every 100 polls, so a helper there waits for it, the frame loop
/// waits for the helper, and the end of the transfer is seen later. Found at the first batch (frames have come in,
/// so the poller has run).
fn helper_cores() -> &'static [usize] {
    static C: std::sync::OnceLock<Vec<usize>> = std::sync::OnceLock::new();
    C.get_or_init(|| {
        let little = crate::http::little_core_list();
        let busy = std::fs::read_dir("/proc").ok().and_then(|d| dma_poller_cpu(d.flatten().map(|e| e.path())));
        let rest: Vec<usize> = little.iter().copied().filter(|&c| Some(c) != busy).collect();
        if rest.is_empty() { little } else { rest }
    })
}

/// The CPU of the busiest DMA completion poller (the xdma driver's kernel threads `cmpl_status_th<N>`, one bound to
/// each CPU, only the one serving the card-to-host engine works) among the process directories `procs`.
pub fn dma_poller_cpu(procs: impl Iterator<Item = std::path::PathBuf>) -> Option<usize> {
    procs
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.bytes().all(|b| b.is_ascii_digit())))
        .filter(|p| std::fs::read_to_string(p.join("comm")).is_ok_and(|c| c.starts_with("cmpl_status_th")))
        .filter_map(|p| std::fs::read_to_string(p.join("stat")).ok().and_then(|s| stat_time_cpu(&s)))
        .max_by_key(|&(t, _)| t)
        .filter(|&(t, _)| t > 0)
        .map(|(_, c)| c)
}

/// CPU time (utime + stime, ticks) and last CPU of a `/proc/<pid>/stat` line.
fn stat_time_cpu(s: &str) -> Option<(u64, usize)> {
    // the name in parentheses may hold spaces: the fields count from after its closing one (field 3, the state)
    let f: Vec<&str> = s.get(s.rfind(')')? + 1..)?.split_whitespace().collect();
    let num = |i: usize| f.get(i).and_then(|x| x.parse::<u64>().ok());
    Some((num(11)? + num(12)?, num(36)? as usize))
}

/// Runs `work` on the calling thread and on up to `n` helper threads on the small cores, all together; returns what
/// each gave, the calling thread's first.
fn together<R: Send>(n: usize, work: impl Fn() -> R + Sync) -> Vec<R> {
    if n == 0 {
        return vec![work()];
    }
    let work = &work;
    std::thread::scope(|s| {
        let hs: Vec<_> = (0..n)
            .map(|_| {
                s.spawn(move || {
                    crate::http::pin_to(helper_cores());
                    work()
                })
            })
            .collect();
        let mut out = vec![work()];
        out.extend(hs.into_iter().map(|h| h.join().expect("fast path helper")));
        out
    })
}

/// `items.iter().map(f).collect()` on the frame loop's thread and the helpers, in order. The items are handed out one
/// by one (a small core takes about twice as long as a fast one).
pub fn par_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let n = helpers().min(items.len().saturating_sub(1));
    if n == 0 {
        return items.iter().map(&f).collect();
    }
    let next = std::sync::atomic::AtomicUsize::new(0);
    let parts = together(n, || {
        let mut got = Vec::new();
        loop {
            let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if i >= items.len() {
                return got;
            }
            got.push((i, f(&items[i])));
        }
    });
    let mut all: Vec<(usize, R)> = parts.into_iter().flatten().collect();
    all.sort_unstable_by_key(|x| x.0);
    all.into_iter().map(|x| x.1).collect()
}

/// `f` for every item, each item to one thread (the frame loop's or a helper's), handed out one by one.
pub fn par_each<T: Send>(items: Vec<T>, f: impl Fn(T) + Sync) {
    let n = helpers().min(items.len().saturating_sub(1));
    if n == 0 {
        items.into_iter().for_each(f);
        return;
    }
    let q = std::sync::Mutex::new(items.into_iter());
    together(n, || loop {
        let Some(x) = q.lock().unwrap().next() else { return };
        f(x);
    });
}

/// Screen mapping of one channel: x = (k - ti) * x_per_sample + x0 (columns), y = val * y_per_volt + y0 (rows).
#[derive(Clone, Copy, Debug)]
pub struct Map {
    pub ti: f64,
    pub x_per_sample: f64,
    pub x0: f64,
    pub y_per_volt: f32,
    pub y0: f32,
}

/// Adds `w` to every pixel of `map` (w x h, row-major) the trace covers: per column the span of its samples, joined
/// to the last sample of the column before (so steep edges stay connected). Needs at least one sample per column
/// (`x_per_sample <= 1`). The fast path draws the same through `spans` and `add_spans`, band by band; this is the
/// whole map at once (tests, benchmark).
#[cfg(test)]
pub fn deposit(map: &mut [u16], w: usize, h: usize, v: &[f32], m: &Map, wgt: u16) {
    add_spans(map, w, 0, &spans(w, h, v, m), wgt);
}

/// One channel of a frame as `deposit` puts it on the screen: for each column from `c0` on, the first and last row its
/// samples cover (first > last: nothing in that column). Worked out once per frame; the rows are then added band by
/// band (`add_spans`), so even one channel is drawn on several cores.
pub struct Spans {
    pub c0: usize,
    pub rows: Vec<(i16, i16)>,
}

/// The `Spans` of `v` on a `w` x `h` map.
pub fn spans(w: usize, h: usize, v: &[f32], m: &Map) -> Spans {
    let mut out = Spans { c0: 0, rows: Vec::new() };
    if v.is_empty() || m.x_per_sample <= 0.0 {
        return out;
    }
    // first sample of column c: ceil(k_of(c)); per column one min/max over a slice (no per-sample screen math)
    let k_of = |x: f64| m.ti + (x - m.x0) / m.x_per_sample;
    let first = |c: i64| (k_of(c as f64).ceil().max(0.0) as usize).min(v.len());
    let c0 = (((-m.ti) * m.x_per_sample + m.x0).floor() as i64).max(0);
    let c1 = ((((v.len() as f64 - m.ti) * m.x_per_sample + m.x0).ceil()) as i64).min(w as i64);
    if c1 <= c0 {
        return out;
    }
    out.c0 = c0 as usize;
    out.rows.reserve((c1 - c0) as usize);
    let hh = h as i32;
    let mut ka = first(c0);
    for c in c0..c1 {
        let kb = first(c + 1);
        if kb <= ka {
            out.rows.push((1, 0));
            continue;
        }
        let (mut mn, mut mx) = (f32::INFINITY, f32::NEG_INFINITY);
        for &x in &v[ka..kb] {
            // f32::min/max skip NaN (gaps)
            mn = mn.min(x);
            mx = mx.max(x);
        }
        // the join to the previous column's last sample
        if ka > 0 && v[ka - 1].is_finite() {
            mn = mn.min(v[ka - 1]);
            mx = mx.max(v[ka - 1]);
        }
        ka = kb;
        if mn.partial_cmp(&mx).is_none_or(|o| o.is_gt()) {
            out.rows.push((1, 0));
            continue;
        }
        let (ya, yb) = (mn * m.y_per_volt + m.y0, mx * m.y_per_volt + m.y0);
        let r0 = (ya.min(yb).round() as i32).max(0);
        let r1 = (ya.max(yb).round() as i32).min(hh - 1);
        out.rows.push(if r0 <= r1 { (r0 as i16, r1 as i16) } else { (1, 0) });
    }
    out
}

#[cfg(test)]
/// Adds `wgt` to the pixels of `s` that fall in `band`: rows `r_lo` .. `r_lo + band.len() / w` of a map `w` wide.
pub fn add_spans(band: &mut [u16], w: usize, r_lo: usize, s: &Spans, wgt: u16) {
    let r_hi = r_lo + band.len() / w;
    if r_hi == r_lo {
        return;
    }
    for (k, &(a, b)) in s.rows.iter().enumerate() {
        // rows are never negative (`spans` clamps them); an empty column has a > b
        let (a, b) = ((a as usize).max(r_lo), (b as usize).min(r_hi - 1));
        if a > b {
            continue;
        }
        let mut i = (a - r_lo) * w + s.c0 + k;
        for _ in a..=b {
            band[i] = band[i].saturating_add(wgt);
            i += w;
        }
    }
}

/// The fast path collects a screen's frames as differences, column by column: a span of rows r0..=r1 with weight w
/// is +w at r0 and -w at r1 + 1 of its column (`h + 1` entries a column, wrapping u16), whatever its height; `fold_t`
/// sums them up once a screen into the row-major map. Written into the map directly, every pixel of a tall span was
/// another cache line (10 us/div, 4 channels, a 1 MHz sine: about 2 ms a frame on the scope).
#[inline(always)]
pub fn diff_span(band: &mut [u16], base: usize, r0: usize, r1: usize, wgt: u16) {
    band[base + r0] = band[base + r0].wrapping_add(wgt);
    band[base + r1 + 1] = band[base + r1 + 1].wrapping_sub(wgt);
}

/// `add_spans` into a band of the collecting differences (`diff_span`): `band` holds the columns `c_lo ..`, `h + 1`
/// entries each.
pub fn add_spans_t(band: &mut [u16], h: usize, c_lo: usize, s: &Spans, wgt: u16) {
    let hp = h + 1;
    let c_hi = c_lo + band.len() / hp;
    for c in s.c0.max(c_lo)..(s.c0 + s.rows.len()).min(c_hi) {
        let (r0, r1) = s.rows[c - s.c0];
        if r0 <= r1 {
            diff_span(band, (c - c_lo) * hp, r0 as usize, r1 as usize, wgt);
        }
    }
}

/// `deposit_dots` into a band of the collecting differences (`add_spans_t`): only the samples of its columns.
pub fn deposit_dots_t(band: &mut [u16], h: usize, c_lo: usize, v: &[f32], m: &Map, wgt: u16) {
    if v.is_empty() || m.x_per_sample <= 0.0 {
        return;
    }
    let hp = h + 1;
    let c_hi = (c_lo + band.len() / hp) as i64;
    let k_of = |x: f64| m.ti + (x - m.x0) / m.x_per_sample;
    let first = |c: i64| (k_of(c as f64).ceil().max(0.0) as usize).min(v.len());
    let c0 = (((-m.ti) * m.x_per_sample + m.x0).floor() as i64).max(c_lo as i64);
    let c1 = ((((v.len() as f64 - m.ti) * m.x_per_sample + m.x0).ceil()) as i64).min(c_hi);
    if c1 <= c0 {
        return;
    }
    let mut ka = first(c0);
    for c in c0..c1 {
        let kb = first(c + 1);
        let base = (c as usize - c_lo) * hp;
        for &x in &v[ka..kb.max(ka)] {
            let r = (x * m.y_per_volt + m.y0).round();
            // NaN fails both comparisons
            if r >= 0.0 && r < h as f32 {
                diff_span(band, base, r as usize, r as usize, wgt);
            }
        }
        ka = kb.max(ka);
    }
}

/// Sums the collected differences `acc` (`diff_span`, `h + 1` a column) of the rows `rows` up into the row-major
/// `map` (`w` x `h`) and clears them: in tiles, so map and differences stay in the cache; columns with nothing in a
/// tile are passed over.
pub fn fold_t(map: &mut [u16], acc: &mut [u16], w: usize, h: usize, rows: std::ops::Range<usize>) {
    const T: usize = 32;
    let hp = h + 1;
    // the entry below the last row touched holds its -w
    let (r_lo, r_end) = (rows.start.min(h), (rows.end.min(h) + 1).min(hp));
    if r_lo >= r_end {
        return;
    }
    let mut run = vec![0u16; w];
    for ra in (r_lo..r_end).step_by(T) {
        let rb = (ra + T).min(r_end);
        for ca in (0..w).step_by(T) {
            for c in ca..(ca + T).min(w) {
                let col = &mut acc[c * hp + ra..c * hp + rb];
                let mut v = run[c];
                if v == 0 && col.iter().fold(0u16, |a, &x| a | x) == 0 {
                    continue;
                }
                for (j, x) in col.iter_mut().enumerate() {
                    v = v.wrapping_add(*x);
                    *x = 0;
                    let r = ra + j;
                    if v != 0 && r < h {
                        let i = r * w + c;
                        map[i] = map[i].saturating_add(v);
                    }
                }
                run[c] = v;
            }
        }
    }
}

/// The rows a placed frame's spans can touch in channel `c`: (first, last + 1), empty when none.
pub fn span_rows(s: &Spans) -> (usize, usize) {
    s.rows.iter().filter(|r| r.0 <= r.1).fold((usize::MAX, 0), |(a, b), r| (a.min(r.0 as usize), b.max(r.1 as usize + 1)))
}

/// Dots display (`Model::dots`): every sample as one pixel, no spans and no joins; the brightness of a column follows
/// where its samples dwell. Same mapping and column walk as `deposit`.
#[cfg(test)]
pub fn deposit_dots(map: &mut [u16], w: usize, h: usize, v: &[f32], m: &Map, wgt: u16) {
    deposit_dots_band(map, w, 0, h, v, m, wgt);
}

#[cfg(test)]
/// `deposit_dots` for the rows `r_lo` .. `r_lo + band.len() / w` of a map `w` x `h`, held in `band`.
pub fn deposit_dots_band(band: &mut [u16], w: usize, r_lo: usize, h: usize, v: &[f32], m: &Map, wgt: u16) {
    if v.is_empty() || m.x_per_sample <= 0.0 {
        return;
    }
    let r_hi = (r_lo + band.len() / w).min(h);
    let k_of = |x: f64| m.ti + (x - m.x0) / m.x_per_sample;
    let first = |c: i64| (k_of(c as f64).ceil().max(0.0) as usize).min(v.len());
    let c0 = (((-m.ti) * m.x_per_sample + m.x0).floor() as i64).max(0);
    let c1 = ((((v.len() as f64 - m.ti) * m.x_per_sample + m.x0).ceil()) as i64).min(w as i64);
    let mut ka = first(c0);
    for c in c0..c1 {
        let kb = first(c + 1);
        for &x in &v[ka..kb.max(ka)] {
            let r = (x * m.y_per_volt + m.y0).round();
            // NaN fails both comparisons
            if r >= r_lo as f32 && r < r_hi as f32 {
                let i = (r as usize - r_lo) * w + c as usize;
                band[i] = band[i].saturating_add(wgt);
            }
        }
        ka = kb.max(ka);
    }
}

/// Max, min, sum and sum of squares of the finite samples of one channel of one frame: the voltage measurements of
/// the statistics (Max, Min, Pk-pk, Mean, RMS, Std dev) for every frame the fast path draws, not only the record.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Fold {
    pub n: u32,
    pub min: f32,
    pub max: f32,
    pub sum: f64,
    pub sumsq: f64,
}

impl Fold {
    pub fn of(v: &[f32]) -> Fold {
        let (mut mn, mut mx) = (f32::INFINITY, f32::NEG_INFINITY);
        // four partial sums in f32 per block of 4096, added up in f64: vector registers, no loss over long records
        let (mut sum, mut sumsq, mut n) = (0.0f64, 0.0f64, 0u32);
        for blk in v.chunks(4096) {
            let (mut s, mut q) = ([0.0f32; 4], [0.0f32; 4]);
            for (j, &x) in blk.iter().enumerate() {
                if x.is_finite() {
                    mn = mn.min(x);
                    mx = mx.max(x);
                    s[j & 3] += x;
                    q[j & 3] += x * x;
                    n += 1;
                }
            }
            sum += s.iter().map(|&x| x as f64).sum::<f64>();
            sumsq += q.iter().map(|&x| x as f64).sum::<f64>();
        }
        Fold { n, min: mn, max: mx, sum, sumsq }
    }

    /// The value of measurement `k`, None for one it does not cover (or too few samples, as `measure::analyze`).
    pub fn get(&self, k: oos_dsp::measure::Meas) -> Option<f64> {
        use oos_dsp::measure::Meas;
        if self.n < 4 {
            return None;
        }
        let n = self.n as f64;
        let mean = self.sum / n;
        Some(match k {
            Meas::Vmax => self.max as f64,
            Meas::Vmin => self.min as f64,
            Meas::Vpp => self.max as f64 - self.min as f64,
            Meas::Vavg => mean,
            Meas::Vrms => (self.sumsq / n).sqrt(),
            Meas::StdDev | Meas::VrmsAc => (self.sumsq / n - mean * mean).max(0.0).sqrt(),
            _ => return None,
        })
    }

    /// Measurements `get` covers.
    pub fn covers(k: oos_dsp::measure::Meas) -> bool {
        use oos_dsp::measure::Meas;
        matches!(k, Meas::Vmax | Meas::Vmin | Meas::Vpp | Meas::Vavg | Meas::Vrms | Meas::StdDev | Meas::VrmsAc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The poller's CPU from `/proc`: the busiest `cmpl_status_th*` thread, the others and other names left out.
    #[test]
    fn dma_poller_is_the_busiest_completion_thread() {
        let line = |name: &str, ut: u64, st: u64, cpu: usize| {
            format!("123 ({}) S 2 0 0 0 -1 2129984 0 0 0 0 {} {} 0 0 20 0 1 0 50 0 0 18446744073709551615 0 0 0 0 0 0 0 2147483647 0 0 0 0 17 {} 0 0 0 0 0", name, ut, st, cpu)
        };
        assert_eq!(stat_time_cpu(&line("cmpl status th", 7, 5, 2)), Some((12, 2)));
        let dir = std::env::temp_dir().join(format!("oos-poller-{}", std::process::id()));
        for (pid, name, ut, cpu) in [("10", "cmpl_status_th0", 3, 0), ("11", "cmpl_status_th1", 9000, 1), ("12", "cmpl_status_th2", 0, 2), ("13", "busyloop", 99999, 3), ("self", "cmpl_status_th9", 99999, 3)] {
            let d = dir.join(pid);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("comm"), format!("{}\n", name)).unwrap();
            std::fs::write(d.join("stat"), line(name, ut, 0, cpu)).unwrap();
        }
        let procs = || std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path());
        assert_eq!(dma_poller_cpu(procs()), Some(1));
        // a driver that never polled (interrupt mode): no core to avoid
        std::fs::write(dir.join("11/stat"), line("cmpl_status_th1", 0, 0, 1)).unwrap();
        std::fs::write(dir.join("10/stat"), line("cmpl_status_th0", 0, 0, 0)).unwrap();
        assert_eq!(dma_poller_cpu(procs()), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn edge_is_refined_near_pre() {
        // a ramp crossing 0.5 between samples 11 and 12 (at 11.25), another rising crossing far away
        let mut v: Vec<f32> = (0..40).map(|k| (k as f32 - 9.0) * 0.4 - 0.4).collect();
        v[30] = -1.0;
        v[31] = 1.0;
        let t = edge_near(&v, 11, 0.5, 0.0, true, false, 4).unwrap();
        assert!((t - 11.25).abs() < 1e-4, "{}", t);
        assert!(edge_near(&v, 11, 0.5, 0.0, false, false, 4).is_none());
        // the nearest of two crossings wins
        assert!((edge_near(&v, 29, 0.0, 0.0, true, false, 40).unwrap() - 30.5).abs() < 1e-4);
    }

    /// A slow sine with noise larger than the band, the hardware point on the falling edge: the smoothed search finds
    /// the rising edge half a period away, never the noise on the falling one.
    #[test]
    fn smoothed_search_takes_the_trend_not_the_noise() {
        // 12500 samples a period, amplitude 0.55, noise +-0.075 (pseudo random), level 0, band 0.06
        let mut r = 12345u32;
        let v: Vec<f32> = (0..50_000)
            .map(|k| {
                r = r.wrapping_mul(1_103_515_245).wrapping_add(12345);
                0.55 * (k as f32 / 12500.0 * std::f32::consts::TAU).sin() + ((r >> 16) as f32 / 65536.0 - 0.5) * 0.15
            })
            .collect();
        for pre in [6250 + 130, 6250 - 40, 6250, 18750 + 200] {
            let (e, any) = edge_smooth(&v, pre, 0.0, 0.06, true, false, 31_250, 62, |_| true);
            let t = e.unwrap();
            assert!((t - 12500.0).abs() < 60.0 || (t - 25000.0).abs() < 60.0, "pre {}: {}", pre, t);
            assert!(!any.unwrap().rising, "pre {}: the nearest edge is the falling one", pre);
        }
        // at the rising edge itself
        assert!((edge_smooth(&v, 12_480, 0.0, 0.06, true, false, 31_250, 62, |_| true).0.unwrap() - 12500.0).abs() < 60.0);
        // falling
        assert!((edge_smooth(&v, 12_480, 0.0, 0.06, false, false, 31_250, 62, |_| true).0.unwrap() - 6250.0).abs() < 60.0);
    }

    /// A falling edge with noise crosses the level upwards too: with hysteresis that is no rising edge, and the rising
    /// edge half a period away is found.
    #[test]
    fn hysteresis_keeps_noise_on_the_other_edge_out() {
        // a sine, 1000 samples a period, with a +-0.02 wiggle; it falls through 0 at 500
        let v: Vec<f32> = (0..3000).map(|k| (k as f32 / 1000.0 * std::f32::consts::TAU).sin() + if k % 2 == 0 { 0.02 } else { -0.02 }).collect();
        // without hysteresis the wiggle at the falling crossing looks like a rising edge
        assert!((edge_near(&v, 500, 0.0, 0.0, true, false, 16).unwrap() - 500.0).abs() < 3.0);
        // with it, nothing rising near 500 ...
        assert!(edge_near(&v, 500, 0.0, 0.1, true, false, 16).is_none());
        // ... and the rising crossing at 1000 within half a period
        assert!((edge_near(&v, 500, 0.0, 0.1, true, false, 600).unwrap() - 1000.0).abs() < 3.0);
        // the falling one is where the hardware said
        assert!((edge_near(&v, 500, 0.0, 0.1, false, false, 16).unwrap() - 500.0).abs() < 6.0);
    }

    #[test]
    fn spans_per_column_and_joined_edges() {
        let (w, h) = (10, 20);
        let mut map = vec![0u16; w * h];
        // a square: 4 samples per column, low (row 15) for 5 columns, then high (row 5)
        let v: Vec<f32> = (0..40).map(|k| if k < 20 { 0.0 } else { 1.0 }).collect();
        let m = Map { ti: 0.0, x_per_sample: 0.25, x0: 0.0, y_per_volt: -10.0, y0: 15.0 };
        deposit(&mut map, w, h, &v, &m, 1);
        let col = |c: usize| (0..h).filter(|&r| map[r * w + c] > 0).collect::<Vec<_>>();
        assert_eq!(col(2), [15]);
        // column 5 holds the step: joined from row 15 (last low sample) up to row 5
        assert_eq!(col(5), (5..=15).collect::<Vec<_>>());
        assert_eq!(col(7), [5]);
        // samples off screen are skipped; a shift by ti moves the step
        let mut map2 = vec![0u16; w * h];
        deposit(&mut map2, w, h, &v, &Map { ti: 8.0, ..m }, 1);
        assert_eq!((0..h).filter(|&r| map2[r * w + 3] > 0).count(), 11);
        // dots: only the sample rows, no join across the step, 4 samples per column add up
        let mut map3 = vec![0u16; w * h];
        deposit_dots(&mut map3, w, h, &v, &m, 1);
        let col3 = |c: usize| (0..h).filter(|&r| map3[r * w + c] > 0).collect::<Vec<_>>();
        assert_eq!((col3(5), map3[5 * w + 5], col3(4)), (vec![5], 4, vec![15]));
    }
}

