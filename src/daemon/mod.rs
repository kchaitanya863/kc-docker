use crate::network::NetworkStore;
use crate::storage::{ContainerRecord, ContainerStatus, ContainerStore, ImageStore, boxr_home};
use crate::volume::VolumeStore;
use anyhow::{Context, Result};
use axum::{
    Json, Router,
    extract::{Path, Query},
    http::StatusCode,
    routing::{delete, get, post},
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
#[cfg(windows)]
use tokio::net::TcpListener;
#[cfg(unix)]
use tokio::net::UnixListener;

#[allow(dead_code)]
#[derive(Clone)]
pub struct DaemonState {
    pub home: PathBuf,
}

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

pub fn create_router(state: DaemonState) -> Router {
    Router::new()
        .route("/_ping", get(ping))
        .route("/version", get(version))
        .route("/v1.45/version", get(version))
        .route("/info", get(info))
        .route("/v1.45/info", get(info))
        .route("/images/json", get(list_images))
        .route("/v1.45/images/json", get(list_images))
        .route("/images/{name}/json", get(inspect_image))
        .route("/v1.45/images/{name}/json", get(inspect_image))
        .route("/images/create", post(create_image))
        .route("/v1.45/images/create", post(create_image))
        .route("/containers/json", get(list_containers))
        .route("/v1.45/containers/json", get(list_containers))
        .route("/containers/{id}/json", get(inspect_container))
        .route("/v1.45/containers/{id}/json", get(inspect_container))
        .route("/containers/create", post(create_container))
        .route("/v1.45/containers/create", post(create_container))
        .route("/containers/{id}/start", post(start_container))
        .route("/v1.45/containers/{id}/start", post(start_container))
        .route("/containers/{id}/stop", post(stop_container))
        .route("/v1.45/containers/{id}/stop", post(stop_container))
        .route("/containers/{id}/restart", post(restart_container))
        .route("/v1.45/containers/{id}/restart", post(restart_container))
        .route("/containers/{id}/kill", post(kill_container))
        .route("/v1.45/containers/{id}/kill", post(kill_container))
        .route("/containers/{id}/wait", post(wait_container))
        .route("/v1.45/containers/{id}/wait", post(wait_container))
        .route("/containers/{id}/logs", get(get_container_logs))
        .route("/v1.45/containers/{id}/logs", get(get_container_logs))
        .route("/containers/{id}", delete(remove_container))
        .route("/v1.45/containers/{id}", delete(remove_container))
        .route("/networks", get(list_networks))
        .route("/v1.45/networks", get(list_networks))
        .route("/networks/create", post(create_network))
        .route("/v1.45/networks/create", post(create_network))
        .route("/volumes", get(list_volumes))
        .route("/v1.45/volumes", get(list_volumes))
        .route("/volumes/create", post(create_volume))
        .route("/v1.45/volumes/create", post(create_volume))
        .with_state(state)
}

pub async fn start_daemon(socket_path: Option<&str>) -> Result<()> {
    let home = boxr_home();

    #[cfg(unix)]
    {
        let sock = socket_path
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("boxr.sock"));

        if sock.exists() {
            let _ = fs::remove_file(&sock);
        }
        if let Some(parent) = sock.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let listener = UnixListener::bind(&sock)
            .with_context(|| format!("Failed to bind Unix domain socket at {:?}", sock))?;

        println!("boxr daemon listening on unix://{:?}", sock);

        tokio::spawn(async {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                let _ = crate::guardrails::ProcessReaper::reap_stale_containers();
            }
        });

        let state = DaemonState { home };
        let app = create_router(state);

        axum::serve(listener, app).await?;
    }

    #[cfg(windows)]
    {
        let addr = socket_path.unwrap_or("127.0.0.1:2375");
        let listener = TcpListener::bind(addr)
            .await
            .with_context(|| format!("Failed to bind TCP listener at {}", addr))?;

        println!("boxr daemon listening on tcp://{}", addr);

        tokio::spawn(async {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                let _ = crate::guardrails::ProcessReaper::reap_stale_containers();
            }
        });

        let state = DaemonState { home };
        let app = create_router(state);

        axum::serve(listener, app).await?;
    }

    Ok(())
}

async fn ping() -> &'static str {
    "OK"
}

async fn version() -> Json<VersionResponse> {
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

async fn info() -> Json<InfoResponse> {
    let c_store = ContainerStore::new();
    let i_store = ImageStore::new();
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

async fn list_images() -> Json<serde_json::Value> {
    let store = ImageStore::new();
    let images = store.list();
    let val = serde_json::to_value(images).unwrap_or_default();
    Json(val)
}

#[derive(Deserialize)]
struct CreateImageQuery {
    #[serde(rename = "fromImage")]
    from_image: String,
}

async fn create_image(
    Query(params): Query<CreateImageQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match crate::pull_image(&params.from_image).await {
        Ok(rec) => Ok(Json(serde_json::to_value(rec).unwrap())),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

#[derive(Deserialize)]
struct ListContainersQuery {
    all: Option<serde_json::Value>,
}

async fn list_containers(Query(params): Query<ListContainersQuery>) -> Json<serde_json::Value> {
    let store = ContainerStore::new();
    let mut containers = store.list();
    let show_all = match &params.all {
        Some(serde_json::Value::Bool(b)) => *b,
        Some(serde_json::Value::Number(n)) => n.as_i64() == Some(1),
        Some(serde_json::Value::String(s)) => s == "1" || s.to_lowercase() == "true",
        _ => false,
    };
    if !show_all {
        containers.retain(|c| matches!(c.status, ContainerStatus::Running));
    }
    Json(serde_json::to_value(containers).unwrap_or_default())
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct CreateContainerRequest {
    #[serde(rename = "Image")]
    image: String,
    #[serde(rename = "Cmd")]
    cmd: Option<Vec<String>>,
    #[serde(rename = "Env")]
    env: Option<Vec<String>>,
}

async fn create_container(
    Json(payload): Json<CreateContainerRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let random_id = hex::encode(crate::storage::container_store::rand_id());
    let store = ContainerStore::new();
    let record = ContainerRecord {
        id: random_id.clone(),
        name: format!("boxr-{}", &random_id[..6]),
        image: payload.image,
        command: payload.cmd.unwrap_or_default(),
        created_at: chrono::Utc::now(),
        status: ContainerStatus::Created,
        bundle_path: format!("/tmp/boxr/containers/{}", random_id),
        restart_policy: crate::health::RestartPolicy::No,
        health_status: crate::health::HealthStatus::None,
        restart_count: 0,
        ports: Vec::new(),
    };

    store
        .add(record.clone())
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    #[derive(Serialize)]
    struct Resp {
        #[serde(rename = "Id")]
        id: String,
        #[serde(rename = "Warnings")]
        warnings: Vec<String>,
    }

    Ok(Json(
        serde_json::to_value(Resp {
            id: random_id,
            warnings: vec![],
        })
        .unwrap(),
    ))
}

async fn start_container(Path(id): Path<String>) -> StatusCode {
    let store = ContainerStore::new();
    match store.update_status(&id, ContainerStatus::Running) {
        Ok(_) => StatusCode::NO_CONTENT,
        Err(_) => StatusCode::NOT_FOUND,
    }
}

async fn stop_container(Path(id): Path<String>) -> StatusCode {
    let store = ContainerStore::new();
    match store.update_status(&id, ContainerStatus::Exited(0)) {
        Ok(_) => StatusCode::NO_CONTENT,
        Err(_) => StatusCode::NOT_FOUND,
    }
}

async fn remove_container(Path(id): Path<String>) -> StatusCode {
    match crate::remove_container(&id, true) {
        Ok(_) => StatusCode::NO_CONTENT,
        Err(_) => StatusCode::NOT_FOUND,
    }
}

async fn list_networks() -> Json<serde_json::Value> {
    let store = NetworkStore::new();
    Json(serde_json::to_value(store.list()).unwrap_or_default())
}

#[derive(Deserialize)]
struct CreateNetworkRequest {
    #[serde(rename = "Name")]
    name: String,
}

async fn create_network(
    Json(payload): Json<CreateNetworkRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let store = NetworkStore::new();
    match store.create(&payload.name, None, None) {
        Ok(net) => Ok(Json(serde_json::to_value(net).unwrap())),
        Err(_) => Err(StatusCode::BAD_REQUEST),
    }
}

async fn list_volumes() -> Json<serde_json::Value> {
    let store = VolumeStore::new();
    #[derive(Serialize)]
    struct VolResp {
        #[serde(rename = "Volumes")]
        volumes: Vec<crate::volume::VolumeRecord>,
    }
    Json(
        serde_json::to_value(VolResp {
            volumes: store.list(),
        })
        .unwrap_or_default(),
    )
}

#[derive(Deserialize)]
struct CreateVolumeRequest {
    #[serde(rename = "Name")]
    name: Option<String>,
}

async fn create_volume(
    Json(payload): Json<CreateVolumeRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let store = VolumeStore::new();
    match store.create(payload.name.as_deref(), None) {
        Ok(vol) => Ok(Json(serde_json::to_value(vol).unwrap())),
        Err(_) => Err(StatusCode::BAD_REQUEST),
    }
}

async fn inspect_container(Path(id): Path<String>) -> Result<Json<serde_json::Value>, StatusCode> {
    let store = ContainerStore::new();
    let c = store.find(&id).ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(serde_json::json!({
        "Id": c.id,
        "Created": c.created_at.to_rfc3339(),
        "Path": c.command.first().cloned().unwrap_or_default(),
        "Args": if c.command.len() > 1 { c.command[1..].to_vec() } else { vec![] },
        "State": {
            "Status": c.status.to_string(),
            "Running": matches!(c.status, ContainerStatus::Running),
            "Paused": matches!(c.status, ContainerStatus::Paused),
            "ExitCode": match c.status {
                ContainerStatus::Exited(code) => code,
                _ => 0,
            }
        },
        "Image": c.image,
        "Name": format!("/{}", c.name),
        "RestartPolicy": { "Name": c.restart_policy.to_string() },
        "NetworkSettings": {
            "Ports": c.ports
        }
    })))
}

async fn inspect_image(Path(name): Path<String>) -> Result<Json<serde_json::Value>, StatusCode> {
    let store = ImageStore::new();
    let img = store.find(&name).ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(serde_json::json!({
        "Id": format!("sha256:{}", img.id),
        "RepoTags": [format!("{}:{}", img.reference, img.tag)],
        "Size": img.size_bytes,
        "Created": img.created_at.to_rfc3339(),
        "Architecture": img.config.architecture,
        "Os": img.config.os,
    })))
}

async fn restart_container(Path(id): Path<String>) -> StatusCode {
    let _ = crate::stop_container(&id);
    match crate::start_container(&id).await {
        Ok(_) => StatusCode::NO_CONTENT,
        Err(_) => StatusCode::NOT_FOUND,
    }
}

async fn kill_container(Path(id): Path<String>) -> StatusCode {
    let store = ContainerStore::new();
    if let Some(c) = store.find(&id) {
        let _ = crate::runtime::kill::ContainerKiller::kill(&c, None);
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

async fn wait_container(Path(id): Path<String>) -> Result<Json<serde_json::Value>, StatusCode> {
    let store = ContainerStore::new();
    let c = store.find(&id).ok_or(StatusCode::NOT_FOUND)?;
    let bundle_path = PathBuf::from(&c.bundle_path);
    let pid_file = bundle_path.join("vm.pid");
    for _ in 0..100 {
        if let Ok(pid_str) = fs::read_to_string(&pid_file) {
            if let Ok(pid) = pid_str.trim().parse::<i32>() {
                #[cfg(unix)]
                if unsafe { libc::kill(pid, 0) == 0 } {
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    continue;
                }
            }
        }
        break;
    }
    let exit_code = if let Ok(c) = fs::read_to_string(bundle_path.join("boxr-exitcode")) {
        c.trim().parse::<i32>().unwrap_or(0)
    } else {
        0
    };
    Ok(Json(serde_json::json!({ "StatusCode": exit_code })))
}

async fn get_container_logs(Path(id): Path<String>) -> Result<String, StatusCode> {
    let store = ContainerStore::new();
    let c = store.find(&id).ok_or(StatusCode::NOT_FOUND)?;
    let log_path = PathBuf::from(&c.bundle_path).join("logs.txt");
    if log_path.exists() {
        fs::read_to_string(&log_path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
    } else {
        Ok(String::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_ping_and_version() {
        let state = DaemonState {
            home: PathBuf::from("/tmp/test-boxr-daemon"),
        };
        let app = create_router(state);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/_ping")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/version")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
