use crate::network::NetworkStore;
use crate::storage::{ContainerStatus, ContainerStore, ImageStore, boxr_home};
use crate::volume::VolumeStore;
use anyhow::{Context, Result};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
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
    unsafe {
        std::env::set_var("BOXR_HOME", &state.home);
    }
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
        .route("/containers/{id}/exec", post(create_container_exec))
        .route("/v1.45/containers/{id}/exec", post(create_container_exec))
        .route("/exec/{id}/start", post(start_exec_instance))
        .route("/v1.45/exec/{id}/start", post(start_exec_instance))
        .route("/exec/{id}/json", get(inspect_exec_instance))
        .route("/v1.45/exec/{id}/json", get(inspect_exec_instance))
        .route("/containers/prune", post(prune_containers_endpoint))
        .route("/v1.45/containers/prune", post(prune_containers_endpoint))
        .route("/images/prune", post(prune_images_endpoint))
        .route("/v1.45/images/prune", post(prune_images_endpoint))
        .route("/volumes/prune", post(prune_volumes_endpoint))
        .route("/v1.45/volumes/prune", post(prune_volumes_endpoint))
        .route("/networks/prune", post(prune_networks_endpoint))
        .route("/v1.45/networks/prune", post(prune_networks_endpoint))
        .route("/networks", get(list_networks))
        .route("/v1.45/networks", get(list_networks))
        .route("/networks/create", post(create_network))
        .route("/v1.45/networks/create", post(create_network))
        .route("/networks/{id}", get(inspect_network))
        .route("/v1.45/networks/{id}", get(inspect_network))
        .route("/networks/{id}", delete(remove_network))
        .route("/v1.45/networks/{id}", delete(remove_network))
        .route("/volumes", get(list_volumes))
        .route("/v1.45/volumes", get(list_volumes))
        .route("/volumes/create", post(create_volume))
        .route("/v1.45/volumes/create", post(create_volume))
        .route("/volumes/{name}", get(inspect_volume))
        .route("/v1.45/volumes/{name}", get(inspect_volume))
        .route("/volumes/{name}", delete(remove_volume))
        .route("/v1.45/volumes/{name}", delete(remove_volume))
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

async fn info(State(state): State<DaemonState>) -> Json<InfoResponse> {
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

async fn list_images(State(state): State<DaemonState>) -> Json<serde_json::Value> {
    let store = ImageStore::with_home(state.home.clone());
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

async fn list_containers(
    State(state): State<DaemonState>,
    Query(params): Query<ListContainersQuery>,
) -> Json<serde_json::Value> {
    let store = ContainerStore::with_home(state.home.clone());
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
#[derive(Deserialize, Default)]
struct CreateContainerQuery {
    name: Option<String>,
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
    #[serde(rename = "WorkingDir")]
    working_dir: Option<String>,
    #[serde(rename = "User")]
    user: Option<String>,
}

async fn create_container(
    State(state): State<DaemonState>,
    Query(query): Query<CreateContainerQuery>,
    Json(payload): Json<CreateContainerRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    unsafe {
        std::env::set_var("BOXR_HOME", &state.home);
    }
    let run_args = crate::cli::RunArgs {
        interactive: false,
        tty: false,
        detach: true,
        rm: false,
        name: query.name,
        env: payload.env.unwrap_or_default(),
        ports: Vec::new(),
        volumes: Vec::new(),
        workdir: payload.working_dir,
        user: payload.user,
        hostname: None,
        add_host: Vec::new(),
        dns: Vec::new(),
        labels: Vec::new(),
        cidfile: None,
        memory: None,
        cpus: None,
        pids_limit: None,
        rootless: true,
        restart: "no".to_string(),
        health_cmd: None,
        platform: None,
        network: "auto".to_string(),
        privileged: false,
        gpus: None,
        entrypoint: None,
        env_file: None,
        shm_size: None,
        cap_add: Vec::new(),
        cap_drop: Vec::new(),
        read_only: false,
        init: false,
        tmpfs: Vec::new(),
        devices: Vec::new(),
        security_opt: Vec::new(),
        cpu_shares: None,
        cpuset_cpus: None,
        memory_swap: None,
        memory_reservation: None,
        dns_search: Vec::new(),
        dns_option: Vec::new(),
        expose: Vec::new(),
        sysctl: Vec::new(),
        stop_timeout: None,
        stop_signal: None,
        annotations: Vec::new(),
        ulimits: Vec::new(),
        ipc: None,
        pid: None,
        uts: None,
        userns: None,
        cgroupns: None,
        cgroup_parent: None,
        isolation: None,
        cpu_count: None,
        cpu_percent: None,
        io_maxbandwidth: None,
        io_maxiops: None,
        publish_all: false,
        ip: None,
        ip6: None,
        mac_address: None,
        link: Vec::new(),
        network_alias: Vec::new(),
        mount: Vec::new(),
        health_interval: None,
        health_timeout: None,
        health_retries: None,
        health_start_period: None,
        health_start_interval: None,
        no_healthcheck: false,
        attach: Vec::new(),
        pull: None,
        quiet: false,
        log_driver: None,
        log_opt: Vec::new(),
        oom_kill_disable: false,
        oom_score_adj: None,
        group_add: Vec::new(),
        label_file: None,
        umask: None,
        domainname: None,
        detach_keys: None,
        blkio_weight: None,
        blkio_weight_device: Vec::new(),
        cpu_period: None,
        cpu_quota: None,
        cpu_rt_period: None,
        cpu_rt_runtime: None,
        cpuset_mems: None,
        device_cgroup_rule: Vec::new(),
        device_read_bps: Vec::new(),
        device_read_iops: Vec::new(),
        device_write_bps: Vec::new(),
        device_write_iops: Vec::new(),
        link_local_ip: Vec::new(),
        memory_swappiness: None,
        runtime: None,
        sig_proxy: true,
        storage_opt: Vec::new(),
        use_api_socket: false,
        volume_driver: None,
        volumes_from: Vec::new(),
        image: payload.image,
        command: payload.cmd.unwrap_or_default(),
    };

    match crate::create_only_container(run_args).await {
        Ok(id) => Ok(Json(serde_json::json!({
            "Id": id,
            "Warnings": []
        }))),
        Err(e) => {
            eprintln!("Failed to create container via REST API: {:#}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn start_container(State(state): State<DaemonState>, Path(id): Path<String>) -> StatusCode {
    unsafe {
        std::env::set_var("BOXR_HOME", &state.home);
    }
    match crate::start_container(&id).await {
        Ok(_) => StatusCode::NO_CONTENT,
        Err(_) => StatusCode::NOT_FOUND,
    }
}

async fn stop_container(State(state): State<DaemonState>, Path(id): Path<String>) -> StatusCode {
    unsafe {
        std::env::set_var("BOXR_HOME", &state.home);
    }
    match crate::stop_container(&id, None) {
        Ok(_) => StatusCode::NO_CONTENT,
        Err(_) => StatusCode::NOT_FOUND,
    }
}

#[derive(Deserialize)]
struct CreateExecRequest {
    #[serde(rename = "Cmd")]
    cmd: Option<Vec<String>>,
    #[serde(rename = "Env")]
    env: Option<Vec<String>>,
    #[serde(rename = "WorkingDir")]
    working_dir: Option<String>,
    #[serde(rename = "User")]
    user: Option<String>,
    #[serde(rename = "Detach")]
    detach: Option<bool>,
}

async fn create_container_exec(
    State(state): State<DaemonState>,
    Path(id): Path<String>,
    Json(payload): Json<CreateExecRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let store = ContainerStore::with_home(state.home.clone());
    let c = store.find(&id).ok_or(StatusCode::NOT_FOUND)?;
    let exec_id = hex::encode(crate::storage::container_store::rand_id());

    let bundle = PathBuf::from(&c.bundle_path);
    let exec_info = serde_json::json!({
        "container_id": c.id,
        "cmd": payload.cmd.unwrap_or_default(),
        "env": payload.env.unwrap_or_default(),
        "working_dir": payload.working_dir,
        "user": payload.user,
        "detach": payload.detach.unwrap_or(false)
    });
    let _ = fs::write(
        bundle.join(format!("exec-{}.json", exec_id)),
        serde_json::to_string(&exec_info).unwrap(),
    );

    Ok(Json(serde_json::json!({
        "Id": exec_id
    })))
}

async fn start_exec_instance(
    State(state): State<DaemonState>,
    Path(exec_id): Path<String>,
) -> Result<String, StatusCode> {
    let store = ContainerStore::with_home(state.home.clone());
    for c in store.list() {
        let bundle = PathBuf::from(&c.bundle_path);
        let exec_file = bundle.join(format!("exec-{}.json", exec_id));
        if exec_file.exists() {
            if let Ok(content) = fs::read_to_string(&exec_file) {
                if let Ok(info) = serde_json::from_str::<serde_json::Value>(&content) {
                    let cmd: Vec<String> = info
                        .get("cmd")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_default();
                    let env: Vec<String> = info
                        .get("env")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_default();
                    let wd = info.get("working_dir").and_then(|v| v.as_str());
                    let user = info.get("user").and_then(|v| v.as_str());
                    let detach = info
                        .get("detach")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);

                    let log_path = bundle.join(format!("exec-{}.log", exec_id));
                    let code =
                        crate::runtime::exec_in_bundle(&bundle, &cmd, &env, wd, user, detach)
                            .unwrap_or(1);
                    let _ = fs::write(
                        bundle.join(format!("exec-{}.done", exec_id)),
                        code.to_string(),
                    );
                    let output = if log_path.exists() {
                        fs::read_to_string(&log_path).unwrap_or_default()
                    } else {
                        String::new()
                    };
                    return Ok(output);
                }
            }
        }
    }
    Err(StatusCode::NOT_FOUND)
}

async fn inspect_exec_instance(
    State(state): State<DaemonState>,
    Path(exec_id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let store = ContainerStore::with_home(state.home.clone());
    for c in store.list() {
        let bundle = PathBuf::from(&c.bundle_path);
        let exec_file = bundle.join(format!("exec-{}.json", exec_id));
        if exec_file.exists() {
            let done_file = bundle.join(format!("exec-{}.done", exec_id));
            let (running, exit_code) = if done_file.exists() {
                let code = fs::read_to_string(&done_file)
                    .ok()
                    .and_then(|s| s.trim().parse::<i32>().ok())
                    .unwrap_or(0);
                (false, serde_json::json!(code))
            } else {
                (true, serde_json::Value::Null)
            };
            return Ok(Json(serde_json::json!({
                "ID": exec_id,
                "Running": running,
                "ExitCode": exit_code,
                "ContainerID": c.id
            })));
        }
    }
    Err(StatusCode::NOT_FOUND)
}

async fn remove_container(State(state): State<DaemonState>, Path(id): Path<String>) -> StatusCode {
    let store = ContainerStore::with_home(state.home.clone());
    match store.remove(&id) {
        Ok(_) => StatusCode::NO_CONTENT,
        Err(_) => StatusCode::NOT_FOUND,
    }
}

async fn list_networks(State(state): State<DaemonState>) -> Json<serde_json::Value> {
    let store = NetworkStore::with_home(state.home.clone());
    Json(serde_json::to_value(store.list()).unwrap_or_default())
}

#[derive(Deserialize)]
struct CreateNetworkRequest {
    #[serde(rename = "Name")]
    name: String,
}

async fn create_network(
    State(state): State<DaemonState>,
    Json(payload): Json<CreateNetworkRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let store = NetworkStore::with_home(state.home.clone());
    match store.create(&payload.name, None, None) {
        Ok(net) => Ok(Json(serde_json::to_value(net).unwrap())),
        Err(_) => Err(StatusCode::BAD_REQUEST),
    }
}

async fn list_volumes(State(state): State<DaemonState>) -> Json<serde_json::Value> {
    let store = VolumeStore::with_home(state.home.clone());
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
    State(state): State<DaemonState>,
    Json(payload): Json<CreateVolumeRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let store = VolumeStore::with_home(state.home.clone());
    match store.create(payload.name.as_deref(), None) {
        Ok(vol) => Ok(Json(serde_json::to_value(vol).unwrap())),
        Err(_) => Err(StatusCode::BAD_REQUEST),
    }
}

async fn inspect_container(
    State(state): State<DaemonState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let store = ContainerStore::with_home(state.home.clone());
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

async fn inspect_image(
    State(state): State<DaemonState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let store = ImageStore::with_home(state.home.clone());
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
    let _ = crate::stop_container(&id, None);
    match crate::start_container(&id).await {
        Ok(_) => StatusCode::NO_CONTENT,
        Err(_) => StatusCode::NOT_FOUND,
    }
}

async fn kill_container(State(state): State<DaemonState>, Path(id): Path<String>) -> StatusCode {
    let store = ContainerStore::with_home(state.home.clone());
    if let Some(c) = store.find(&id) {
        let _ = crate::runtime::kill::ContainerKiller::kill(&c, None);
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

async fn wait_container(
    State(state): State<DaemonState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let store = ContainerStore::with_home(state.home.clone());
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

async fn get_container_logs(
    State(state): State<DaemonState>,
    Path(id): Path<String>,
) -> Result<String, StatusCode> {
    let store = ContainerStore::with_home(state.home.clone());
    let c = store.find(&id).ok_or(StatusCode::NOT_FOUND)?;
    let log_path = PathBuf::from(&c.bundle_path).join("logs.txt");
    if log_path.exists() {
        fs::read_to_string(&log_path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
    } else {
        Ok(String::new())
    }
}

async fn prune_containers_endpoint(State(state): State<DaemonState>) -> Json<serde_json::Value> {
    let c_store = ContainerStore::with_home(state.home.clone());
    let containers = c_store.list();
    let mut deleted = Vec::new();
    for c in containers {
        if !matches!(c.status, ContainerStatus::Running) {
            let _ = c_store.remove(&c.id);
            deleted.push(c.id);
        }
    }
    Json(serde_json::json!({
        "ContainersDeleted": deleted,
        "SpaceReclaimed": 0
    }))
}

async fn prune_images_endpoint(State(state): State<DaemonState>) -> Json<serde_json::Value> {
    let i_store = ImageStore::with_home(state.home.clone());
    let c_store = ContainerStore::with_home(state.home.clone());
    let images = i_store.list();
    let containers = c_store.list();
    let used_images: std::collections::HashSet<String> =
        containers.iter().map(|c| c.image.clone()).collect();

    let mut deleted = Vec::new();
    for img in images {
        let tag = format!("{}:{}", img.reference, img.tag);
        let is_used = used_images.contains(&tag)
            || used_images.contains(&img.reference)
            || used_images.contains(&img.id);
        if !is_used {
            let _ = i_store.remove(&img.id);
            deleted.push(serde_json::json!({ "Deleted": img.id }));
        }
    }
    Json(serde_json::json!({
        "ImagesDeleted": deleted,
        "SpaceReclaimed": 0
    }))
}

async fn prune_volumes_endpoint(State(state): State<DaemonState>) -> Json<serde_json::Value> {
    let store = VolumeStore::with_home(state.home.clone());
    let pruned = store.prune().unwrap_or_default();
    Json(serde_json::json!({
        "VolumesDeleted": pruned,
        "SpaceReclaimed": 0
    }))
}

async fn prune_networks_endpoint(State(state): State<DaemonState>) -> Json<serde_json::Value> {
    let store = NetworkStore::with_home(state.home.clone());
    let mut deleted = Vec::new();
    for net in store.list() {
        if net.name != NetworkStore::DEFAULT_NETWORK && net.containers.is_empty() {
            let _ = store.remove(&net.name);
            deleted.push(net.name);
        }
    }
    Json(serde_json::json!({
        "NetworksDeleted": deleted
    }))
}

async fn inspect_network(
    State(state): State<DaemonState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let store = NetworkStore::with_home(state.home.clone());
    let net = store.find(&id).ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(
        serde_json::to_value(net).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
    ))
}

async fn remove_network(State(state): State<DaemonState>, Path(id): Path<String>) -> StatusCode {
    let store = NetworkStore::with_home(state.home.clone());
    match store.remove(&id) {
        Ok(_) => StatusCode::NO_CONTENT,
        Err(_) => StatusCode::NOT_FOUND,
    }
}

async fn inspect_volume(
    State(state): State<DaemonState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let store = VolumeStore::with_home(state.home.clone());
    let vol = store.find(&name).ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(
        serde_json::to_value(vol).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
    ))
}

async fn remove_volume(State(state): State<DaemonState>, Path(name): Path<String>) -> StatusCode {
    let store = VolumeStore::with_home(state.home.clone());
    match store.remove(&name) {
        Ok(_) => StatusCode::NO_CONTENT,
        Err(_) => StatusCode::NOT_FOUND,
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

    #[tokio::test]
    async fn test_daemon_prune_and_crud_endpoints() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().to_path_buf();
        let base_home = boxr_home();
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
        let state = DaemonState { home: home.clone() };
        let app = create_router(state);

        // Test POST /containers/prune
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1.45/containers/prune")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Test POST /images/prune
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1.45/images/prune")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Test POST /volumes/prune
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1.45/volumes/prune")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Test POST /networks/prune
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1.45/networks/prune")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Test POST /containers/create
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1.45/containers/create?name=daemon-test-box")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"Image":"alpine:latest","Cmd":["echo","hello"]}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let created_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let cont_id = created_json.get("Id").unwrap().as_str().unwrap();

        // Test POST /containers/{id}/exec
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1.45/containers/{}/exec", cont_id))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"Cmd":["echo","exec-test"]}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let exec_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let exec_id = exec_json.get("Id").unwrap().as_str().unwrap();

        // Test GET /exec/{id}/json while still running
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/v1.45/exec/{}/json", exec_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let inspect_val: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(inspect_val.get("Running").unwrap().as_bool(), Some(true));
        assert!(inspect_val.get("ExitCode").unwrap().is_null());

        // Cleanup created container
        let _ = crate::remove_container(cont_id, true);
    }
}
