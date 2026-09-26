use super::DaemonState;
use crate::storage::ImageStore;
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use chrono::Utc;
use serde::Deserialize;

pub async fn list_images(State(state): State<DaemonState>) -> Json<serde_json::Value> {
    let store = ImageStore::with_home(state.home.clone());
    let images = store.list();
    let val = serde_json::to_value(images).unwrap_or_default();
    Json(val)
}

#[derive(Deserialize)]
pub struct CreateImageQuery {
    #[serde(rename = "fromImage")]
    pub from_image: String,
}

pub async fn create_image(
    Query(params): Query<CreateImageQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match crate::pull_image(&params.from_image).await {
        Ok(rec) => Ok(Json(serde_json::to_value(rec).unwrap())),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn inspect_image(
    State(state): State<DaemonState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let store = ImageStore::with_home(state.home.clone());
    let img = store.find(&name).ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(serde_json::json!({
        "Id": format!("sha256:{}", img.id),
        "RepoTags": [format!("{}:{}", img.display_reference(), img.tag)],
        "Size": img.size_bytes,
        "Created": img.created_at.to_rfc3339(),
        "Architecture": img.config.architecture,
        "Os": img.config.os,
    })))
}

#[derive(Deserialize, Default)]
pub struct RemoveImageQuery {
    pub force: Option<bool>,
    pub noprune: Option<bool>,
}

pub async fn remove_image_endpoint(
    State(state): State<DaemonState>,
    Path(name): Path<String>,
    Query(query): Query<RemoveImageQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let img_store = ImageStore::with_home(state.home.clone());
    let img = img_store.find(&name).ok_or(StatusCode::NOT_FOUND)?;

    let force = query.force.unwrap_or(false);
    let no_prune = query.noprune.unwrap_or(false);

    if !force {
        let c_store = crate::storage::ContainerStore::with_home(state.home.clone());
        let containers = c_store.list();
        let full_name = img.qualified_name();
        let short_name = format!("{}:{}", img.reference, img.tag);

        for c in containers {
            if crate::storage::image_store::refs_equivalent(&c.image, &full_name)
                || crate::storage::image_store::refs_equivalent(&c.image, &short_name)
                || c.image == img.id
                || c.image.starts_with(&img.id)
            {
                return Err(StatusCode::CONFLICT);
            }
        }
    }

    let removed = if no_prune {
        img_store
            .remove_metadata_only(&name)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    } else {
        img_store
            .remove(&name)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    };

    Ok(Json(serde_json::json!([
        { "Untagged": format!("{}:{}", removed.reference, removed.tag) },
        { "Deleted": format!("sha256:{}", removed.id) }
    ])))
}

#[derive(Deserialize, Default)]
pub struct TagImageQuery {
    pub repo: Option<String>,
    pub tag: Option<String>,
}

pub async fn tag_image_endpoint(
    State(state): State<DaemonState>,
    Path(name): Path<String>,
    Query(query): Query<TagImageQuery>,
) -> StatusCode {
    let store = ImageStore::with_home(state.home.clone());
    let src = match store.find(&name) {
        Some(s) => s,
        None => return StatusCode::NOT_FOUND,
    };

    let repo = query.repo.unwrap_or_else(|| src.reference.clone());
    let tag = query.tag.unwrap_or_else(|| "latest".to_string());

    // Normalize the target so qualified spellings store canonical registry/repo/tag.
    let target = crate::oci::reference::ImageReference::parse(&format!("{}:{}", repo, tag))
        .unwrap_or(crate::oci::reference::ImageReference {
            registry: crate::oci::reference::ImageReference::DEFAULT_REGISTRY.to_string(),
            repository: repo.clone(),
            tag: tag.clone(),
            digest: None,
        });

    let record = crate::storage::ImageRecord {
        id: src.id.clone(),
        reference: target.repository,
        tag: target.tag,
        registry: target.registry,
        manifest_digest: src.manifest_digest.clone(),
        config_digest: src.config_digest.clone(),
        size_bytes: src.size_bytes,
        created_at: Utc::now(),
        rootfs_path: src.rootfs_path.clone(),
        config: src.config.clone(),
    };

    match store.add(record) {
        Ok(_) => StatusCode::CREATED,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

pub async fn get_image_history_endpoint(
    State(state): State<DaemonState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let store = ImageStore::with_home(state.home.clone());
    let img = store.find(&name).ok_or(StatusCode::NOT_FOUND)?;

    let mut history_items = Vec::new();
    if !img.config.history.is_empty() {
        let diff_count = img
            .config
            .rootfs
            .as_ref()
            .map(|r| r.diff_ids.len())
            .unwrap_or(1);
        let per_layer_size = if diff_count > 0 {
            img.size_bytes / diff_count as i64
        } else {
            img.size_bytes
        };
        for (i, h) in img.config.history.iter().rev().enumerate() {
            let id = if i == 0 {
                format!("sha256:{}", img.id)
            } else {
                "<missing>".to_string()
            };
            let created = img.created_at.timestamp();
            let created_by = h
                .created_by
                .as_deref()
                .unwrap_or("/bin/sh -c #(nop)")
                .to_string();
            let comment = h.comment.as_deref().unwrap_or("").to_string();
            let size = if h.empty_layer == Some(true) {
                0
            } else {
                per_layer_size
            };

            history_items.push(serde_json::json!({
                "Id": id,
                "Created": created,
                "CreatedBy": created_by,
                "Size": size,
                "Comment": comment,
                "Tags": if i == 0 { vec![format!("{}:{}", img.reference, img.tag)] } else { vec![] }
            }));
        }
    } else {
        history_items.push(serde_json::json!({
            "Id": format!("sha256:{}", img.id),
            "Created": img.created_at.timestamp(),
            "CreatedBy": "/bin/sh",
            "Size": img.size_bytes,
            "Comment": "",
            "Tags": [format!("{}:{}", img.reference, img.tag)]
        }));
    }

    Ok(Json(serde_json::Value::Array(history_items)))
}
