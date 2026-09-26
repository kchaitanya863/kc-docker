use super::DaemonState;
use crate::storage::{ContainerStatus, ContainerStore, ImageStore};
use axum::{Json, extract::State};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct VersionResponse {
    #[serde(rename = "Version")]
    pub version: String,
    #[serde(rename = "ApiVersion")]
    pub api_version: String,
    #[serde(rename = "MinAPIVersion")]
    pub min_api_version: String,
    #[serde(rename = "GitCommit")]
    pub git_commit: String,
    #[serde(rename = "GoVersion")]
    pub go_version: String,
    #[serde(rename = "Os")]
    pub os: String,
    #[serde(rename = "Arch")]
    pub arch: String,
    #[serde(rename = "KernelVersion")]
    pub kernel_version: String,
    #[serde(rename = "Experimental")]
    pub experimental: bool,
}

#[derive(Debug, Serialize)]
pub struct InfoResponse {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "Containers")]
    pub containers: usize,
    #[serde(rename = "ContainersRunning")]
    pub containers_running: usize,
    #[serde(rename = "ContainersPaused")]
    pub containers_paused: usize,
    #[serde(rename = "ContainersStopped")]
    pub containers_stopped: usize,
    #[serde(rename = "Images")]
    pub images: usize,
    #[serde(rename = "Driver")]
    pub driver: String,
    #[serde(rename = "SystemTime")]
    pub system_time: String,
    #[serde(rename = "ServerVersion")]
    pub server_version: String,
}

pub async fn ping() -> &'static str {
    "OK"
}

pub async fn version() -> Json<VersionResponse> {
    Json(VersionResponse {
        version: "0.1.0".to_string(),
        api_version: "1.45".to_string(),
        min_api_version: "1.24".to_string(),
        git_commit: "boxr-git".to_string(),
        go_version: "rust-1.98.1".to_string(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        kernel_version: "standard".to_string(),
        experimental: true,
    })
}

pub async fn info(State(state): State<DaemonState>) -> Json<InfoResponse> {
    let c_store = ContainerStore::with_home(state.home.clone());
    let i_store = ImageStore::with_home(state.home.clone());
    let containers = c_store.list();
    let running = containers
        .iter()
        .filter(|c| matches!(c.status, ContainerStatus::Running))
        .count();
    let stopped = containers
        .iter()
        .filter(|c| matches!(c.status, ContainerStatus::Exited(_)))
        .count();

    Json(InfoResponse {
        id: "boxr-engine-01".to_string(),
        containers: containers.len(),
        containers_running: running,
        containers_paused: 0,
        containers_stopped: stopped,
        images: i_store.list().len(),
        driver: "overlayfs".to_string(),
        system_time: chrono::Utc::now().to_rfc3339(),
        server_version: "0.1.0".to_string(),
    })
}
