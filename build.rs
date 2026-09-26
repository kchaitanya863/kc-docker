// Captures the rustc toolchain version at compile time so `boxr version`
// can report it honestly (instead of aping Docker's "Go version" field).
fn main() {
    let rustc_version = std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "rustc unknown".to_string());
    println!("cargo:rustc-env=RUSTC_VERSION={}", rustc_version);
    println!("cargo:rerun-if-changed=build.rs");
}
