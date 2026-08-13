use std::path::PathBuf;
use std::process::Command;

use crate::util::{Result, cargo, run, run_capture, workspace_root};

/// The report page, with a `__PAYLOAD__` placeholder where the run JSON goes.
const TEMPLATE: &str = include_str!("../assets/selfplay.html");

pub struct SelfplayConfig {
    pub games: String,
    pub tc: String,
    pub hash: String,
    pub seed: String,
    /// Continuation cells kept per engine in the report ("0" keeps every cell).
    pub top: String,
    pub opening: String,
    /// Run folder name; defaults to a timestamp.
    pub name: Option<String>,
    pub verbose: bool,
}

impl Default for SelfplayConfig {
    fn default() -> Self {
        Self {
            games: "4".into(),
            tc: "8+0.08".into(),
            hash: "16".into(),
            seed: "1".into(),
            top: "20000".into(),
            opening: "8".into(),
            name: None,
            verbose: false,
        }
    }
}

/// Play N self-play games at a time control, then render the HTML report.
pub fn selfplay(cfg: &SelfplayConfig) -> Result<()> {
    let root = workspace_root();

    println!("[selfplay] building the harness (release, +stats)...");
    run(cargo().args([
        "build",
        "--release",
        "--features",
        "stats",
        "--example",
        "selfplay",
    ]))?;

    let stamp = run_capture(Command::new("date").arg("+%Y%m%d-%H%M%S")).unwrap_or_else(|_| "run".into());
    let name = cfg
        .name
        .clone()
        .unwrap_or_else(|| format!("{stamp}-{}g-{}", cfg.games, cfg.tc.replace('+', "i")));
    let run_dir = root.join("target/selfplay/runs").join(&name);
    std::fs::create_dir_all(&run_dir).map_err(|e| format!("cannot create {}: {e}", run_dir.display()))?;

    let json_path = run_dir.join("run.json");
    let bin = root.join("target/release/examples/selfplay");
    if !bin.is_file() {
        return Err(format!("harness binary not found at {}", bin.display()));
    }

    let mut cmd = Command::new(&bin);
    cmd.current_dir(root)
        .args(["--games", &cfg.games])
        .args(["--tc", &cfg.tc])
        .args(["--hash", &cfg.hash])
        .args(["--seed", &cfg.seed])
        .args(["--top", &cfg.top])
        .args(["--opening", &cfg.opening])
        .arg("--out")
        .arg(&json_path);
    if cfg.verbose {
        cmd.arg("--verbose");
    }
    run(&mut cmd)?;

    let report = render(&json_path)?;
    println!("[selfplay] report: {}", report.display());
    println!("[selfplay] open it with: xdg-open {}", report.display());
    Ok(())
}

/// Splice a run's JSON into the template. Kept separate so an existing run can be
/// re-rendered after a template change without replaying the games.
pub fn render(json_path: &std::path::Path) -> Result<PathBuf> {
    let payload = std::fs::read_to_string(json_path)
        .map_err(|e| format!("cannot read {}: {e}", json_path.display()))?;

    // The payload is JSON inside a <script>, so the only sequence that could end
    // the block early is a literal "</script>"; engine output cannot contain one,
    // but escape the slash anyway rather than trust that.
    let payload = payload.replace("</", "<\\/");

    let html = TEMPLATE.replace("__PAYLOAD__", payload.trim());
    let out = json_path.with_file_name("report.html");
    std::fs::write(&out, &html).map_err(|e| format!("cannot write {}: {e}", out.display()))?;

    println!(
        "[selfplay] {} ({:.2} MB)",
        out.display(),
        html.len() as f64 / 1e6
    );
    Ok(out)
}

/// `selfplay-report RUN` — RUN is a run folder, a run name, or a run.json path.
pub fn render_cmd(arg: &str) -> Result<()> {
    let root = workspace_root();
    let given = PathBuf::from(arg);

    let json = if given.is_file() {
        given
    } else {
        let dir = if given.is_dir() { given } else { root.join("target/selfplay/runs").join(arg) };
        let json = dir.join("run.json");
        if !json.is_file() {
            return Err(format!("no run.json in {}", dir.display()));
        }
        json
    };

    render(&json).map(|_| ())
}
