use std::path::Path;
use std::process::Command;

const NET_URL: &str = "https://github.com/dhg14n9/mythos-nn/releases/download/nets";

fn main() {
    let root = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let evalfile = std::env::var("EVALFILE").ok().filter(|x| !x.is_empty());

    let nnue_path = match evalfile {
        Some(file) => Path::new(&root).join(file),
        None => {
            let name = std::fs::read_to_string(Path::new(&root).join("network.txt"))
                .expect("failed to read network.txt")
                .trim()
                .to_string();
            let path = Path::new(&root).join("nets").join(&name);

            if !path.is_file() {
                download(&name, &path);
            }

            path
        }
    };

    if !nnue_path.is_file() {
        panic!("NNUE file not found: {}", nnue_path.display());
    }

    println!("cargo:rustc-env=MYTHOS_NET={}", nnue_path.display());
    println!("cargo:rerun-if-env-changed=EVALFILE");
    println!("cargo:rerun-if-changed={}", nnue_path.display());
    println!("cargo:rerun-if-changed=network.txt");
}

fn download(name: &str, path: &Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let part = path.with_extension(format!("part{}", std::process::id()));

    let status = Command::new("curl")
        .arg("-sSfL")
        .arg(format!("{NET_URL}/{name}"))
        .arg("-o")
        .arg(&part)
        .status()
        .expect("failed to run curl");

    if !status.success() {
        let _ = std::fs::remove_file(&part);
        panic!("failed to download {name} from {NET_URL}");
    }

    if std::fs::rename(&part, path).is_err() && !path.is_file() {
        panic!("failed to move {} into place", part.display());
    }
    let _ = std::fs::remove_file(&part);
}
