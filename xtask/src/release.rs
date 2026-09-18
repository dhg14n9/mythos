use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::util::{Result, cargo, git, run, run_capture, workspace_root};

/// One binary we ship: a CPU baseline plus the rustc flags that pin it.
/// No runtime CPU dispatch -- every SIMD path is picked by `cfg(target_feature)` -- so covering testers' machines means one binary per baseline.
struct Variant {
    /// Goes in the filename: `v3` in `mythos-1.0.0-linux-x86-64-v3`.
    name: &'static str,
    target_cpu: &'static str,
    /// Extra `-C` flags, appended to RUSTFLAGS after the target-cpu.
    extra: &'static [&'static str],
}

const VARIANTS: &[Variant] = &[
    Variant { name: "v1", target_cpu: "x86-64",    extra: &[] },
    Variant { name: "v2", target_cpu: "x86-64-v2", extra: &[] },
    Variant { name: "v3", target_cpu: "x86-64-v3", extra: &[] },
];

// x86-64-v4 is deliberately absent: nothing uses AVX-512, and most dev machines cannot verify a v4 binary.

/// v3 with the PEXT movegen path compiled out: `-C target-cpu=x86-64-v3` implies BMI2, and lookup.rs then uses
/// `_pext_u64`, which is microcoded on Zen 1/2 (~18 cycles) and can land slower than v2. Keeps AVX2.
const V3_NOPEXT: Variant = Variant {
    name: "v3-nopext",
    target_cpu: "x86-64-v3",
    extra: &["-C", "target-feature=-bmi2"],
};

struct Platform {
    name: &'static str,
    /// `None` builds for the host; `Some` passes `--target`.
    triple: Option<&'static str>,
    suffix: &'static str,
}

const PLATFORMS: &[Platform] = &[
    Platform { name: "linux",   triple: None,                          suffix: ""     },
    Platform { name: "windows", triple: Some("x86_64-pc-windows-gnu"), suffix: ".exe" },
];

const OUT_DIR: &str = "target/release-artifacts";
const BUILD_DIR: &str = "target/release-build";

struct Artifact {
    path: PathBuf,
    variant: &'static str,
    platform: &'static str,
    /// Only filled in for host binaries — we cannot run the .exe files here.
    bench: Option<(u64, u64)>,
}

pub fn release(nopext: bool) -> Result<()> {
    let root = workspace_root();
    let version = package_version()?;

    let describe = run_capture(git().args(["describe", "--tags", "--always"]))
        .unwrap_or_else(|_| "unknown".into());
    println!("[release] mythos {version} at {describe}");

    // Untracked files can't end up in a tag, so --untracked-files=no only warns about what would change the binary.
    let dirty = !run_capture(git().args(["status", "--porcelain", "--untracked-files=no"]))?
        .is_empty();
    if dirty {
        println!("[release] WARNING: tracked files are modified — these binaries will not");
        println!("[release]          match anything you can check out by tag or sha.");
    }

    let mut variants: Vec<&Variant> = VARIANTS.iter().collect();
    if nopext {
        variants.push(&V3_NOPEXT);
    }

    let out = root.join(OUT_DIR);
    fs::create_dir_all(&out).map_err(|e| format!("failed to create {OUT_DIR}: {e}"))?;

    let mut artifacts = Vec::new();
    for platform in PLATFORMS {
        for variant in &variants {
            artifacts.push(build(variant, platform, &version, root, &out)?);
        }
    }

    verify(&mut artifacts, &version)?;
    summarize(&artifacts, &out);
    Ok(())
}

fn build(
    variant: &Variant,
    platform: &Platform,
    version: &str,
    root: &Path,
    out: &Path,
) -> Result<Artifact> {
    let label = format!("{}-{}", platform.name, variant.name);
    println!("[release] building {label}");

    // An env RUSTFLAGS *replaces* [build] rustflags in .cargo/config.toml, which is what we want: the repo default is target-cpu=native.
    let mut rustflags = format!("-C target-cpu={}", variant.target_cpu);
    for flag in variant.extra {
        rustflags.push(' ');
        rustflags.push_str(flag);
    }

    // Cargo does not key its build cache on RUSTFLAGS, so variants sharing one target dir would overwrite each other.
    let target_dir = root.join(BUILD_DIR).join(&label);

    let mut cmd = cargo();
    cmd.env("RUSTFLAGS", &rustflags)
        .args(["build", "--release", "--bin", "mythos"])
        .arg("--target-dir")
        .arg(&target_dir);
    if let Some(triple) = platform.triple {
        cmd.args(["--target", triple]);
    }
    run(&mut cmd).map_err(|e| match platform.triple {
        Some(triple) => format!(
            "{e}\n\nCross-compiling to {triple} needs the target and a linker:\n  \
             rustup target add {triple}\n  \
             and x86_64-w64-mingw32-gcc on PATH (package `mingw-w64-gcc`)"
        ),
        None => e,
    })?;

    let built = match platform.triple {
        Some(triple) => target_dir.join(triple).join("release"),
        None => target_dir.join("release"),
    }
    .join(format!("mythos{}", platform.suffix));

    let name = format!(
        "mythos-{version}-{}-x86-64-{}{}",
        platform.name, variant.name, platform.suffix
    );
    let dest = out.join(&name);
    fs::copy(&built, &dest)
        .map_err(|e| format!("failed to copy {} to {}: {e}", built.display(), dest.display()))?;

    Ok(Artifact {
        path: dest,
        variant: variant.name,
        platform: platform.name,
        bench: None,
    })
}

fn verify(artifacts: &mut [Artifact], version: &str) -> Result<()> {
    println!("[release] verifying");

    for art in artifacts.iter_mut() {
        if art.platform == "windows" {
            check_windows_imports(&art.path)?;
            continue;
        }
        check_version(&art.path, version)?;
        art.bench = Some(bench(&art.path)?);
    }

    // The node count is a functional fingerprint: identical counts across baselines prove the PEXT and fallback movegen
    // agree, and that the AVX2 and scalar forward passes do. A mismatch must not ship.
    let mut counts: Vec<(&str, u64)> = artifacts
        .iter()
        .filter_map(|a| a.bench.map(|(nodes, _)| (a.variant, nodes)))
        .collect();
    counts.dedup_by_key(|(_, nodes)| *nodes);
    if counts.len() > 1 {
        let detail: Vec<String> = counts
            .iter()
            .map(|(name, nodes)| format!("{name}: {nodes}"))
            .collect();
        return Err(format!(
            "bench node counts differ across baselines ({}) — a cfg-gated path \
             disagrees with the others; do not ship",
            detail.join(", ")
        ));
    }

    Ok(())
}

fn check_version(bin: &Path, version: &str) -> Result<()> {
    // `mythos uci` runs `uci` before falling through to the stdin loop, and a null stdin ends that loop immediately.
    let out = run_capture(
        Command::new(bin)
            .arg("uci")
            .stdin(Stdio::null())
            .current_dir(workspace_root()),
    )?;

    let expected = format!("id name Mythos {version}");
    if !out.lines().any(|l| l.trim() == expected) {
        let got = out
            .lines()
            .find(|l| l.starts_with("id name"))
            .unwrap_or("<no id name line>");
        return Err(format!(
            "{}: expected `{expected}`, got `{got}`",
            bin.display()
        ));
    }
    Ok(())
}

fn bench(bin: &Path) -> Result<(u64, u64)> {
    let out = run_capture(
        Command::new(bin)
            .arg("bench")
            .stdin(Stdio::null())
            .current_dir(workspace_root()),
    )?;
    parse_bench(&out).ok_or_else(|| {
        format!("{}: no `<nodes> nodes <nps> nps` line in bench output", bin.display())
    })
}

fn parse_bench(output: &str) -> Option<(u64, u64)> {
    // The summary line is last and reads `<nodes> nodes <nps> nps`.
    for line in output.lines().rev() {
        let toks: Vec<&str> = line.split_whitespace().collect();
        let (Some(n), Some(s)) = (
            toks.iter().position(|t| *t == "nodes"),
            toks.iter().position(|t| *t == "nps"),
        ) else {
            continue;
        };
        if n == 0 || s == 0 {
            continue;
        }
        let (Ok(nodes), Ok(nps)) = (toks[n - 1].parse(), toks[s - 1].parse()) else {
            continue;
        };
        return Some((nodes, nps));
    }
    None
}

/// A mingw build linking libgcc or libwinpthread dynamically greets the tester with a missing-DLL dialog.
fn check_windows_imports(exe: &Path) -> Result<()> {
    const OBJDUMP: &str = "x86_64-w64-mingw32-objdump";

    let out = match Command::new(OBJDUMP).arg("-p").arg(exe).output() {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).into_owned(),
        // Missing toolchain is not a build failure; say so rather than failing.
        _ => {
            println!(
                "[release] WARNING: {OBJDUMP} unavailable — could not check {} for stray DLLs",
                exe.display()
            );
            return Ok(());
        }
    };

    let stray: Vec<&str> = out
        .lines()
        .filter_map(|l| l.trim().strip_prefix("DLL Name:"))
        .map(str::trim)
        .filter(|dll| {
            let lower = dll.to_ascii_lowercase();
            !(lower.starts_with("api-ms-win-")
                || lower == "kernel32.dll"
                || lower == "ntdll.dll")
        })
        .collect();

    if !stray.is_empty() {
        return Err(format!(
            "{}: links DLLs a tester may not have: {}",
            exe.display(),
            stray.join(", ")
        ));
    }
    Ok(())
}

fn summarize(artifacts: &[Artifact], out: &Path) {
    println!();
    println!("{}", out.display());
    for art in artifacts {
        let name = art
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        match art.bench {
            Some((nodes, nps)) => println!("  {name:<40}  {nodes} nodes  {nps} nps"),
            None => println!("  {name:<40}  (not runnable here)"),
        }
    }
    println!();
    println!("Upload these with:");
    println!("  gh release create <tag> {}/* --title ... --notes-file ...", out.display());
}

fn package_version() -> Result<String> {
    let out = run_capture(cargo().args(["metadata", "--format-version", "1", "--no-deps"]))?;
    let meta: serde_json::Value =
        serde_json::from_str(&out).map_err(|e| format!("failed to parse cargo metadata: {e}"))?;
    meta["packages"]
        .as_array()
        .and_then(|pkgs| pkgs.iter().find(|p| p["name"] == "mythos"))
        .and_then(|pkg| pkg["version"].as_str())
        .map(str::to_string)
        .ok_or_else(|| "no `mythos` package in cargo metadata".into())
}
