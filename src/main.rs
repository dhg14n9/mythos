fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("bench") => {
            let use_tt = args[1..]
                .iter()
                .any(|a| matches!(a.as_str(), "tt" | "--tt"));
            if !mythos::bench::run(use_tt) {
                std::process::exit(1);
            }
        }
        Some("searchbench") => {
            let depth = args
                .get(1)
                .and_then(|d| d.parse().ok())
                .unwrap_or(7);
            mythos::bench::search_bench(depth);
        }
        Some(arg) => {
            eprintln!(
                "unknown argument: {arg} (expected `bench [tt]`, `searchbench [depth]` or no arguments for UCI)"
            );
            std::process::exit(2);
        }
        None => mythos::uci::run(),
    }
}
