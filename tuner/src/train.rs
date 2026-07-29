use std::ops::Range;
use mythos::eval::trace::{initial_weights, NUM_PARAMS, TEMPO};
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
    split: usize
}

impl Trainer {
    pub fn new(dataset: Dataset) -> Self {
        let split = dataset.len() * 9 / 10;
        let mut trainer = Self {
            dataset, weights: from_initial(), k: 0.0, split
        };
        trainer.k = trainer.fit_k();

        trainer
    }

    pub fn k(&self) -> f64 {
        self.k
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
        todo!()
    }

    fn apply_optimizer(&mut self, gradients: Weights) {
        todo!()
    }

    fn gradient(&self) -> Weights {
        let mut gradient: Weights = [[0.0; 2]; NUM_PARAMS];
        let len = (self.train_range().end - self.train_range().start) as f64;

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

            let g = self.k() * (s - r);

            for packed in coeffs {
                let (index, val) = unpack(*packed);
                gradient[index][0] += g * mg_weight * (val as f64) / len;
                gradient[index][1] += g * eg_weight * (val as f64) / len;
            }
        }

        gradient
    }
}

pub struct Adam {
    m: Weights,
    v: Weights,
    t: u64,
    lr: f64
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
    let analytic = trainer.gradient();
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
