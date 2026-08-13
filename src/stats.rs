//! Optional search instrumentation.
//!
//! Everything here is written through the [`stat!`] macro, which expands to nothing
//! unless the `stats` feature is on — so ordinary builds, benches and SPRT matches
//! carry no counter arithmetic at all. `cargo xtask selfplay` turns the feature on
//! for its own build and reads these numbers back out of `Search::stats`.

/// Wraps instrumentation-only code. Compiled away without the `stats` feature.
#[cfg(feature = "stats")]
#[macro_export]
macro_rules! stat {
    ($($body:tt)*) => { $($body)* };
}

#[cfg(not(feature = "stats"))]
#[macro_export]
macro_rules! stat {
    ($($body:tt)*) => {};
}

/// One completed iterative-deepening iteration.
#[derive(Clone, Default)]
pub struct IterInfo {
    pub depth: usize,
    pub score: i32,
    pub nodes: u64,
    pub ms: u64,
    /// Aspiration re-searches this iteration needed before the window held.
    pub researches: usize,
    pub pv: String,
}

/// Counters for one search (one `go`, i.e. one move of a game).
#[derive(Clone, Default)]
pub struct Stats {
    pub qnodes: u64,
    pub seldepth: usize,

    pub tt_probe: u64,
    pub tt_hit: u64,
    pub tt_cut: u64,

    /// Beta cutoffs in `negamax`, and how many came from the first move tried.
    pub cutoffs: u64,
    pub cutoff_quiet: u64,
    /// Cutoff move index, bucketed: 0, 1, 2, 3, 4, 5, 6, 7+.
    pub cutoff_idx: [u64; 8],
    /// Nodes that searched every move without a cutoff.
    pub all_nodes: u64,

    pub nmp_try: u64,
    pub nmp_cut: u64,
    pub rfp_cut: u64,
    pub lmp_skip: u64,
    pub futility_skip: u64,
    pub see_skip: u64,

    pub lmr_reduced: u64,
    pub lmr_plies: u64,
    pub lmr_research: u64,
    pub pv_research: u64,

    pub iters: Vec<IterInfo>,
}

impl Stats {
    pub fn cutoff_first(&self) -> u64 {
        self.cutoff_idx[0]
    }

    /// Share of cutoffs that came from the first move tried — the headline
    /// move-ordering number.
    pub fn first_move_rate(&self) -> f64 {
        if self.cutoffs == 0 {
            0.0
        } else {
            self.cutoff_first() as f64 / self.cutoffs as f64
        }
    }

    pub fn add(&mut self, other: &Stats) {
        self.qnodes += other.qnodes;
        self.seldepth = self.seldepth.max(other.seldepth);
        self.tt_probe += other.tt_probe;
        self.tt_hit += other.tt_hit;
        self.tt_cut += other.tt_cut;
        self.cutoffs += other.cutoffs;
        self.cutoff_quiet += other.cutoff_quiet;
        for i in 0..8 {
            self.cutoff_idx[i] += other.cutoff_idx[i];
        }
        self.all_nodes += other.all_nodes;
        self.nmp_try += other.nmp_try;
        self.nmp_cut += other.nmp_cut;
        self.rfp_cut += other.rfp_cut;
        self.lmp_skip += other.lmp_skip;
        self.futility_skip += other.futility_skip;
        self.see_skip += other.see_skip;
        self.lmr_reduced += other.lmr_reduced;
        self.lmr_plies += other.lmr_plies;
        self.lmr_research += other.lmr_research;
        self.pv_research += other.pv_research;
    }
}
