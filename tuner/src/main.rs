pub mod data;
pub mod format;
pub mod prepare;
pub mod dataset;
pub mod train;
pub mod report;
pub mod emit;

const EPD: &str = "tuner/data/quiet-labeled.epd";
const DIR: &str = "tuner/data";

// Paths are relative to the repo root, so run from there:
//   cargo run -p tuner --release -- prepare [--limit N]
//   cargo run -p tuner --release -- train [--epochs N]
//   cargo run -p tuner --release -- emit [--dry-run | --check]
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let limit: Option<usize> = args.iter()
        .position(|a| a == "--limit")
        .map(|i| args.get(i + 1).expect("--limit needs a value"))
        .map(|n| n.parse().expect("--limit needs a number"));

    // A safety cap, not a run length — the lr schedule ends the run when the weights
    // stop moving. High enough that the schedule, not this number, is what stops it.
    let epochs: usize = args.iter()
        .position(|a| a == "--epochs")
        .map(|i| args.get(i + 1).expect("--epochs needs a value"))
        .map(|n| n.parse().expect("--epochs needs a number"))
        .unwrap_or(3000);

    match args.first().map(String::as_str) {
        Some("prepare") => prepare::prepare(EPD, DIR, limit),
        Some("verify") => dataset::verify(EPD, DIR, limit),
        Some("fit-k") => train::fit_k(DIR),
        Some("check-grad") => train::check_gradient(DIR),
        Some("train") => train::train(DIR, epochs),
        Some("emit") if args.iter().any(|a| a == "--check") => emit::check(DIR),
        Some("emit") => emit::emit(DIR, args.iter().any(|a| a == "--dry-run")),
        Some(other) => panic!("unknown command {other}"),
        None => panic!("No command provide")
    }
}
