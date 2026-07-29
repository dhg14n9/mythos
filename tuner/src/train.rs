use std::ops::Range;
use mythos::eval::trace::{initial_weights, NUM_PARAMS};
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
