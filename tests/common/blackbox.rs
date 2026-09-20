//! Shared helpers for black-box integration tests (isolated BOXR_HOME, CLI runners).

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant};
use tempfile::TempDir;

/// Returns path to the boxr binary under `target/{debug|release}/`.
pub fn boxr_bin() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("target");
    path.push(if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    });
    path.push("boxr");
    if !path.exists() {
        let alt = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(if cfg!(debug_assertions) {
                "release"
            } else {
                "debug"
            })
            .join("boxr");
        if alt.exists() {
            return alt;
        }
    }
    path
}

/// Create an isolated BOXR_HOME that symlinks heavy dirs from the real home.
pub fn isolated_home() -> (TempDir, PathBuf) {
    let temp = tempfile::tempdir().expect("failed to create temp BOXR_HOME");
    let home = temp.path().to_path_buf();
    let base_home = boxr::storage::boxr_home();
    for dir_name in &["images", "layers", "vm", "bin"] {
        let src = base_home.join(dir_name);
        if src.exists() {
            let dst = home.join(dir_name);
            #[cfg(unix)]
            let _ = std::os::unix::fs::symlink(&src, &dst);
            #[cfg(windows)]
            let _ = std::os::windows::fs::symlink_dir(&src, &dst);
        }
    }
    let src_images_json = base_home.join("images.json");
    if src_images_json.exists() {
        let _ = fs::copy(&src_images_json, home.join("images.json"));
    }
    (temp, home)
}

pub fn rand_suffix() -> String {
    hex::encode(boxr::storage::container_store::rand_id())[..8].to_string()
}

pub fn run_boxr(home: &Path, args: &[&str]) -> Output {
    let bin = boxr_bin();
    assert!(bin.exists(), "boxr binary not found at {}", bin.display());
    let mut cmd = Command::new(&bin);
    cmd.env_remove("DOCKER_HOST");
    cmd.env("BOXR_HOME", home);
    cmd.args(args).output().expect("failed to execute boxr")
}

pub fn run_boxr_ok(home: &Path, args: &[&str]) -> String {
    let out = run_boxr(home, args);
    assert!(
        out.status.success(),
        "boxr {} failed (exit {}): stderr={} stdout={}",
        args.join(" "),
        out.status,
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

pub fn run_boxr_fail(home: &Path, args: &[&str]) {
    let out = run_boxr(home, args);
    assert!(
        !out.status.success(),
        "expected boxr {} to fail, got success: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stdout)
    );
}

pub fn combined_output(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

pub fn skip_on_macos() -> bool {
    cfg!(target_os = "macos")
}

pub fn skip_on_linux() -> bool {
    cfg!(target_os = "linux")
}

pub fn fixture_path(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel)
}

pub fn wait_container_running(home: &Path, name: &str, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        let out = run_boxr(home, &["inspect", "-f", "{{.State.Running}}", name]);
        if out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout);
            if stdout.trim() == "true" {
                return true;
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    false
}

pub fn wait_exec_success(home: &Path, container: &str, cmd: &[&str], timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        let mut args = vec!["exec", container];
        args.extend_from_slice(cmd);
        let out = run_boxr(home, &args);
        if out.status.success() {
            return true;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    false
}

pub fn cleanup_container(home: &Path, name: &str) {
    let _ = run_boxr(home, &["rm", "-f", name]);
}

pub fn cleanup_volume(home: &Path, name: &str) {
    let _ = run_boxr(home, &["volume", "rm", "-f", name]);
}

pub fn cleanup_network(home: &Path, name: &str) {
    let _ = run_boxr(home, &["network", "rm", name]);
}

pub fn inspect_json(home: &Path, id_or_name: &str) -> serde_json::Value {
    let out = run_boxr_ok(home, &["inspect", id_or_name]);
    let arr: Vec<serde_json::Value> =
        serde_json::from_str(&out).expect("invalid inspect JSON");
    arr.into_iter().next().expect("empty inspect result")
}

pub fn pull_if_needed(home: &Path, image: &str) {
    let out = run_boxr(home, &["images"]);
    let listing = String::from_utf8_lossy(&out.stdout);
    let repo = image.split(':').next().unwrap_or(image);
    let tag = image.split(':').nth(1).unwrap_or("latest");
    let needle = format!("{}/{}", repo.replace('/', " "), tag);
    let alt_needle = format!("{} {}", repo, tag);
    if !listing.contains(&needle) && !listing.contains(&alt_needle) && !listing.contains(repo) {
        run_boxr_ok(home, &["pull", image]);
    }
}
