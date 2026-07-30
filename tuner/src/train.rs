use std::fmt::Write as _;
use std::ops::Range;
use std::path::Path;
use mythos::eval::trace::{initial_weights, BISHOP_MOB, BISHOP_PAIR, DOUBLED_PAWN, ISOLATED_PAWN,
    KNIGHT_MOB, NUM_PARAMS, PASSED_PAWN, PSQT, QUEEN_MOB, ROOK_MOB, TEMPO};
use crate::dataset::Dataset;
use crate::format::{unpack, Record};

pub type Weights = [[f64; 2]; NUM_PARAMS];

fn sigmoid(x: f64) -> f64 {
    1f64 / (1f64 + (-x).exp())
}

// divergent guard
const EPS: f64 = 1e-15;

fn squash(x: f64) -> f64 {
    sigmoid(x).clamp(EPS, 1.0 - EPS)
}

fn from_initial() -> Weights {
    let mut result: Weights = [[0f64; 2]; NUM_PARAMS];
    let initial_weights = initial_weights();
    for (i, s) in initial_weights.iter().enumerate() {
        result[i][0] = s.0 as f64;
        result[i][1] = s.1 as f64;
    }

    result
}

pub struct Trainer {
    dataset: Dataset,
    weights: Weights,
    k: f64,
    split: usize,
    adam: Adam
}

impl Trainer {
    pub fn new(dataset: Dataset) -> Self {
        let split = dataset.len() * 9 / 10;
        let mut trainer = Self {
            dataset, weights: from_initial(), k: 0.0, split, adam: Adam::new(1.0)
        };
        trainer.k = trainer.fit_k();

        trainer
    }

    pub fn k(&self) -> f64 {
        self.k
    }

    pub fn weights(&self) -> &Weights {
        &self.weights
    }

    pub fn train_range(&self) -> Range<usize> {
        0..self.split
    }

    pub fn val_range(&self) -> Range<usize> {
        self.split..self.dataset.len()
    }

    fn energy(record: &Record, coeffs: &[u16], weights: &Weights) -> f64 {
        let mut mg = 0f64;
        let mut eg = 0f64;

        let mg_weight = record.phase as f64 / 24.0;
        let eg_weight = 1.0 - mg_weight;
        for packed in coeffs {
            let (i, value) = unpack(*packed);
            mg += value as f64 * weights[i][0];
            eg += value as f64 * weights[i][1];
        }

        mg * mg_weight + eg * eg_weight + record.frozen as f64
    }

    fn loss(dataset: &Dataset, weights: &Weights, k: f64, range: Range<usize>) -> f64 {
        // trust that energies is correct
        let len = range.end - range.start;
        let mut result = 0f64;
        for i in range {
            let (record, coeff) = dataset.entry(i);
            let r = record.get_result();
            let s = squash(k * Self::energy(record, coeff, weights));
            result += r * s.ln() + (1.0 - r) * (1.0 - s).ln()
        }

        -result / (len as f64)
    }

    pub fn frozen_energies(&self, range: Range<usize>) -> (Vec<f64>, Vec<f64>) {
        let energies = range.clone()
            .map(|i| {
                let (record, coeff) = self.dataset.entry(i);
                Self::energy(record, coeff, &self.weights)
            })
            .collect();

        let results = self.dataset.records()[range].iter().map(Record::get_result).collect();

        (energies, results)
    }

    pub fn fit_k(&self) -> f64 {
        let (energies, results) = self.frozen_energies(self.train_range());

        let mut lo = 0.001f64;
        let mut hi = 0.02f64;
        for _ in 0..40 {
            let m1 = lo + (hi - lo) / 3.0;
            let m2 = hi - (hi - lo) / 3.0;
            if loss_over(&energies, &results, m1) < loss_over(&energies, &results, m2) {
                hi = m2
            } else {
                lo = m1
            }
        }

        (lo + hi) / 2.0
    }

    pub fn epoch(&mut self) -> f64 {
        let (gradient, loss) = self.gradient();
        self.adam.step(&mut self.weights, &gradient);

        loss
    }

    // validation set loss for double check 
    pub fn val_loss(&self) -> f64 {
        Self::loss(&self.dataset, &self.weights, self.k, self.val_range())
    }

    fn gradient(&self) -> (Weights, f64) {
        let mut gradient: Weights = [[0.0; 2]; NUM_PARAMS];
        let len = (self.train_range().end - self.train_range().start) as f64;
        let mut loss: f64 = 0.0;

        for i in self.train_range() {
            let (record, coeffs) = self.dataset.entry(i);
            let mg_weight = record.phase as f64 / 24.0;
            let eg_weight = 1.0 - mg_weight;

            let mut mg: f64 = 0.0;
            let mut eg: f64 = 0.0;

            for packed in coeffs {
                let (index, value) = unpack(*packed);

                mg += self.weights[index][0] * value as f64;
                eg += self.weights[index][1] * value as f64;
            }

            let e = mg * mg_weight + eg * eg_weight + record.frozen as f64;
            let s = squash(self.k() * e);
            let r = record.get_result();

            let g = self.k() * (s - r) / len;

            for packed in coeffs {
                let (index, val) = unpack(*packed);
                gradient[index][0] += g * mg_weight * (val as f64);
                gradient[index][1] += g * eg_weight * (val as f64);
            }

            loss += r * s.ln() + (1.0 - r) * (1.0 - s).ln();
        }

        (gradient, -loss / len)
    }
}

pub struct Adam {
    m: Weights,
    v: Weights,
    t: u64,
    lr: f64
}


impl Adam {
    const BETA1: f64 = 0.9;
    const BETA2: f64 = 0.999;
    const EPS: f64 = 1e-8;

    fn new(lr: f64) -> Self {
        Self {
            m: [[0.0; 2]; NUM_PARAMS],
            v: [[0.0; 2]; NUM_PARAMS],
            t: 0,
            lr
        }
    }

    fn step(&mut self, weights: &mut Weights, grads: &Weights) {
        self.t += 1;

        let bc1 = 1.0 - Self::BETA1.powi(self.t as i32);
        let bc2 = 1.0 - Self::BETA2.powi(self.t as i32);

        let len = weights.len();
        for i in 0..len {
            let g = grads[i];


            self.m[i][0] = Self::BETA1 * self.m[i][0] + (1.0 - Self::BETA1) * g[0];
            self.m[i][1] = Self::BETA1 * self.m[i][1] + (1.0 - Self::BETA1) * g[1];
            self.v[i][0] = Self::BETA2 * self.v[i][0] + (1.0 - Self::BETA2) * g[0] * g[0];
            self.v[i][1] = Self::BETA2 * self.v[i][1] + (1.0 - Self::BETA2) * g[1] * g[1];

            let m_hat_0 = self.m[i][0] / bc1;
            let m_hat_1 = self.m[i][1] / bc1;
            let v_hat_0 = self.v[i][0] / bc2;
            let v_hat_1 = self.v[i][1] / bc2;

            weights[i][0] -= self.lr * m_hat_0 / (v_hat_0.sqrt() + Self::EPS);
            weights[i][1] -= self.lr * m_hat_1 / (v_hat_1.sqrt() + Self::EPS);
        }

    }

}


fn loss_over(energies: &[f64], results: &[f64], k: f64) -> f64 {
    let mut result = 0f64;
    for i in 0..energies.len() {
        let s = squash(k * energies[i]);
        result += results[i] * s.ln() + (1.0 - results[i]) * (1.0 - s).ln()
    }

    -result / (energies.len() as f64)
}

pub fn fit_k(dir: &str) {
    let trainer = Trainer::new(Dataset::open(dir));
    let (energies, results) = trainer.frozen_energies(trainer.train_range());

    println!("loss curve:");
    for i in 0..=20 {
        let k = 0.001 + (0.02 - 0.001) * i as f64 / 20.0;
        println!("  K = {k:.5}  loss = {:.6}", loss_over(&energies, &results, k));
    }

    // Trainer::new already fit this — re-running the search here would be a third
    // pass over the data for an answer we hold.
    let k = trainer.k();
    let loss = loss_over(&energies, &results, k);

    // sigmoid(0) is exactly 0.5, so K = 0 evaluates the same loss at a flat
    // prediction — the baseline is the objective itself, not a second formula
    // that has to be kept in sync with it. Equals ln 2 for cross-entropy.
    let baseline = loss_over(&energies, &results, 0.0);

    println!();
    println!("K        = {k:.6}");
    println!("loss     = {loss:.6} (cross-entropy)");
    println!("baseline = {baseline:.6} (flat 0.5)");

    assert!(loss < loss_over(&energies, &results, 0.001)
         && loss < loss_over(&energies, &results, 0.02),
        "fitted K is worse than a bracket endpoint — the search ran backwards");
}

// gradient() is the chain rule done by hand, which is where sign flips and dropped
// constants hide. But a derivative is only a slope, and a slope can be measured
// without any calculus: nudge one weight, see how far the loss moved. If the two
// disagree, the symbolic version is wrong.
//
// Worth the runtime because every plausible mistake here is invisible downstream —
// a stray factor, a missing 1/N, even a dropped s(1-s) all still point roughly
// downhill, so the loss falls and the run looks healthy while fitting the wrong
// function. This is the §4 counterpart to the trace equivalence test.
pub fn check_gradient(dir: &str) {
    let mut trainer = Trainer::new(Dataset::open(dir));
    let (analytic, _) = trainer.gradient();
    let range = trainer.train_range();
    let k = trainer.k();

    // Weights are centipawn-scale, so h this large is still a small relative nudge.
    // The textbook 1e-5 would put the loss difference down among the f64 rounding
    // noise of summing 1.28M terms.
    let h = 1e-2;

    // Structurally dead params have a true gradient of exactly zero and would match
    // trivially, proving nothing. Ranking by magnitude keeps the probes on params
    // the data actually exercises. TEMPO is pinned because every position touches it.
    let mut ranked: Vec<(usize, usize)> =
        (0..NUM_PARAMS).flat_map(|i| [(i, 0), (i, 1)]).collect();
    ranked.sort_by(|a, b| analytic[b.0][b.1].abs().total_cmp(&analytic[a.0][a.1].abs()));

    let mut probes = vec![(TEMPO, 0), (TEMPO, 1)];
    for &p in ranked.iter().take(10) {
        if !probes.contains(&p) {
            probes.push(p);
        }
    }

    println!("central differences at h = {h}, over {} training positions", range.len());
    println!("{:>5} {:>5} {:>16} {:>16} {:>10}", "param", "half", "analytic", "numeric", "rel err");

    let mut worst = 0f64;
    for (i, j) in probes {
        let original = trainer.weights[i][j];

        trainer.weights[i][j] = original + h;
        let up = Trainer::loss(&trainer.dataset, &trainer.weights, k, range.clone());

        trainer.weights[i][j] = original - h;
        let down = Trainer::loss(&trainer.dataset, &trainer.weights, k, range.clone());

        // Restore before the next probe, or the perturbations compound and every
        // measurement after the first is taken at the wrong point.
        trainer.weights[i][j] = original;

        // Central, not one-sided: the error is O(h²) instead of O(h), which is the
        // difference between matching to six digits and matching to two.
        let numeric = (up - down) / (2.0 * h);
        let a = analytic[i][j];

        // Relative, because gradient magnitudes span orders of magnitude across
        // params — one absolute tolerance would either pass everything or fail it.
        let rel = (a - numeric).abs() / a.abs().max(numeric.abs()).max(1e-12);
        worst = worst.max(rel);

        println!("{i:>5} {:>5} {a:>16.9} {numeric:>16.9} {rel:>10.2e}",
            if j == 0 { "mg" } else { "eg" });
    }

    println!();
    println!("worst relative error = {worst:.2e}");

    assert!(worst < 1e-4,
        "gradient disagrees with finite differences. A clean ratio between the two \
         columns names the dropped factor: 2 = differentiated squared error instead \
         of cross-entropy, N = missing the 1/N, 24 = a taper weight applied to E but \
         not to the gradient. A sign flip is r-s vs s-r.");
}

pub fn train(dir: &str, epochs: usize) {
    let mut trainer = Trainer::new(Dataset::open(dir));

    println!("K = {:.6} (frozen for the whole run)", trainer.k());
    println!("{} training / {} validation positions",
        trainer.train_range().len(), trainer.val_range().len());
    println!();
    println!("{:>6} {:>12} {:>12}", "epoch", "train", "val");

    let mut train = f64::NAN;
    for epoch in 1..=epochs {
        train = trainer.epoch();

        if epoch == 1 || epoch % 10 == 0 {
            println!("{epoch:>6} {train:>12.6} {:>12.6}", trainer.val_loss());
        }
    }

    let path = Path::new(dir).join("weights.txt");
    let header = format!(
        "K = {:.6}, {epochs} epochs, train loss {train:.6}, validation loss {:.6}",
        trainer.k(), trainer.val_loss());

    std::fs::write(&path, dump_weights(trainer.weights(), &header))
        .unwrap_or_else(|e| panic!("cannot write {}: {e}", path.display()));

    println!();
    println!("wrote {}", path.display());
}

// Coefficient is always zero for these, so no position ever produces a gradient for
// them and Adam leaves them at whatever they were seeded with. Report zero rather
// than a stale seed that reads as a tuned value.
fn is_dead(index: usize) -> bool {
    let pawn_square = index.wrapping_sub(PSQT);

    (pawn_square < 8 || (56..64).contains(&pawn_square))
        || index == PASSED_PAWN
        || index == PASSED_PAWN + 7
}

fn dump_weights(weights: &Weights, header: &str) -> String {
    let initial = initial_weights();

    let now = |i: usize, half: usize| -> i32 {
        if is_dead(i) { 0 } else { weights[i][half].round() as i32 }
    };
    let was = |i: usize, half: usize| -> i32 {
        if is_dead(i) { return 0 }
        if half == 0 { initial[i].0 } else { initial[i].1 }
    };

    let mut out = String::new();
    writeln!(out, "# Mythos tuned eval weights").unwrap();
    writeln!(out, "# {header}").unwrap();
    writeln!(out, "#").unwrap();
    writeln!(out, "# Rounded to i32. \"was\" is the seed value before tuning.").unwrap();
    writeln!(out, "# King safety is not here — it is nonlinear, so the tuner holds it frozen.").unwrap();

    let names = ["Pawn", "Knight", "Bishop", "Rook", "Queen", "King"];

    writeln!(out, "\n## Implied piece values (mean of each PSQT block)").unwrap();
    for (pt, name) in names.iter().enumerate() {
        let squares: Vec<usize> = if pt == 0 { (8..56).collect() } else { (0..64).collect() };
        let mean = |half: usize| -> f64 {
            squares.iter().map(|&sq| now(PSQT + pt * 64 + sq, half) as f64).sum::<f64>()
                / squares.len() as f64
        };
        let mean_was = |half: usize| -> f64 {
            squares.iter().map(|&sq| was(PSQT + pt * 64 + sq, half) as f64).sum::<f64>()
                / squares.len() as f64
        };

        writeln!(out, "  {name:<7} mg {:>7.1} (was {:>6.1})   eg {:>7.1} (was {:>6.1})",
            mean(0), mean_was(0), mean(1), mean_was(1)).unwrap();
    }

    writeln!(out, "\n## PSQT").unwrap();
    for (pt, name) in names.iter().enumerate() {
        for (half, tag) in ["mg", "eg"].iter().enumerate() {
            writeln!(out, "\n### {name} {tag}").unwrap();
            for rank in 0..8 {
                for file in 0..8 {
                    write!(out, "{:5},", now(PSQT + pt * 64 + rank * 8 + file, half)).unwrap();
                }
                out.push('\n');
            }
        }
    }

    writeln!(out, "\n## Scalars").unwrap();
    for (label, index) in [
        ("TEMPO", TEMPO),
        ("BISHOP_PAIR", BISHOP_PAIR),
        ("ISOLATED_PAWN", ISOLATED_PAWN),
        ("DOUBLED_PAWN", DOUBLED_PAWN),
    ] {
        writeln!(out, "  {label:<16} mg {:>6} (was {:>5})   eg {:>6} (was {:>5})",
            now(index, 0), was(index, 0), now(index, 1), was(index, 1)).unwrap();
    }


    writeln!(out, "\n## Tables").unwrap();
    for (label, base, len) in [
        ("PASSED_PAWN",    PASSED_PAWN, 8),
        ("KNIGHT_MOBILITY", KNIGHT_MOB, 9),
        ("BISHOP_MOBILITY", BISHOP_MOB, 14),
        ("ROOK_MOBILITY",     ROOK_MOB, 15),
        ("QUEEN_MOBILITY",   QUEEN_MOB, 28),
    ] {
        writeln!(out, "\n### {label}").unwrap();
        for n in 0..len {
            writeln!(out, "  [{n:>2}]  mg {:>6} (was {:>5})   eg {:>6} (was {:>5})",
                now(base + n, 0), was(base + n, 0), now(base + n, 1), was(base + n, 1)).unwrap();
        }
    }

    out
}
