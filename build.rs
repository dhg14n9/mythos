fn main() {
    let path = std::env::var("EVALFILE")
        .ok().filter(|x| !x.is_empty())
        .unwrap_or_else(|| "nets/net.nnue".to_string());
    let root = std::env::var("CARGO_MANIFEST_DIR").unwrap();

    let mut nnue_path = std::path::PathBuf::new();
    nnue_path.push(root);
    nnue_path.push(path);

    if !nnue_path.is_file() {
        panic!("NNUE path does not exist: {}", nnue_path.display());
    }

    println!("cargo:rustc-env=MYTHOS_NET={}", nnue_path.display());
    println!("cargo:rerun-if-env-changed=EVALFILE");
    println!("cargo:rerun-if-changed={}", nnue_path.display())

}