fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("bench") | Some("searchbench") => {
            let depth = args
                .get(1)
                .and_then(|d| d.parse().ok())
                .unwrap_or(mythos::bench::BENCH_DEPTH);
            mythos::bench::search_bench(depth);
        }
        Some("perftsuite") => {
            let use_tt = args[1..]
                .iter()
                .any(|a| matches!(a.as_str(), "tt" | "--tt"));
            if !mythos::bench::run(use_tt) {
                std::process::exit(1);
            }
        }
        _ => mythos::uci::run(&args),
    }
}
