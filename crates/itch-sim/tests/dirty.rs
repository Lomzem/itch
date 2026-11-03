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

#[test]
fn rebuilding_after_a_tracked_root_edit_updates_the_dirty_flag() {
    let repo = Repo::committed("rebuild");
    let app = repo.0.join("crates/app");
    std::fs::create_dir_all(app.join("src")).unwrap();
    std::fs::create_dir(repo.0.join("build")).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../build/dirty.rs"),
        repo.0.join("build/dirty.rs"),
    )
    .unwrap();
    std::fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("build.rs"), app.join("build.rs"))
        .unwrap();
    for (path, contents) in [
        ("Cargo.toml", "[workspace]\nresolver = '3'\nmembers = ['crates/app']\n"),
        (
            "crates/app/Cargo.toml",
            "[package]\nname = 'dirty-fixture'\nversion = '0.1.0'\nedition = '2024'\n",
        ),
        ("crates/app/src/main.rs", "fn main() { println!(\"{}\", env!(\"ITCH_DIRTY\")); }\n"),
        (".gitignore", "/target/\n"),
        ("README.md", "clean\n"),
    ] {
        std::fs::write(repo.0.join(path), contents).unwrap();
    }
    let cargo = |args: &[&str]| {
        let output = Command::new(env!("CARGO"))
            .args(args)
            .current_dir(&repo.0)
            .env("CARGO_TARGET_DIR", repo.0.join("target"))
            // A read-only git status must not refresh the index and
            // incidentally invalidate Cargo's cache for the root edit.
            .env("GIT_OPTIONAL_LOCKS", "0")
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8(output.stdout).unwrap()
    };
    cargo(&["generate-lockfile", "--offline"]);
    git(&repo.0, &["add", "."]);
    git(
        &repo.0,
        &["-c", "user.name=t", "-c", "user.email=t@example.com", "commit", "-qm", "fixture"],
    );
    assert_eq!(cargo(&["run", "--quiet", "--offline"]).trim(), "false");
    std::fs::write(repo.0.join("README.md"), "changed\n").unwrap();
    assert_eq!(cargo(&["run", "--quiet", "--offline"]).trim(), "true");
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
