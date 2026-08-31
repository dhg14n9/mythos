fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("bench") | Some("searchbench") => {
            let (depth, hash) = mythos::bench::parse_bench_args(&args[1..]);
            mythos::bench::search_bench(depth, hash);
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
