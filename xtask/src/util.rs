use std::path::{Path, PathBuf};
use std::process::Command;

pub type Result<T> = std::result::Result<T, String>;

pub const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// xtask lives one level below the workspace root.
pub fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask manifest dir has a parent")
}

pub fn cargo() -> Command {
    let mut cmd = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    cmd.current_dir(workspace_root());
    cmd
}

/// Removes a temporary git worktree when dropped, so it is cleaned up
/// on the error path (including Ctrl-C, which unwinds via a failed wait).
pub struct WorktreeGuard {
    pub dir: PathBuf,
}

impl Drop for WorktreeGuard {
    fn drop(&mut self) {
        let _ = git()
            .args(["worktree", "remove", "--force"])
            .arg(&self.dir)
            .status();
    }
}

pub fn git() -> Command {
    let mut cmd = Command::new("git");
    cmd.current_dir(workspace_root());
    cmd
}

/// Run a command to completion, inheriting stdio; non-zero exit becomes Err.
pub fn run(cmd: &mut Command) -> Result<()> {
    let program = cmd.get_program().to_string_lossy().into_owned();
    let status = cmd
        .status()
        .map_err(|e| format!("failed to run {program}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} exited with {status}"))
    }
}

/// Run a command and capture stdout as a trimmed string; non-zero exit becomes Err.
pub fn run_capture(cmd: &mut Command) -> Result<String> {
    let program = cmd.get_program().to_string_lossy().into_owned();
    let out = cmd
        .output()
        .map_err(|e| format!("failed to run {program}: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(format!(
            "{program} exited with {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}
