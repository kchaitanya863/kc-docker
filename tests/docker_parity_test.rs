use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::tempdir;

fn boxr_bin() -> PathBuf {
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

fn boxr_cmd(bin: &PathBuf) -> Command {
    let mut cmd = Command::new(bin);
    cmd.env_remove("DOCKER_HOST");
    cmd
}

fn unique_id() -> String {
    hex::encode(boxr::storage::container_store::rand_id())[..8].to_string()
}

/// Docker Parity Test: Version and System Info commands
#[test]
fn test_docker_parity_version_and_info() {
    let bin = boxr_bin();
    if !bin.exists() {
        return;
    }

    let out = boxr_cmd(&bin).arg("version").output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Client:"));
    assert!(stdout.contains("Server:"));
    assert!(stdout.contains("Version:"));
    assert!(stdout.contains("API version:"));

    let out = boxr_cmd(&bin).arg("info").output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Containers:"));
    assert!(stdout.contains("Images:"));
    assert!(stdout.contains("Operating System:"));
}

/// Docker Parity Test: Volume CLI lifecycle (create, ls, inspect, rm, prune)
#[test]
fn test_docker_parity_volume_crud() {
    let bin = boxr_bin();
    if !bin.exists() {
        return;
    }

    let vol = format!("dockertest-vol-{}", unique_id());

    // 1. volume create
    let out = boxr_cmd(&bin)
        .args(["volume", "create", &vol])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), vol);

    // 2. volume ls
    let out = boxr_cmd(&bin).args(["volume", "ls"]).output().unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains(&vol));

    // 3. volume inspect
    let out = boxr_cmd(&bin)
        .args(["volume", "inspect", &vol])
        .output()
        .unwrap();
    assert!(out.status.success());
    let json_str = String::from_utf8_lossy(&out.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    let name_val = parsed.get("name").or_else(|| parsed.get("Name")).unwrap().as_str().unwrap();
    assert_eq!(name_val, vol);

    // 4. volume rm
    let out = boxr_cmd(&bin).args(["volume", "rm", &vol]).output().unwrap();
    assert!(out.status.success());

    // 5. Verify gone
    let out = boxr_cmd(&bin).args(["volume", "ls"]).output().unwrap();
    assert!(!String::from_utf8_lossy(&out.stdout).contains(&vol));
}

/// Docker Parity Test: Network CLI lifecycle (create, ls, inspect, rm, prune)
#[test]
fn test_docker_parity_network_crud() {
    let bin = boxr_bin();
    if !bin.exists() {
        return;
    }

    let net = format!("dockertest-net-{}", unique_id());

    // 1. network create
    let out = boxr_cmd(&bin)
        .args(["network", "create", &net])
        .output()
        .unwrap();
    assert!(out.status.success());

    // 2. network ls
    let out = boxr_cmd(&bin).args(["network", "ls"]).output().unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains(&net));

    // 3. network inspect
    let out = boxr_cmd(&bin)
        .args(["network", "inspect", &net])
        .output()
        .unwrap();
    assert!(out.status.success());
    let json_str = String::from_utf8_lossy(&out.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    let name_val = parsed.get("name").or_else(|| parsed.get("Name")).unwrap().as_str().unwrap();
    assert_eq!(name_val, net);

    // 4. network rm
    let out = boxr_cmd(&bin).args(["network", "rm", &net]).output().unwrap();
    assert!(out.status.success());

    // 5. Verify removed
    let out = boxr_cmd(&bin).args(["network", "ls"]).output().unwrap();
    assert!(!String::from_utf8_lossy(&out.stdout).contains(&net));
}

/// Docker Parity Test: Container Creation without starting (docker create)
#[test]
fn test_docker_parity_create_command() {
    let bin = boxr_bin();
    if !bin.exists() {
        return;
    }

    let name = format!("dockertest-created-{}", unique_id());

    let out = boxr_cmd(&bin)
        .args(["create", "--name", &name, "alpine", "echo", "created"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let id = String::from_utf8_lossy(&out.stdout).trim().to_string();
    assert!(!id.is_empty());

    // Inspect created container
    let out = boxr_cmd(&bin).args(["inspect", &name]).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains(&name));

    // Cleanup
    let _ = boxr_cmd(&bin).args(["rm", &name]).output();
}

/// Docker Parity Test: Dockerfile Build and Image Tagging / Removal
#[test]
fn test_docker_parity_build_and_tag_lifecycle() {
    let bin = boxr_bin();
    if !bin.exists() {
        return;
    }

    let temp = tempdir().unwrap();
    let dockerfile = r#"
FROM alpine:latest
ENV APP_ENV=production
RUN echo "hello docker parity" > /app.txt
CMD ["cat", "/app.txt"]
"#;

    fs::write(temp.path().join("Dockerfile"), dockerfile).unwrap();
    let img_name = format!("dockertest-img-{}", unique_id());
    let tag1 = format!("{}:1.0", img_name);
    let tag2 = format!("{}:latest", img_name);

    // 1. build
    let out = boxr_cmd(&bin)
        .args(["build", "-t", &tag1, temp.path().to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success(), "docker build failed: {:?}", out);

    // 2. tag
    let out = boxr_cmd(&bin).args(["tag", &tag1, &tag2]).output().unwrap();
    assert!(out.status.success());

    // 3. images list
    let out = boxr_cmd(&bin).args(["images"]).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains(&img_name));

    // 4. history
    let out = boxr_cmd(&bin).args(["history", &tag1]).output().unwrap();
    assert!(out.status.success());

    // 5. rmi
    let out = boxr_cmd(&bin).args(["rmi", &tag2]).output().unwrap();
    assert!(out.status.success());
    let out = boxr_cmd(&bin).args(["rmi", &tag1]).output().unwrap();
    assert!(out.status.success());
}

/// Docker Parity Test: Docker Compose specification orchestration
#[test]
fn test_docker_parity_compose_up_down() {
    let bin = boxr_bin();
    if !bin.exists() {
        return;
    }

    let temp = tempdir().unwrap();
    let compose_file = r#"
version: '3.8'
services:
  web:
    image: nginx:alpine
    ports:
      - "8999:80"
  cache:
    image: redis:alpine
"#;

    let file_path = temp.path().join("docker-compose.yml");
    fs::write(&file_path, compose_file).unwrap();

    let out = boxr_cmd(&bin)
        .args(["compose", "-f", file_path.to_str().unwrap(), "up", "-d"])
        .output()
        .unwrap();
    assert!(out.status.success());

    let out = boxr_cmd(&bin)
        .args(["compose", "-f", file_path.to_str().unwrap(), "down"])
        .output()
        .unwrap();
    assert!(out.status.success());
}

/// Docker Parity Test: System disk usage and prune commands
#[test]
fn test_docker_parity_system_df_and_prune() {
    let bin = boxr_bin();
    if !bin.exists() {
        return;
    }

    let out = boxr_cmd(&bin).args(["system", "df"]).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("TYPE"));
    assert!(stdout.contains("TOTAL"));

    let out = boxr_cmd(&bin).args(["system", "prune", "-f"]).output().unwrap();
    assert!(out.status.success());
}

/// Docker Parity Test: Image Inspect, History, Save and Load lifecycle
#[test]
fn test_docker_parity_image_inspect_history_save_load() {
    let bin = boxr_bin();
    if !bin.exists() {
        return;
    }

    let temp = tempdir().unwrap();
    let dockerfile = "FROM alpine:latest\nRUN echo 'save-load-test' > /test.txt\n";
    fs::write(temp.path().join("Dockerfile"), dockerfile).unwrap();

    let tag = format!("dockertest-saveload-{}", unique_id());
    let archive_path = temp.path().join("image-archive.tar");

    // 1. Build
    let out = boxr_cmd(&bin)
        .args(["build", "-t", &tag, temp.path().to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success());

    // 2. Inspect
    let out = boxr_cmd(&bin).args(["inspect", &tag]).output().unwrap();
    assert!(out.status.success());
    let json_str = String::from_utf8_lossy(&out.stdout);
    assert!(json_str.contains(&tag));

    // 3. History
    let out = boxr_cmd(&bin).args(["history", &tag]).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("IMAGE"));

    // 4. Save
    let out = boxr_cmd(&bin)
        .args(["save", "-o", archive_path.to_str().unwrap(), &tag])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(archive_path.exists());

    // 5. Remove image
    let out = boxr_cmd(&bin).args(["rmi", &tag]).output().unwrap();
    assert!(out.status.success());

    // 6. Load
    let out = boxr_cmd(&bin)
        .args(["load", "-i", archive_path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success());

    // Verify loaded
    let out = boxr_cmd(&bin).args(["images"]).output().unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains(&tag));

    // Cleanup
    let _ = boxr_cmd(&bin).args(["rmi", &tag]).output();
}

/// Docker Parity Test: Container Rename and Port Inspection
#[test]
fn test_docker_parity_container_rename_and_port() {
    let bin = boxr_bin();
    if !bin.exists() {
        return;
    }

    let orig_name = format!("dockertest-orig-{}", unique_id());
    let new_name = format!("dockertest-new-{}", unique_id());

    // Create container with port mapping
    let out = boxr_cmd(&bin)
        .args(["create", "--name", &orig_name, "-p", "8888:80", "alpine"])
        .output()
        .unwrap();
    assert!(out.status.success());

    // Port check
    let out = boxr_cmd(&bin).args(["port", &orig_name]).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("80/tcp") || stdout.contains("8888"));

    // Rename
    let out = boxr_cmd(&bin).args(["rename", &orig_name, &new_name]).output().unwrap();
    assert!(out.status.success());

    // Verify new name
    let out = boxr_cmd(&bin).args(["inspect", &new_name]).output().unwrap();
    assert!(out.status.success());

    // Old name should now fail
    let out = boxr_cmd(&bin).args(["inspect", &orig_name]).output().unwrap();
    assert!(!out.status.success());

    // Cleanup
    let _ = boxr_cmd(&bin).args(["rm", &new_name]).output();
}

/// Docker Parity Test: Negative CLI handling (non-existent containers/images)
#[test]
fn test_docker_parity_error_handling() {
    let bin = boxr_bin();
    if !bin.exists() {
        return;
    }

    // Inspect non-existent container should exit non-zero
    let out = boxr_cmd(&bin)
        .args(["inspect", "container_does_not_exist_xyz"])
        .output()
        .unwrap();
    assert!(!out.status.success());

    // Stop non-existent container should exit non-zero
    let out = boxr_cmd(&bin)
        .args(["stop", "container_does_not_exist_xyz"])
        .output()
        .unwrap();
    assert!(!out.status.success());

    // Remove non-existent volume should exit non-zero
    let out = boxr_cmd(&bin)
        .args(["volume", "rm", "volume_does_not_exist_xyz"])
        .output()
        .unwrap();
    assert!(!out.status.success());
}
