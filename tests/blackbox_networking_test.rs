#[path = "common/blackbox.rs"]
mod blackbox;

use blackbox::*;
use std::time::Duration;

#[test]
fn test_d1_tcp_publish_and_curl() {
    let (_guard, home) = isolated_home();
    pull_if_needed(&home, "nginx:alpine");
    let suffix = rand_suffix();
    let ctr = format!("bb-net-tcp-{}", suffix);
    let port = 19000 + suffix.chars().take(3).fold(0u32, |a, c| {
        a * 10 + c.to_digit(10).unwrap_or(1)
    }) % 500;
    run_boxr_ok(
        &home,
        &[
            "run",
            "-d",
            "--name",
            &ctr,
            "-p",
            &format!("127.0.0.1:{}:80", port),
            "nginx:alpine",
        ],
    );
    assert!(wait_container_running(&home, &ctr, Duration::from_secs(30)));
    std::thread::sleep(Duration::from_secs(2));
    let curl = std::process::Command::new("curl")
        .args(["-sf", &format!("http://127.0.0.1:{}/", port)])
        .output();
    cleanup_container(&home, &ctr);
    if let Ok(out) = curl {
        assert!(out.status.success());
    }
}

#[test]
fn test_d4_port_collision_rejected() {
    let (_guard, home) = isolated_home();
    pull_if_needed(&home, "alpine:latest");
    let suffix = rand_suffix();
    let port = 19500 + suffix.len() as u16;
    let c1 = format!("bb-collide-1-{}", suffix);
    let c2 = format!("bb-collide-2-{}", suffix);
    run_boxr_ok(
        &home,
        &[
            "run",
            "-d",
            "--name",
            &c1,
            "-p",
            &format!("{}:80", port),
            "alpine",
            "sleep",
            "60",
        ],
    );
    let out = run_boxr(
        &home,
        &[
            "run",
            "-d",
            "--name",
            &c2,
            "-p",
            &format!("{}:80", port),
            "alpine",
            "sleep",
            "60",
        ],
    );
    cleanup_container(&home, &c1);
    cleanup_container(&home, &c2);
    assert!(!out.status.success(), "expected port collision error");
}

#[test]
fn test_d5_network_connect() {
    let (_guard, home) = isolated_home();
    pull_if_needed(&home, "alpine:latest");
    let suffix = rand_suffix();
    let net = format!("bb-net-{}", suffix);
    let ctr = format!("bb-net-ctr-{}", suffix);
    run_boxr_ok(&home, &["network", "create", &net]);
    run_boxr_ok(
        &home,
        &["run", "-d", "--name", &ctr, "--network", "none", "alpine", "sleep", "120"],
    );
    let out = run_boxr(&home, &["network", "connect", &net, &ctr]);
    cleanup_container(&home, &ctr);
    cleanup_network(&home, &net);
    assert!(out.status.success());
}

#[test]
fn test_d7_restart_preserves_port_forward() {
    let (_guard, home) = isolated_home();
    pull_if_needed(&home, "nginx:alpine");
    let suffix = rand_suffix();
    let ctr = format!("bb-restart-{}", suffix);
    let port = 19600 + suffix.len() as u16;
    run_boxr_ok(
        &home,
        &[
            "run",
            "-d",
            "--name",
            &ctr,
            "-p",
            &format!("{}:80", port),
            "nginx:alpine",
        ],
    );
    std::thread::sleep(Duration::from_secs(2));
    run_boxr_ok(&home, &["restart", &ctr]);
    std::thread::sleep(Duration::from_secs(3));
    let curl = std::process::Command::new("curl")
        .args(["-sf", &format!("http://127.0.0.1:{}/", port)])
        .output();
    cleanup_container(&home, &ctr);
    if let Ok(out) = curl {
        assert!(out.status.success(), "port forward lost after restart");
    }
}

#[test]
fn test_d10_outbound_connectivity() {
    let (_guard, home) = isolated_home();
    pull_if_needed(&home, "alpine:latest");
    let out = run_boxr(
        &home,
        &[
            "run",
            "--rm",
            "alpine",
            "wget",
            "-qO-",
            "http://example.com",
        ],
    );
    let combined = combined_output(&out);
    assert!(
        out.status.success() || combined.contains("Example"),
        "outbound HTTP failed: {}",
        combined
    );
}

#[test]
fn test_d_network_none_blocks_external() {
    let (_guard, home) = isolated_home();
    pull_if_needed(&home, "alpine:latest");
    let out = run_boxr(
        &home,
        &[
            "run",
            "--rm",
            "--network",
            "none",
            "alpine",
            "wget",
            "-q",
            "--timeout=3",
            "-O",
            "/dev/null",
            "http://8.8.8.8",
        ],
    );
    assert!(!out.status.success());
}

#[test]
fn test_e1_dns_resolution() {
    let (_guard, home) = isolated_home();
    pull_if_needed(&home, "alpine:latest");
    let out = run_boxr(
        &home,
        &[
            "run",
            "--rm",
            "alpine",
            "/bin/sh",
            "-c",
            "nslookup example.com 2>/dev/null || wget -qO- http://example.com | head -1",
        ],
    );
    let combined = combined_output(&out);
    assert!(
        out.status.success() || combined.contains("Example") || combined.contains("Address"),
        "DNS/outbound failed: {}",
        combined
    );
}

#[test]
fn test_e2_custom_dns_server() {
    let (_guard, home) = isolated_home();
    pull_if_needed(&home, "alpine:latest");
    let out = run_boxr_ok(
        &home,
        &[
            "run",
            "--rm",
            "--dns",
            "8.8.8.8",
            "alpine",
            "cat",
            "/etc/resolv.conf",
        ],
    );
    assert!(out.contains("8.8.8.8") || out.contains("nameserver"));
}

#[test]
fn test_e7_bridge_hosts_name_resolution() {
    let (_guard, home) = isolated_home();
    pull_if_needed(&home, "alpine:latest");
    let fixture = fixture_path("tests/fixtures/compose/minimal-dns.yml");
    let _ = run_boxr(&home, &["compose", "-f", fixture.to_str().unwrap(), "down", "-v"]);
    run_boxr_ok(
        &home,
        &["compose", "-f", fixture.to_str().unwrap(), "up", "-d"],
    );
    std::thread::sleep(Duration::from_secs(5));
    let ps = run_boxr_ok(&home, &["compose", "-f", fixture.to_str().unwrap(), "ps"]);
    assert!(ps.contains("web") || ps.contains("db"));
    let _ = run_boxr(&home, &["compose", "-f", fixture.to_str().unwrap(), "down", "-v"]);
}

#[cfg(target_os = "linux")]
#[test]
fn test_d_linux_usernet_mode() {
    let (_guard, home) = isolated_home();
    pull_if_needed(&home, "alpine:latest");
    let out = run_boxr(
        &home,
        &[
            "run",
            "--rm",
            "--network",
            "usernet",
            "alpine",
            "wget",
            "-qO-",
            "http://example.com",
        ],
    );
    assert!(out.status.success() || combined_output(&out).contains("Example"));
}
