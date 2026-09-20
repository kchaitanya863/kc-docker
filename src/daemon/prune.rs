use super::DaemonState;
use crate::storage::{ContainerStatus, ContainerStore, ImageStore};
use crate::volume::VolumeStore;
use crate::network::NetworkStore;
use axum::{
    Json,
    extract::State,
};

pub async fn prune_containers_endpoint(State(state): State<DaemonState>) -> Json<serde_json::Value> {
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

pub async fn prune_images_endpoint(State(state): State<DaemonState>) -> Json<serde_json::Value> {
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
            let is_dangling = img.tag == "<none>" || img.reference.is_empty() || img.reference == "<none>";
            if is_dangling {
                let _ = i_store.remove(&img.id);
                deleted.push(serde_json::json!({ "Deleted": img.id }));
            }
        }
    }
    Json(serde_json::json!({
        "ImagesDeleted": deleted,
        "SpaceReclaimed": 0
    }))
}

pub async fn prune_volumes_endpoint(State(state): State<DaemonState>) -> Json<serde_json::Value> {
    let store = VolumeStore::with_home(state.home.clone());
    let pruned = store.prune().unwrap_or_default();
    Json(serde_json::json!({
        "VolumesDeleted": pruned,
        "SpaceReclaimed": 0
    }))
}

pub async fn prune_networks_endpoint(State(state): State<DaemonState>) -> Json<serde_json::Value> {
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
