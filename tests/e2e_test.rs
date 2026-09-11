use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::tempdir;

fn boxr_bin() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("target");
    path.push(if cfg!(debug_assertions) { "debug" } else { "release" });
    path.push("boxr");
    if !path.exists() {
        // Fallback to release or debug if other was built
        let alt = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(if cfg!(debug_assertions) { "release" } else { "debug" })
            .join("boxr");
        if alt.exists() {
            return alt;
        }
    }
    path
}

#[test]
fn test_e2e_cli_version_and_help() {
    let bin = boxr_bin();
    if !bin.exists() {
        return;
    }

    let output = Command::new(&bin).arg("--version").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("boxr 0.1.0"));

    let output = Command::new(&bin).arg("--help").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Usage: boxr <COMMAND>"));
}

#[test]
fn test_e2e_volume_lifecycle() {
    let bin = boxr_bin();
    if !bin.exists() {
        return;
    }

    let vol_name = "e2e-vol-test";

    // Create
    let output = Command::new(&bin).args(["volume", "create", vol_name]).output().unwrap();
    assert!(output.status.success());

    // List
    let output = Command::new(&bin).args(["volume", "ls"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(vol_name));

    // Inspect
    let output = Command::new(&bin).args(["volume", "inspect", vol_name]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(vol_name));

    // Remove
    let output = Command::new(&bin).args(["volume", "rm", vol_name]).output().unwrap();
    assert!(output.status.success());
}

#[test]
fn test_e2e_network_lifecycle() {
    let bin = boxr_bin();
    if !bin.exists() {
        return;
    }

    let net_name = "e2e-net-test";

    // Create
    let output = Command::new(&bin).args(["network", "create", net_name]).output().unwrap();
    assert!(output.status.success());

    // List
    let output = Command::new(&bin).args(["network", "ls"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(net_name));

    // Inspect
    let output = Command::new(&bin).args(["network", "inspect", net_name]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(net_name));

    // Remove
    let output = Command::new(&bin).args(["network", "rm", net_name]).output().unwrap();
    assert!(output.status.success());
}

#[test]
fn test_e2e_system_df_and_completions() {
    let bin = boxr_bin();
    if !bin.exists() {
        return;
    }

    // System df
    let output = Command::new(&bin).args(["system", "df"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Images"));
    assert!(stdout.contains("Containers"));

    // Shell completions
    for shell in ["bash", "zsh", "fish"] {
        let output = Command::new(&bin).args(["completion", shell]).output().unwrap();
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(!stdout.is_empty());
    }

    // Alias eval
    let output = Command::new(&bin).args(["alias"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("alias docker=\"boxr\""));
}

#[test]
fn test_e2e_dockerfile_multi_stage_build() {
    let bin = boxr_bin();
    if !bin.exists() {
        return;
    }

    let temp = tempdir().unwrap();
    let dockerfile = r#"
FROM alpine:latest AS stage1
WORKDIR /build
RUN echo "compiled artifact" > /build/app.bin

FROM alpine:latest
WORKDIR /app
COPY --from=stage1 /build/app.bin /app/app.bin
CMD ["/bin/cat", "/app/app.bin"]
"#;

    fs::write(temp.path().join("Dockerfile"), dockerfile).unwrap();
    fs::write(temp.path().join(".dockerignore"), "*.tmp\n").unwrap();

    let tag = "e2e-multistage:v1";

    let output = Command::new(&bin)
        .args(["build", "-t", tag, temp.path().to_str().unwrap()])
        .output()
        .unwrap();

    assert!(output.status.success());

    // Verify image in images list
    let output = Command::new(&bin).args(["images"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("e2e-multistage"));

    // Clean up
    let _ = Command::new(&bin).args(["rmi", tag]).output();
}
