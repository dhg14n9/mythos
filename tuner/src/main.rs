pub mod data;
pub mod format;
pub mod prepare;
pub mod dataset;
pub mod train;
pub mod report;

const EPD: &str = "tuner/data/quiet-labeled.epd";
const DIR: &str = "tuner/data";

// Paths are relative to the repo root, so run from there:
//   cargo run -p tuner --release -- prepare [--limit N]
//   cargo run -p tuner --release -- train [--epochs N]
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let limit: Option<usize> = args.iter()
        .position(|a| a == "--limit")
        .map(|i| args.get(i + 1).expect("--limit needs a value"))
        .map(|n| n.parse().expect("--limit needs a number"));

    let epochs: usize = args.iter()
        .position(|a| a == "--epochs")
        .map(|i| args.get(i + 1).expect("--epochs needs a value"))
        .map(|n| n.parse().expect("--epochs needs a number"))
        .unwrap_or(200);

    match args.first().map(String::as_str) {
        Some("prepare") => prepare::prepare(EPD, DIR, limit),
        Some("verify") => dataset::verify(EPD, DIR, limit),
        Some("fit-k") => train::fit_k(DIR),
        Some("check-grad") => train::check_gradient(DIR),
        Some("train") => train::train(DIR, epochs),
        Some(other) => panic!("unknown command {other}"),
        None => panic!("No command provide")
    }
}
