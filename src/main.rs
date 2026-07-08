fn main() {
    // `mythos bench [tt]` runs the perft bench suite and exits (non-zero on
    // any count mismatch, so it can gate CI); `tt` enables the transposition
    // table. No arguments starts the UCI loop.
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("bench") => {
            let use_tt = args[1..].iter().any(|a| matches!(a.as_str(), "tt" | "--tt"));
            if !mythos::bench::run(use_tt) {
                std::process::exit(1);
            }
        }
        Some(arg) => {
            eprintln!("unknown argument: {arg} (expected `bench [tt]` or no arguments for UCI)");
            std::process::exit(2);
        }
        None => mythos::uci::run(),
    }
}