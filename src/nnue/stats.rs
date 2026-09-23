// Accumulator refresh instrumentation. Compiled to nothing without `--features nnue-stats`.
//
//     cargo build --release --features nnue-stats
//     taskset -c 0 ./target/release/mythos bench
//
// Never measure NPS with it on -- the atomics sit in the refresh path.
//
//   refreshes per node       how often the expensive path fires; a wider KING_LAYOUT raises this.
//   features per refresh     the diff the Finny table actually applied.
//   full-rebuild equivalent  the piece count; its ratio to the above is the whole value of the table.
//   cold refreshes           first hit on a cell, where the diff degenerates to a full rebuild.
//   l1 density               share of 4-byte L1 input chunks that are nonzero; what sparse affine can skip.

#[cfg(feature = "nnue-stats")]
mod imp {
    use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

    use crate::bench::group_digits;
    use crate::nnue::HL;

    static REFRESHES: AtomicU64 = AtomicU64::new(0);
    static COLD: AtomicU64 = AtomicU64::new(0);
    static DIFF_FEATURES: AtomicU64 = AtomicU64::new(0);
    static FULL_FEATURES: AtomicU64 = AtomicU64::new(0);
    static L1_CHUNKS: AtomicU64 = AtomicU64::new(0);
    static L1_NONZERO: AtomicU64 = AtomicU64::new(0);

    // `diff`: weight vectors this refresh touched. `full`: what a from-scratch rebuild would have, i.e. the piece count.
    pub fn record_refresh(diff: u64, full: u64, cold: bool) {
        REFRESHES.fetch_add(1, Relaxed);
        DIFF_FEATURES.fetch_add(diff, Relaxed);
        FULL_FEATURES.fetch_add(full, Relaxed);
        if cold {
            COLD.fetch_add(1, Relaxed);
        }
    }

    pub fn record_l1_input(input: &[u8]) {
        let chunks = input.chunks_exact(4);
        L1_CHUNKS.fetch_add(chunks.len() as u64, Relaxed);
        L1_NONZERO.fetch_add(chunks.filter(|c| c.iter().any(|&b| b != 0)).count() as u64, Relaxed);
    }

    pub fn reset() {
        for counter in [&REFRESHES, &COLD, &DIFF_FEATURES, &FULL_FEATURES, &L1_CHUNKS, &L1_NONZERO] {
            counter.store(0, Relaxed);
        }
    }

    pub fn report(nodes: u64) {
        let refreshes = REFRESHES.load(Relaxed);
        let cold = COLD.load(Relaxed);
        let diff = DIFF_FEATURES.load(Relaxed);
        let full = FULL_FEATURES.load(Relaxed);
        let chunks = L1_CHUNKS.load(Relaxed);
        let nonzero = L1_NONZERO.load(Relaxed);

        let per = |n: u64, d: u64| if d == 0 { 0.0 } else { n as f64 / d as f64 };

        println!();
        println!("  -- finny tables (nnue-stats) --");
        println!("  refreshes      : {:>13}  ({:.2} per 100 nodes)",
                 group_digits(refreshes), per(refreshes, nodes) * 100.0);
        println!("  cold           : {:>13}  ({:.3}% of refreshes)",
                 group_digits(cold), per(cold, refreshes) * 100.0);
        println!("  features       : {:>13}  ({:.1} per refresh)",
                 group_digits(diff), per(diff, refreshes));
        println!("  full rebuild   : {:>13}  ({:.1} per refresh)",
                 group_digits(full), per(full, refreshes));
        println!("  saved          : {:>12.1}%", (1.0 - per(diff, full)) * 100.0);

        println!();
        println!("  -- l1 sparsity (nnue-stats) --");
        println!("  evals          : {:>13}", group_digits(chunks / (HL / 4) as u64));
        println!("  nonzero chunks : {:>13}  ({:.1} of {} per eval)",
                 group_digits(nonzero), per(nonzero, chunks) * (HL / 4) as f64, HL / 4);
        println!("  density        : {:>12.1}%", per(nonzero, chunks) * 100.0);
    }
}

#[cfg(not(feature = "nnue-stats"))]
mod imp {
    #[inline(always)]
    pub fn record_refresh(_diff: u64, _full: u64, _cold: bool) {}

    #[inline(always)]
    pub fn record_l1_input(_input: &[u8]) {}

    #[inline(always)]
    pub fn reset() {}

    #[inline(always)]
    pub fn report(_nodes: u64) {}
}

pub use imp::{record_l1_input, record_refresh, report, reset};
