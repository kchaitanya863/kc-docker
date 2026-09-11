mod cli;
mod oci;
mod runtime;
mod storage;

use anyhow::Result;
use chrono::Utc;
use clap::Parser;
use cli::{Cli, Commands, PsArgs, RunArgs, SpecArgs};
use oci::distribution::RegistryClient;
use oci::image::unpack_layer;
use oci::reference::ImageReference;
use oci::runtime::Spec;
use runtime::execute_bundle;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use storage::{
    ensure_directories, ContainerRecord, ContainerStatus, ContainerStore, ImageRecord, ImageStore,
};

#[tokio::main]
async fn main() -> Result<()> {
    ensure_directories()?;
    let cli = Cli::parse();

    match cli.command {
        Commands::Pull(args) => {
            pull_image(&args.image).await?;
        }
        Commands::Run(args) => {
            let code = run_container(args).await?;
            std::process::exit(code);
        }
        Commands::Images => {
            list_images()?;
        }
        Commands::Ps(args) => {
            list_containers(args)?;
        }
        Commands::Rm(args) => {
            remove_container(&args.container)?;
        }
        Commands::Rmi(args) => {
            remove_image(&args.image)?;
        }
        Commands::Spec(args) => {
            generate_spec(args)?;
        }
    }

    Ok(())
}

async fn pull_image(image_str: &str) -> Result<ImageRecord> {
    let reference = ImageReference::parse(image_str)?;
    let mut client = RegistryClient::new();

    println!("Pulling from {}", reference.display_name());

    let (manifest, manifest_digest) = client.fetch_manifest(&reference).await?;
    let short_digest = if manifest_digest.len() > 19 {
        &manifest_digest[..19]
    } else {
        &manifest_digest
    };
    println!("Manifest: {}", short_digest);

    let config = client.fetch_config(&reference, &manifest.config).await?;

    let home = storage::boxr_home();
    let layers_dir = home.join("layers");
    let mut total_size = 0i64;

    for layer_desc in &manifest.layers {
        total_size += layer_desc.size;
        let safe_name = layer_desc.digest.replace(':', "_");
        let layer_file = layers_dir.join(format!("{}.tar", safe_name));

        client
            .download_blob_to_file(&reference, layer_desc, &layer_file)
            .await?;
    }

    // Prepare image rootfs directory
    let safe_manifest = manifest_digest.replace(':', "_");
    let image_dir = home.join("images").join(&safe_manifest);
    let rootfs_dir = image_dir.join("rootfs");

    if rootfs_dir.exists() {
        let _ = fs::remove_dir_all(&rootfs_dir);
    }
    fs::create_dir_all(&rootfs_dir)?;

    println!("Extracting image layers to rootfs...");
    for layer_desc in &manifest.layers {
        let safe_name = layer_desc.digest.replace(':', "_");
        let layer_file = layers_dir.join(format!("{}.tar", safe_name));
        unpack_layer(&layer_file, &rootfs_dir)?;
    }

    let image_id = if manifest_digest.starts_with("sha256:") {
        &manifest_digest[7..19]
    } else if manifest_digest.len() > 12 {
        &manifest_digest[..12]
    } else {
        &manifest_digest
    };

    let record = ImageRecord {
        id: image_id.to_string(),
        reference: reference.repository.clone(),
        tag: reference.tag.clone(),
        manifest_digest: manifest_digest.clone(),
        config_digest: manifest.config.digest.clone(),
        size_bytes: total_size,
        created_at: Utc::now(),
        rootfs_path: rootfs_dir.to_string_lossy().to_string(),
        config,
    };

    let store = ImageStore::new();
    store.add(record.clone())?;

    println!("Digest: {}", manifest_digest);
    println!("Status: Downloaded image for {}", reference.display_name());
    Ok(record)
}

async fn run_container(args: RunArgs) -> Result<i32> {
    let image_store = ImageStore::new();
    let image_record = match image_store.find(&args.image) {
        Some(record) => record,
        None => {
            println!("Unable to find image '{}' locally", args.image);
            pull_image(&args.image).await?
        }
    };

    // Generate container ID and name
    let random_bytes: [u8; 6] = rand_bytes();
    let container_id = hex::encode(random_bytes);
    let container_name = args.name.unwrap_or_else(|| format!("boxr-{}", &container_id[..6]));

    let home = storage::boxr_home();
    let bundle_dir = home.join("containers").join(&container_id);
    let container_rootfs = bundle_dir.join("rootfs");

    fs::create_dir_all(&bundle_dir)?;

    // Clone/copy base rootfs into container bundle rootfs
    let base_rootfs = PathBuf::from(&image_record.rootfs_path);
    copy_dir_recursive(&base_rootfs, &container_rootfs)?;

    // Build OCI Runtime Spec
    let cmd_override = if !args.command.is_empty() {
        Some(args.command.as_slice())
    } else {
        None
    };

    let env_override = if !args.env.is_empty() {
        Some(args.env.as_slice())
    } else {
        None
    };

    let spec = Spec::new_default(
        image_record.config.config.as_ref(),
        cmd_override,
        env_override,
    );

    spec.save_to_bundle(&bundle_dir)?;

    // Register container in store
    let container_store = ContainerStore::new();
    let record = ContainerRecord {
        id: container_id.clone(),
        name: container_name.clone(),
        image: format!("{}:{}", image_record.reference, image_record.tag),
        command: spec.process.args.clone(),
        created_at: Utc::now(),
        status: ContainerStatus::Running,
        bundle_path: bundle_dir.to_string_lossy().to_string(),
    };
    container_store.add(record)?;

    // Execute the container
    let exit_code = execute_bundle(&bundle_dir, &spec)?;

    if args.rm {
        let _ = container_store.remove(&container_id);
    } else {
        let _ = container_store.update_status(&container_id, ContainerStatus::Exited(exit_code));
    }

    Ok(exit_code)
}

fn list_images() -> Result<()> {
    let store = ImageStore::new();
    let images = store.list();

    println!("{:<28} {:<12} {:<16} {:<24} {:<10}", "REPOSITORY", "TAG", "IMAGE ID", "CREATED", "SIZE");

    for img in images {
        let size_mb = (img.size_bytes as f64) / (1024.0 * 1024.0);
        let size_str = if size_mb < 1.0 {
            format!("{:.1} KB", (img.size_bytes as f64) / 1024.0)
        } else {
            format!("{:.2} MB", size_mb)
        };

        println!(
            "{:<28} {:<12} {:<16} {:<24} {:<10}",
            img.reference,
            img.tag,
            img.id,
            img.created_at.format("%Y-%m-%d %H:%M:%S"),
            size_str
        );
    }

    Ok(())
}

fn list_containers(args: PsArgs) -> Result<()> {
    let store = ContainerStore::new();
    let containers = store.list();

    println!("{:<14} {:<24} {:<20} {:<20} {:<16} {:<16}", "CONTAINER ID", "IMAGE", "COMMAND", "CREATED", "STATUS", "NAMES");

    for c in containers {
        if !args.all && !matches!(c.status, ContainerStatus::Running) {
            continue;
        }

        let cmd_display = if c.command.is_empty() {
            "".to_string()
        } else {
            format!("\"{}\"", c.command.join(" "))
        };
        let truncated_cmd = if cmd_display.len() > 18 {
            format!("{}...", &cmd_display[..15])
        } else {
            cmd_display
        };

        println!(
            "{:<14} {:<24} {:<20} {:<20} {:<16} {:<16}",
            &c.id[..12.min(c.id.len())],
            c.image,
            truncated_cmd,
            c.created_at.format("%Y-%m-%d %H:%M:%S"),
            c.status.to_string(),
            c.name
        );
    }

    Ok(())
}

fn remove_container(container: &str) -> Result<()> {
    let store = ContainerStore::new();
    let removed = store.remove(container)?;
    println!("{}", removed.id);
    Ok(())
}

fn remove_image(image: &str) -> Result<()> {
    let store = ImageStore::new();
    let removed = store.remove(image)?;
    println!("Untagged: {}:{}", removed.reference, removed.tag);
    println!("Deleted: {}", removed.id);
    Ok(())
}

fn generate_spec(args: SpecArgs) -> Result<()> {
    let bundle_path = args
        .bundle
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));

    fs::create_dir_all(&bundle_path)?;
    let spec = Spec::new_default(None, None, None);
    spec.save_to_bundle(&bundle_path)?;

    let config_file = bundle_path.join("config.json");
    let content = fs::read_to_string(&config_file)?;
    println!("{}", content);
    Ok(())
}

fn rand_bytes() -> [u8; 6] {
    use std::time::SystemTime;
    let mut bytes = [0u8; 6];
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let pid = std::process::id();
    let combined = now ^ ((pid as u128) << 32);
    let mut hasher = Sha256::new();
    hasher.update(combined.to_le_bytes());
    let hash = hasher.finalize();
    bytes.copy_from_slice(&hash[..6]);
    bytes
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());

        if ty.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else if ty.is_symlink() {
            if let Ok(link_target) = fs::read_link(&from) {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::symlink;
                    let _ = symlink(link_target, &to);
                }
            }
        } else {
            let _ = fs::copy(&from, &to);
        }
    }
    Ok(())
}

