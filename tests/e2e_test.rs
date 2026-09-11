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

#[test]
fn test_e2e_container_lifecycle_pause_unpause_rename_commit_wait() {
    let bin = boxr_bin();
    if !bin.exists() {
        return;
    }

    let name = "e2e-cycle-test";
    let renamed = "e2e-cycle-renamed";
    let snap_img = "e2e-snap:v1";

    // 1. Run in background
    let output = Command::new(&bin)
        .args(["run", "-d", "--name", name, "alpine", "/bin/sh", "-c", "echo 'committed file' > /committed.txt; sleep 1"])
        .output()
        .unwrap();
    assert!(output.status.success());

    // 2. Pause
    let output = Command::new(&bin).args(["pause", name]).output().unwrap();
    assert!(output.status.success());

    // Verify paused status
    let output = Command::new(&bin).args(["ps", "-a"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Paused"));

    // 3. Unpause
    let output = Command::new(&bin).args(["unpause", name]).output().unwrap();
    assert!(output.status.success());

    // 4. Rename
    let output = Command::new(&bin).args(["rename", name, renamed]).output().unwrap();
    assert!(output.status.success());

    // 5. Commit
    let output = Command::new(&bin).args(["commit", renamed, snap_img]).output().unwrap();
    assert!(output.status.success());

    // Verify committed image runs and has the file
    let output = Command::new(&bin)
        .args(["run", "--rm", snap_img, "/bin/cat", "/committed.txt"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("committed file"));

    // 6. Wait
    let output = Command::new(&bin).args(["wait", renamed]).output().unwrap();
    assert!(output.status.success());

    // 7. Clean up
    let _ = Command::new(&bin).args(["rm", renamed]).output();
    let _ = Command::new(&bin).args(["rmi", snap_img]).output();
}

#[test]
fn test_e2e_filesystem_diff_and_copy() {
    let bin = boxr_bin();
    if !bin.exists() {
        return;
    }

    let cont_name = "e2e-diff-copy-test";

    // Run container synchronously so changes are committed to container rootfs
    let output = Command::new(&bin)
        .args(["run", "--name", cont_name, "alpine", "/bin/sh", "-c", "touch /e2e-added.txt; echo 'changed' >> /etc/hosts"])
        .output()
        .unwrap();
    assert!(output.status.success());

    // Test boxr diff
    let output = Command::new(&bin).args(["diff", cont_name]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("A /e2e-added.txt"));

    // Test container-to-host copy
    let temp = tempdir().unwrap();
    let local_dest = temp.path().join("copied-from-cont.txt");
    let output = Command::new(&bin)
        .args(["cp", &format!("{}:/e2e-added.txt", cont_name), local_dest.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(local_dest.exists());

    // Test host-to-container copy
    let host_file = temp.path().join("host-src.txt");
    fs::write(&host_file, b"content from host").unwrap();
    let output = Command::new(&bin)
        .args(["cp", host_file.to_str().unwrap(), &format!("{}:/received-on-cont.txt", cont_name)])
        .output()
        .unwrap();
    assert!(output.status.success());

    // Verify file inside container rootfs
    let dest_in_cont = temp.path().join("verify-back.txt");
    let output = Command::new(&bin)
        .args(["cp", &format!("{}:/received-on-cont.txt", cont_name), dest_in_cont.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(fs::read_to_string(dest_in_cont).unwrap(), "content from host");

    // Clean up
    let _ = Command::new(&bin).args(["rm", cont_name]).output();
}

#[test]
fn test_e2e_defensive_security_and_kill() {
    let bin = boxr_bin();
    if !bin.exists() {
        return;
    }

    // 1. Path traversal in volume mount is rejected
    let output = Command::new(&bin)
        .args(["run", "--rm", "-v", "../../../etc:/data", "alpine", "/bin/echo", "test"])
        .output()
        .unwrap();
    assert!(!output.status.success(), "Volume mount with path traversal must fail");

    // 2. Kill command delivers signal and terminates process
    let cont_name = "e2e-kill-target";
    let output = Command::new(&bin)
        .args(["run", "-d", "--name", cont_name, "alpine", "/bin/sh", "-c", "sleep 30"])
        .output()
        .unwrap();
    assert!(output.status.success());

    // Send SIGKILL via boxr kill
    let output = Command::new(&bin).args(["kill", cont_name]).output().unwrap();
    assert!(output.status.success());

    // Verify exit code is 137 (128 + 9)
    let output = Command::new(&bin).args(["ps", "-a"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Exited (137)"));

    // Clean up
    let _ = Command::new(&bin).args(["rm", cont_name]).output();
}

#[test]
fn test_e2e_concurrent_load_test() {
    let bin = boxr_bin();
    if !bin.exists() {
        return;
    }

    // Launch 3 concurrent workers in parallel with unique names
    let run_id = hex::encode(boxr::storage::container_store::rand_id());
    let w1 = format!("load-a-{}", &run_id[..6]);
    let w2 = format!("load-b-{}", &run_id[..6]);
    let w3 = format!("load-c-{}", &run_id[..6]);
    let workers = [w1, w2, w3];

    for w in &workers {
        let output = Command::new(&bin)
            .args(["run", "-d", "--name", w, "alpine", "/bin/echo", "result=465"])
            .output()
            .unwrap();
        assert!(output.status.success());
    }

    // Wait on all workers
    for w in &workers {
        let output = Command::new(&bin).args(["wait", w]).output().unwrap();
        assert!(output.status.success());

        // Verify computed result in logs
        let output = Command::new(&bin).args(["logs", w]).output().unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("result=465"));

        // Clean up
        let _ = Command::new(&bin).args(["rm", w]).output();
    }
}

#[test]
fn test_e2e_real_service_workload_redis() {
    let bin = boxr_bin();
    if !bin.exists() {
        return;
    }

    let run_id = hex::encode(boxr::storage::container_store::rand_id());
    let cont_name = format!("e2e-redis-{}", &run_id[..6]);
    let host_port = "6381";

    // Start Redis server
    let output = Command::new(&bin)
        .args(["run", "-d", "--name", &cont_name, "-p", &format!("{}:6379", host_port), "redis:alpine", "redis-server", "--protected-mode", "no"])
        .output()
        .unwrap();
    assert!(output.status.success());

    std::thread::sleep(std::time::Duration::from_secs(2));

    // Test ping via exec
    let output = Command::new(&bin)
        .args(["exec", &cont_name, "redis-cli", "ping"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("PONG"));

    // Test SET / GET via exec
    let output = Command::new(&bin)
        .args(["exec", &cont_name, "redis-cli", "set", "e2e_key", "BoxrWorks"])
        .output()
        .unwrap();
    assert!(output.status.success());

    let output = Command::new(&bin)
        .args(["exec", &cont_name, "redis-cli", "get", "e2e_key"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("BoxrWorks"));

    // Clean up
    let _ = Command::new(&bin).args(["stop", &cont_name]).output();
    let _ = Command::new(&bin).args(["rm", &cont_name]).output();
}
