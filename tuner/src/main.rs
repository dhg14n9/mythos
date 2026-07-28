use mythos::board::board::Board;

pub mod data;
pub mod format;
pub mod prepare;

const EPD: &str = "tuner/data/quiet-labeled.epd";
const DIR: &str = "tuner/data";

// Paths are relative to the repo root, so run from there:
//   cargo run -p tuner --release -- prepare [--limit N]
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let limit: Option<usize> = args.iter()
        .position(|a| a == "--limit")
        .map(|i| args.get(i + 1).expect("--limit needs a value"))
        .map(|n| n.parse().expect("--limit needs a number"));

    match args.first().map(String::as_str) {
        Some("prepare") => prepare::prepare(EPD, DIR, limit),
        Some("stats") | None => stats(limit),
        Some(other) => panic!("unknown command {other}"),
    }
}

// The original label-split check. Superseded once the read-back check recomputes
// the split from entries.rec instead.
fn stats(limit: Option<usize>) {
    let mut win = 0;
    let mut draw = 0;
    let mut lose = 0;

    let temp = |_board: &Board, score: f32| {
        if score == 1.0 {
            win += 1;
        }
        else if score == 0.5 {
            draw += 1;
        }
        else {
            lose += 1;
        }
    };

    data::parse_data(EPD, limit, temp);

    print!("win: {win}\nlose: {lose}\ndraw: {draw}\n")
}