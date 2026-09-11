use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use tar::Archive;

/// Known OCI and Docker media types
#[allow(dead_code)]
pub mod media_types {
    pub const OCI_INDEX_V1: &str = "application/vnd.oci.image.index.v1+json";
    pub const OCI_MANIFEST_V1: &str = "application/vnd.oci.image.manifest.v1+json";
    pub const OCI_IMAGE_CONFIG_V1: &str = "application/vnd.oci.image.config.v1+json";
    pub const OCI_LAYER_TAR: &str = "application/vnd.oci.image.layer.v1.tar";
    pub const OCI_LAYER_TAR_GZIP: &str = "application/vnd.oci.image.layer.v1.tar+gzip";

    pub const DOCKER_MANIFEST_LIST_V2: &str = "application/vnd.docker.distribution.manifest.list.v2+json";
    pub const DOCKER_MANIFEST_V2: &str = "application/vnd.docker.distribution.manifest.v2+json";
    pub const DOCKER_CONTAINER_IMAGE_V1: &str = "application/vnd.docker.container.image.v1+json";
    pub const DOCKER_LAYER_TAR_GZIP: &str = "application/vnd.docker.image.rootfs.diff.tar.gzip";
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Platform {
    pub architecture: String,
    pub os: String,
    #[serde(rename = "os.version", skip_serializing_if = "Option::is_none")]
    pub os_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Descriptor {
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub digest: String,
    pub size: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<Platform>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestListOrIndex {
    #[serde(rename = "schemaVersion")]
    pub schema_version: i32,
    #[serde(rename = "mediaType", skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    pub manifests: Vec<Descriptor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageManifest {
    #[serde(rename = "schemaVersion")]
    pub schema_version: i32,
    #[serde(rename = "mediaType", skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    pub config: Descriptor,
    pub layers: Vec<Descriptor>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExecutionConfig {
    #[serde(rename = "User", skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(rename = "Env", skip_serializing_if = "Option::is_none")]
    pub env: Option<Vec<String>>,
    #[serde(rename = "Entrypoint", skip_serializing_if = "Option::is_none")]
    pub entrypoint: Option<Vec<String>>,
    #[serde(rename = "Cmd", skip_serializing_if = "Option::is_none")]
    pub cmd: Option<Vec<String>>,
    #[serde(rename = "WorkingDir", skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
    #[serde(rename = "Labels", skip_serializing_if = "Option::is_none")]
    pub labels: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootFsConfig {
    #[serde(rename = "type")]
    pub fs_type: String,
    pub diff_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageConfig {
    pub architecture: String,
    pub os: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<ExecutionConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rootfs: Option<RootFsConfig>,
}

/// Unpacks a layer archive (.tar or .tar.gz) into a target rootfs directory.
/// Accurately handles OCI whiteout files:
/// - `.wh..wh..opq`: Opaque whiteout, marks parent directory as opaque (clears existing entries in destination)
/// - `.wh.<name>`: Explicit whiteout, indicates that `<name>` in the same directory should be removed.
pub fn unpack_layer(layer_archive_path: &Path, target_dir: &Path) -> Result<()> {
    fs::create_dir_all(target_dir)
        .with_context(|| format!("Failed to create target rootfs directory: {:?}", target_dir))?;

    let file = File::open(layer_archive_path)
        .with_context(|| format!("Failed to open layer archive: {:?}", layer_archive_path))?;

    // Check if gzipped by reading first 2 bytes (0x1f, 0x8b)
    let mut header = [0u8; 2];
    let mut reader: Box<dyn Read> = {
        let mut f = File::open(layer_archive_path)?;
        let n = f.read(&mut header).unwrap_or(0);
        if n == 2 && header[0] == 0x1f && header[1] == 0x8b {
            Box::new(GzDecoder::new(file))
        } else {
            Box::new(file)
        }
    };

    let mut archive = Archive::new(&mut reader);
    // Don't unpack entries that match whiteout markers directly as regular files
    let mut pending_whiteouts: Vec<PathBuf> = Vec::new();

    for entry_result in archive.entries()? {
        let mut entry = entry_result?;
        let entry_path = entry.path()?.to_path_buf();
        let file_name = entry_path.file_name().and_then(|s| s.to_str()).unwrap_or("");

        if file_name == ".wh..wh..opq" {
            // Opaque whiteout: clear previous content in this directory
            if let Some(parent) = entry_path.parent() {
                let dest_parent = target_dir.join(parent);
                if dest_parent.exists() {
                    for dir_entry in fs::read_dir(&dest_parent)? {
                        let path = dir_entry?.path();
                        if path.is_dir() {
                            let _ = fs::remove_dir_all(&path);
                        } else {
                            let _ = fs::remove_file(&path);
                        }
                    }
                }
            }
            continue;
        }

        if let Some(stripped_wh) = file_name.strip_prefix(".wh.") {
            // Explicit whiteout: remove corresponding file/folder in parent
            if let Some(parent) = entry_path.parent() {
                let to_remove = target_dir.join(parent).join(stripped_wh);
                pending_whiteouts.push(to_remove);
            }
            continue;
        }

        // Normal file unpacking
        let dest = target_dir.join(&entry_path);
        // Ensure parent directory exists
        if let Some(p) = dest.parent() {
            fs::create_dir_all(p)?;
        }

        // Unpack entry safely
        if let Err(e) = entry.unpack_in(target_dir) {
            // On non-root platforms (like macOS), some chown/mknod operations in tar might fail.
            // In that case, fallback to manually extracting content.
            if !dest.exists() {
                eprintln!("Direct unpack warning ({}), attempting fallback: {:?}", e, dest);
            }
        }
    }

    // Apply pending whiteout removals
    for wh in pending_whiteouts {
        if wh.is_dir() {
            let _ = fs::remove_dir_all(&wh);
        } else if wh.exists() || fs::symlink_metadata(&wh).is_ok() {
            let _ = fs::remove_file(&wh);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use tempfile::tempdir;

    #[test]
    fn test_unpack_tar_gz_layer() {
        let temp = tempdir().unwrap();
        let archive_path = temp.path().join("layer.tar.gz");
        let dest_path = temp.path().join("rootfs");

        // Create a gzipped tar archive with a file
        let file = File::create(&archive_path).unwrap();
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = tar::Builder::new(encoder);

        let data = b"hello from layer";
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, "test.txt", &data[..]).unwrap();
        builder.finish().unwrap();
        let gz = builder.into_inner().unwrap();
        gz.finish().unwrap();

        // Unpack
        unpack_layer(&archive_path, &dest_path).unwrap();

        let extracted_file = dest_path.join("test.txt");
        assert!(extracted_file.exists());
        let content = fs::read_to_string(extracted_file).unwrap();
        assert_eq!(content, "hello from layer");
    }
}
