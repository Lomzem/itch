use std::path::{Path, PathBuf};
use std::process::Command;

#[allow(dead_code)]
mod build {
    include!("../../../build/dirty.rs");
}

#[test]
fn a_clean_checkout_is_not_dirty() {
    let repo = Repo::committed("clean");
    assert!(!build::dirty(&repo.0));
}

#[test]
fn an_uncommitted_change_is_dirty() {
    let repo = Repo::committed("changed");
    std::fs::write(repo.0.join("main.rs"), "fn main() { todo!() }").unwrap();
    assert!(build::dirty(&repo.0));
}

#[test]
fn a_directory_outside_any_checkout_is_not_dirty() {
    let dir = scratch("outside");
    assert!(!build::dirty(&dir));
}

/// A throwaway git checkout with one committed file.
struct Repo(PathBuf);

impl Repo {
    fn committed(name: &str) -> Self {
        let dir = scratch(name);
        std::fs::write(dir.join("main.rs"), "fn main() {}").unwrap();
        git(&dir, &["init", "-q"]);
        git(&dir, &["add", "."]);
        git(&dir, &["-c", "user.name=t", "-c", "user.email=t@example.com", "commit", "-qm", "c"]);
        Self(dir)
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// An empty directory of its own, outside this checkout.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("itch-dirty-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git").args(args).current_dir(dir).status().unwrap();
    assert!(status.success(), "git {args:?}");
}
