// The `dirty` flag `Hello` reports, shared by the firmware's and the sim's
// build scripts through `include!`.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Whether the git checkout holding `dir` has uncommitted changes. No git,
/// or no checkout, counts as clean.
pub fn dirty(dir: &Path) -> bool {
    git(dir, &["status", "--porcelain"]).is_some_and(|status| !status.is_empty())
}

/// Sets `ITCH_DIRTY` to `true` or `false` for the crate being built. Reruns
/// when git's index or HEAD moves, or any crate's files change.
pub fn emit_dirty() {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("set by cargo"));
    println!("cargo:rustc-env=ITCH_DIRTY={}", dirty(&manifest));
    println!("cargo:rerun-if-changed=..");
    if let Some(git_dir) = git(&manifest, &["rev-parse", "--absolute-git-dir"]) {
        let git_dir = Path::new(git_dir.trim());
        println!("cargo:rerun-if-changed={}", git_dir.join("index").display());
        println!("cargo:rerun-if-changed={}", git_dir.join("HEAD").display());
    }
}

/// What `git args` prints in `dir`, or `None` if git is missing or fails.
fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).current_dir(dir).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}
