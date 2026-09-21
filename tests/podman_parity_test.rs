//! Podman-specific parity tests (exists subcommands, network reload, machine).

use clap::Parser;
use boxr::cli::Cli;

#[test]
fn test_podman_container_exists_parses() {
    let cli = Cli::try_parse_from(["boxr", "container", "exists", "my-container"]).unwrap();
    let _ = cli;
}

#[test]
fn test_podman_image_exists_parses() {
    let cli = Cli::try_parse_from(["boxr", "image", "exists", "alpine:latest"]).unwrap();
    let _ = cli;
}

#[test]
fn test_podman_volume_exists_parses() {
    let cli = Cli::try_parse_from(["boxr", "volume", "exists", "myvol"]).unwrap();
    let _ = cli;
}

#[test]
fn test_podman_network_exists_parses() {
    let cli = Cli::try_parse_from(["boxr", "network", "exists", "bridge"]).unwrap();
    let _ = cli;
}

#[test]
fn test_podman_network_reload_parses() {
    let cli = Cli::try_parse_from(["boxr", "network", "reload", "web"]).unwrap();
    let _ = cli;
}

#[test]
fn test_container_exists_missing_returns_error() {
    let err = boxr::ensure_container_exists("definitely-missing-container-id").unwrap_err();
    assert!(err.to_string().contains("not found"));
}

#[test]
fn test_volume_exists_missing_returns_error() {
    let err = boxr::ensure_volume_exists("definitely-missing-volume").unwrap_err();
    assert!(err.to_string().contains("not found"));
}

#[test]
fn test_network_exists_missing_returns_error() {
    let err = boxr::ensure_network_exists("definitely-missing-network").unwrap_err();
    assert!(err.to_string().contains("not found"));
}

#[test]
fn test_image_exists_missing_returns_error() {
    let err = boxr::ensure_image_exists("definitely-missing-image:tag").unwrap_err();
    assert!(err.to_string().contains("not found"));
}

#[test]
fn test_network_reload_requires_container() {
    let err = boxr::reload_container_networks(&[]).unwrap_err();
    assert!(err.to_string().contains("requires at least one container"));
}
