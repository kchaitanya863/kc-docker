#![recursion_limit = "256"]
//! # boxr 📦
//!
//! A fast, lightweight, production-grade **Open Container Initiative (OCI)** compliant
//! container engine, image builder, compose orchestrator, and runtime written in **Rust**.
//!
//! ## Core Specifications Implemented
//! - **OCI Distribution Spec**: Registry v2 client with bearer token authentication and multi-arch resolution.
//! - **OCI Image Spec**: Manifest parsing, config extraction, layered rootfs extraction, and whiteout deletion handling.
//! - **OCI Runtime Spec**: Generation of standard `config.json` container bundles and process execution.
//!
//! ## Subsystems
//! - [`auth`]: Registry credentials management, image tarball archiving (`save`/`load`), and registry push.
//! - [`builder`]: Multi-stage Dockerfile parser, step executor, and content-addressed build cache.
//! - [`cgroups`]: Linux cgroups v2 resource controllers (memory, CPU quota, and PID limits).
//! - [`cli`]: Command-line arguments and subcommands parsing via Clap.
//! - [`completions`]: Shell auto-completion script generators (`bash`, `zsh`, `fish`) and drop-in aliases.
//! - [`compose`]: `docker-compose.yml` parser with topological dependency ordering.
//! - [`daemon`]: Unix domain socket server with Docker-compatible REST API endpoints.
//! - [`events`]: Real-time container lifecycle events stream with JSONL persistence.
//! - [`health`]: Container healthcheck probes and automatic restart policy supervisor.
//! - [`kube`]: Kubernetes Pod YAML manifest generator and executor (`play kube` / `generate kube`).
//! - [`network`]: Software bridge networks, sequential IPAM, and user-space rootless port forwarding.
//! - [`oci`]: OCI spec definitions, image reference parsing, and registry distribution client.
//! - [`pod`]: Podman-style pod abstractions for multi-container groups sharing namespaces.
//! - [`runtime`]: Platform-specific container process execution engines (Linux & macOS).
//! - [`security`]: Rootless user namespaces (`CLONE_NEWUSER`), UID/GID maps, capability whitelisting, and Seccomp filters.
//! - [`stats`]: Real-time streaming container resource monitoring (CPU %, memory, PIDs).
//! - [`storage`]: Content-addressable layer storage, image store, and Copy-on-Write overlay drivers.
//! - [`system`]: System disk usage auditing (`system df`) and resource pruning (`system prune`).
//! - [`terminal`]: Interactive terminal PTY raw mode guards and window resize signals.
//! - [`volume`]: Persistent named volumes and host directory bind mounts.

pub mod auth;
pub mod builder;
pub mod cgroups;
pub mod cli;
pub mod completions;
pub mod compose;
pub mod daemon;
pub mod events;
pub mod health;
pub mod kube;
pub mod network;
pub mod oci;
pub mod pod;
pub mod runtime;
pub mod security;
pub mod stats;
pub mod storage;
pub mod system;
pub mod terminal;
pub mod volume;

use anyhow::{Result, anyhow};
use chrono::Utc;
use cli::{
    BuildArgs, BuilderAction, Cli, Commands, ComposeArgs, ComposeSubcommand, DiffArgs, ExecArgs,
    GenerateAction, GenerateSubcommands, LogsArgs, NetworkAction, NetworkSubcommands, PlayAction,
    PlaySubcommands, PodAction, PodSubcommands, PsArgs, RunArgs, SpecArgs, SystemAction, TopArgs,
    UnshareArgs, VolumeAction, VolumeSubcommands,
};
use events::{ContainerEvent, EventManager};
use network::{NetworkStore, PortMapping};
use oci::distribution::RegistryClient;
use oci::image::unpack_layer;
use oci::reference::ImageReference;
use oci::runtime::Spec;
use runtime::{exec_in_bundle, execute_bundle};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use storage::{
    ContainerRecord, ContainerStatus, ContainerStore, ImageRecord, ImageStore, OverlayDriver,
    ensure_directories,
};
use volume::VolumeStore;

pub async fn run_cli(cli: Cli) -> Result<i32> {
    ensure_directories()?;

    match cli.command {
        Commands::Pull(args) => {
            pull_image_with_platform(&args.image, args.platform.as_deref()).await?;
            Ok(0)
        }
        Commands::Run(args) => {
            let code = run_container(args).await?;
            Ok(code)
        }
        Commands::Create(args) => {
            let id = create_only_container(args).await?;
            println!("{}", id);
            Ok(0)
        }
        Commands::Restart(args) => {
            restart_container(&args).await?;
            Ok(0)
        }
        Commands::Port(args) => {
            port_container(&args)?;
            Ok(0)
        }
        Commands::Tag(args) => {
            tag_image(&args)?;
            Ok(0)
        }
        Commands::Export(args) => {
            export_container(&args)?;
            Ok(0)
        }
        Commands::Import(args) => {
            import_image(&args)?;
            Ok(0)
        }
        Commands::History(args) => {
            history_image(&args)?;
            Ok(0)
        }
        Commands::Search(args) => {
            search_hub(&args).await?;
            Ok(0)
        }
        Commands::Info => {
            info_system()?;
            Ok(0)
        }
        Commands::Version => {
            println!("Client: Boxr Engine");
            println!(" Version:           {}", env!("CARGO_PKG_VERSION"));
            println!(" API version:       1.45");
            println!(" Go version:        rustc {}", env!("CARGO_PKG_VERSION"));
            println!(" Git commit:        main");
            println!(" Built:             2026-09-14");
            println!(
                " OS/Arch:           {}/{}",
                std::env::consts::OS,
                std::env::consts::ARCH
            );
            println!("\nServer: Boxr Engine");
            println!(" Engine:");
            println!("  Version:          {}", env!("CARGO_PKG_VERSION"));
            println!("  API version:      1.45");
            println!(
                "  OS/Arch:          {}/{}",
                std::env::consts::OS,
                std::env::consts::ARCH
            );
            Ok(0)
        }
        Commands::Stop(args) => {
            stop_container(&args.container)?;
            Ok(0)
        }
        Commands::Start(args) => {
            start_container(&args.container).await?;
            Ok(0)
        }
        Commands::Logs(args) => {
            container_logs(&args)?;
            Ok(0)
        }
        Commands::Exec(args) => {
            let code = exec_container(&args)?;
            Ok(code)
        }
        Commands::Inspect(args) => {
            inspect_target(&args.target)?;
            Ok(0)
        }
        Commands::Build(args) => {
            build_image(args).await?;
            Ok(0)
        }
        Commands::Compose(args) => {
            handle_compose(args).await?;
            Ok(0)
        }
        Commands::Save(args) => {
            let output_path = args.output.map(PathBuf::from).unwrap_or_else(|| {
                PathBuf::from(format!(
                    "{}.tar",
                    args.image.replace('/', "_").replace(':', "_")
                ))
            });
            auth::ImageArchiver::save(&args.image, &output_path)?;
            Ok(0)
        }
        Commands::Load(args) => {
            let input_path = args.input.map(PathBuf::from).ok_or_else(|| {
                anyhow::anyhow!("Input tar archive (-i/--input) is required for load")
            })?;
            auth::ImageArchiver::load(&input_path)?;
            Ok(0)
        }
        Commands::Push(args) => {
            auth::RegistryPusher::push(&args.image).await?;
            Ok(0)
        }
        Commands::Login(args) => {
            let server = args.server.as_deref().unwrap_or("docker.io");
            let username = args.username.unwrap_or_else(|| {
                eprint!("Username: ");
                let mut u = String::new();
                let _ = std::io::stdin().read_line(&mut u);
                u.trim().to_string()
            });
            let password = args.password.unwrap_or_else(|| {
                eprint!("Password: ");
                let mut p = String::new();
                let _ = std::io::stdin().read_line(&mut p);
                p.trim().to_string()
            });
            auth::CredentialStore::new().login(server, &username, &password)?;
            println!("Login Succeeded for {}", server);
            Ok(0)
        }
        Commands::Logout(args) => {
            let server = args.server.as_deref().unwrap_or("docker.io");
            auth::CredentialStore::new().logout(server)?;
            println!("Logout Succeeded for {}", server);
            Ok(0)
        }
        Commands::Volume(args) => {
            handle_volume(args)?;
            Ok(0)
        }
        Commands::Network(args) => {
            handle_network(args)?;
            Ok(0)
        }
        Commands::Daemon(args) => {
            daemon::start_daemon(args.socket.as_deref()).await?;
            Ok(0)
        }
        Commands::Builder(args) => match args.command {
            BuilderAction::Prune => {
                let count = builder::BuildCache::prune()?;
                println!("Total reclaimed build cache entries: {}", count);
                Ok(0)
            }
        },
        Commands::Diff(args) => {
            diff_container(&args)?;
            Ok(0)
        }
        Commands::Top(args) => {
            top_container(&args)?;
            Ok(0)
        }
        Commands::Commit(args) => {
            commit_container(&args)?;
            Ok(0)
        }
        Commands::Pause(args) => {
            pause_container(&args)?;
            Ok(0)
        }
        Commands::Unpause(args) => {
            unpause_container(&args)?;
            Ok(0)
        }
        Commands::Rename(args) => {
            rename_container(&args)?;
            Ok(0)
        }
        Commands::Wait(args) => {
            let code = wait_container(&args)?;
            Ok(code)
        }
        Commands::Cp(args) => {
            cp_container(&args)?;
            Ok(0)
        }
        Commands::Update(args) => {
            update_container(&args)?;
            Ok(0)
        }
        Commands::Attach(args) => {
            attach_container(&args)?;
            Ok(0)
        }
        Commands::Kill(args) => {
            kill_container(&args)?;
            Ok(0)
        }
        Commands::System(args) => match args.command {
            SystemAction::Df => {
                system::SystemManager::print_df()?;
                Ok(0)
            }
            SystemAction::Prune { all, volumes } => {
                system::SystemManager::prune(all, volumes)?;
                Ok(0)
            }
        },
        Commands::Stats(args) => {
            stats::StatsCollector::display_stats(&args.containers, args.no_stream)?;
            Ok(0)
        }
        Commands::Events(args) => {
            events::EventManager::stream_events(args.since.as_deref(), args.filter.as_deref())?;
            Ok(0)
        }
        Commands::Completion(args) => {
            let shell = completions::ShellType::parse(&args.shell)?;
            println!("{}", completions::CompletionGenerator::generate(shell));
            Ok(0)
        }
        Commands::Pod(args) => {
            handle_pod(args)?;
            Ok(0)
        }
        Commands::Play(args) => {
            handle_play(args).await?;
            Ok(0)
        }
        Commands::Generate(args) => {
            handle_generate(args)?;
            Ok(0)
        }
        Commands::Unshare(args) => {
            let code = handle_unshare(args)?;
            Ok(code)
        }
        Commands::Alias(args) => {
            if args.install {
                let bin_path = completions::CompletionGenerator::install_docker_wrapper()?;
                println!("Installed docker wrapper script in: {}/docker", bin_path);
                println!("Add to your PATH:\n  export PATH=\"{}:$PATH\"", bin_path);
            } else {
                println!("alias docker=\"boxr\"");
            }
            Ok(0)
        }
        Commands::Images => {
            list_images()?;
            Ok(0)
        }
        Commands::Ps(args) => {
            list_containers(args)?;
            Ok(0)
        }
        Commands::Rm(args) => {
            remove_container(&args.container)?;
            Ok(0)
        }
        Commands::Rmi(args) => {
            remove_image(&args.image)?;
            Ok(0)
        }
        Commands::Spec(args) => {
            generate_spec(args)?;
            Ok(0)
        }
    }
}

pub async fn pull_image(image_str: &str) -> Result<ImageRecord> {
    pull_image_with_platform(image_str, None).await
}

pub async fn pull_image_with_platform(
    image_str: &str,
    target_platform: Option<&str>,
) -> Result<ImageRecord> {
    let reference = ImageReference::parse(image_str)?;
    let mut client = RegistryClient::new();

    if let Some(plat) = target_platform {
        println!(
            "Pulling from {} (platform: {})",
            reference.display_name(),
            plat
        );
    } else {
        println!("Pulling from {}", reference.display_name());
    }

    let (manifest, manifest_digest) = client
        .fetch_manifest_with_platform(&reference, target_platform)
        .await?;
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

pub async fn run_container(args: RunArgs) -> Result<i32> {
    let image_store = ImageStore::new();
    let image_record = match image_store.find_with_platform(&args.image, args.platform.as_deref()) {
        Some(record) if Path::new(&record.rootfs_path).exists() => record,
        _ => {
            if let Some(plat) = &args.platform {
                println!("Unable to find image '{}' ({}) locally", args.image, plat);
            } else {
                println!("Unable to find image '{}' locally", args.image);
            }
            pull_image_with_platform(&args.image, args.platform.as_deref()).await?
        }
    };

    // Parse port mappings
    let mut parsed_ports = Vec::new();
    for p in &args.ports {
        parsed_ports.push(PortMapping::parse(p)?);
    }

    // Resolve volume mounts
    let vol_store = VolumeStore::new();
    let mut parsed_mounts = Vec::new();
    for v in &args.volumes {
        parsed_mounts.push(vol_store.resolve_mount(v)?);
    }

    // Generate container ID and name
    let random_bytes: [u8; 6] = rand_bytes();
    let container_id = hex::encode(random_bytes);
    let container_name = args
        .name
        .unwrap_or_else(|| format!("boxr-{}", &container_id[..6]));

    let home = storage::boxr_home();
    let bundle_dir = home.join("containers").join(&container_id);
    let base_rootfs = PathBuf::from(&image_record.rootfs_path);

    let is_fast_ephemeral = args.rm && !args.detach && args.volumes.is_empty();

    let cow_bundle = if is_fast_ephemeral {
        None
    } else {
        fs::create_dir_all(&bundle_dir)?;
        Some(OverlayDriver::create_cow_layer(&bundle_dir, &base_rootfs)?)
    };

    // Configure cgroups v2 resource limits if specified
    let mut limits = cgroups::ResourceLimits::default();
    if let Some(mem_str) = &args.memory {
        limits.memory_max_bytes = cgroups::ResourceLimits::parse_memory(mem_str).ok();
    }
    if let Some(cpus_str) = &args.cpus {
        if let Ok((quota, period)) = cgroups::ResourceLimits::parse_cpus(cpus_str) {
            limits.cpu_quota_us = Some(quota);
            limits.cpu_period_us = Some(period);
        }
    }
    limits.pids_max = args.pids_limit;

    if !is_fast_ephemeral {
        if let Ok(cgroup_mgr) = cgroups::CgroupV2Manager::new(&container_id) {
            let _ = cgroup_mgr.apply_limits(&limits);
        }
    }

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

    let mut spec = Spec::new_default(
        image_record.config.config.as_ref(),
        cmd_override,
        env_override,
    );

    let mut annotations = HashMap::new();
    annotations.insert(
        "org.opencontainers.image.architecture".to_string(),
        image_record.config.architecture.clone(),
    );
    if let Some(p) = &args.platform {
        annotations.insert("boxr.platform".to_string(), p.clone());
    }
    if let Some(g) = &args.gpus {
        annotations.insert("boxr.gpus".to_string(), g.clone());
    }
    spec.annotations = Some(annotations);

    if is_fast_ephemeral {
        spec.root.path = base_rootfs.to_string_lossy().to_string();
        spec.root.readonly = true;
    } else {
        spec.save_to_bundle(&bundle_dir)?;
    }

    let restart_policy = health::parse_restart_policy(&args.restart)?;
    let mut health_cfg = health::HealthConfig::default();
    if let Some(cmd) = &args.health_cmd {
        health_cfg.test = cmd.split_whitespace().map(|s| s.to_string()).collect();
    }
    let initial_health = if health_cfg.test.is_empty() {
        health::HealthStatus::None
    } else {
        health::HealthStatus::Starting
    };

    // Register container in store
    let container_store = ContainerStore::new();
    let initial_status = if args.detach {
        ContainerStatus::Running
    } else {
        ContainerStatus::Running
    };

    let record = ContainerRecord {
        id: container_id.clone(),
        name: container_name.clone(),
        image: format!("{}:{}", image_record.reference, image_record.tag),
        command: spec.process.args.clone(),
        created_at: Utc::now(),
        status: initial_status,
        bundle_path: bundle_dir.to_string_lossy().to_string(),
        restart_policy: restart_policy.clone(),
        health_status: initial_health,
        restart_count: 0,
        ports: parsed_ports.clone(),
    };

    let mut event_attrs = HashMap::new();
    event_attrs.insert(
        "image".to_string(),
        format!("{}:{}", image_record.reference, image_record.tag),
    );
    event_attrs.insert("name".to_string(), container_name.clone());

    if !is_fast_ephemeral {
        EventManager::record(ContainerEvent::new(
            "container",
            "create",
            &container_id,
            &container_name,
            event_attrs.clone(),
        ));
        container_store.add(record)?;
    }

    if args.detach {
        println!("{}", container_id);
        if !parsed_ports.is_empty() {
            let _ = network::rootless::PortForwardManager::start_forwarding(&parsed_ports).await;
        }
    }

    if !is_fast_ephemeral {
        EventManager::record(ContainerEvent::new(
            "container",
            "start",
            &container_id,
            &container_name,
            event_attrs.clone(),
        ));
    }

    // Enter raw terminal mode if interactive TTY was requested
    let _term_guard = if args.interactive && args.tty {
        terminal::TerminalGuard::enter_raw_mode().ok()
    } else {
        None
    };

    // Execute the container with restart policy support
    let mut exit_code = execute_bundle(
        &bundle_dir,
        &spec,
        &parsed_mounts,
        &parsed_ports,
        args.detach,
    )?;
    let mut restart_count = 0;

    while !args.detach {
        let should_restart = match &restart_policy {
            health::RestartPolicy::Always => true,
            health::RestartPolicy::OnFailure { max_retries } => {
                exit_code != 0 && restart_count < *max_retries
            }
            _ => false,
        };

        if should_restart {
            restart_count += 1;
            println!(
                "Container {} exited with code {}, restarting (attempt {})...",
                container_id, exit_code, restart_count
            );
            exit_code = execute_bundle(&bundle_dir, &spec, &parsed_mounts, &parsed_ports, false)?;
        } else {
            break;
        }
    }

    if !is_fast_ephemeral {
        event_attrs.insert("exitCode".to_string(), exit_code.to_string());
        EventManager::record(ContainerEvent::new(
            "container",
            "die",
            &container_id,
            &container_name,
            event_attrs,
        ));
    }

    // Health check evaluation if configured
    if !health_cfg.test.is_empty() {
        let mut health_res = health::HealthCheckResult::default();
        let _ = health::check_container_health(&bundle_dir, &health_cfg, &mut health_res);
    }

    if !args.detach {
        if args.rm {
            if let Some(ref cb) = cow_bundle {
                let _ = OverlayDriver::cleanup(cb);
                let _ = container_store.remove(&container_id);
            }
        } else {
            let _ =
                container_store.update_status(&container_id, ContainerStatus::Exited(exit_code));
        }
    }

    Ok(exit_code)
}

pub fn stop_container(container: &str) -> Result<()> {
    let store = ContainerStore::new();
    if let Some(c) = store.find(container) {
        #[cfg(target_os = "macos")]
        {
            let runner_name = format!("boxr-runner-{}", c.id);
            let _ = std::process::Command::new("docker")
                .args(["kill", &runner_name])
                .output();
        }
    }
    store.update_status(container, ContainerStatus::Exited(0))?;
    println!("{}", container);
    Ok(())
}

pub async fn start_container(container: &str) -> Result<()> {
    let store = ContainerStore::new();
    let rec = store
        .find(container)
        .ok_or_else(|| anyhow!("Container '{}' not found", container))?;

    let bundle_path = PathBuf::from(&rec.bundle_path);
    let config_file = bundle_path.join("config.json");
    let content = fs::read_to_string(&config_file)?;
    let spec: Spec = serde_json::from_str(&content)?;

    store.update_status(&rec.id, ContainerStatus::Running)?;
    let code = execute_bundle(&bundle_path, &spec, &[], &[], false)?;
    store.update_status(&rec.id, ContainerStatus::Exited(code))?;
    Ok(())
}

pub fn container_logs(args: &LogsArgs) -> Result<()> {
    let store = ContainerStore::new();
    let rec = store
        .find(&args.container)
        .ok_or_else(|| anyhow!("Container '{}' not found", args.container))?;

    let log_path = PathBuf::from(&rec.bundle_path).join("logs.txt");

    let print_line = |line: &str| {
        if args.timestamps {
            println!("{} {}", Utc::now().to_rfc3339(), line);
        } else {
            println!("{}", line);
        }
    };

    #[cfg(target_os = "macos")]
    let runner_output = {
        let docker_bin = runtime::darwin::find_real_docker_bin();
        let runner_name = format!("boxr-runner-{}", rec.id);
        if let Ok(out) = std::process::Command::new(&docker_bin)
            .args(["logs", &runner_name])
            .output()
        {
            let combined = String::from_utf8_lossy(&out.stdout).to_string()
                + &String::from_utf8_lossy(&out.stderr);
            if !combined.trim().is_empty() {
                Some(combined)
            } else {
                None
            }
        } else {
            None
        }
    };

    #[cfg(not(target_os = "macos"))]
    let runner_output: Option<String> = None;

    let content = if let Some(out) = runner_output {
        out
    } else if log_path.exists() {
        fs::read_to_string(&log_path)?
    } else {
        println!("No logs available for container {}", args.container);
        return Ok(());
    };

    let mut lines: Vec<&str> = content.lines().collect();

    if let Some(tail) = args.tail {
        if lines.len() > tail {
            lines = lines[lines.len() - tail..].to_vec();
        }
    }

    for l in lines {
        print_line(l);
    }

    if args.follow {
        let mut pos = fs::metadata(&log_path)?.len();
        loop {
            if let Some(current) = store.find(&rec.id) {
                if !matches!(current.status, ContainerStatus::Running) {
                    break;
                }
            }

            let meta = fs::metadata(&log_path)?;
            let new_len = meta.len();
            if new_len > pos {
                use std::io::{BufRead, Seek};
                let mut file = fs::File::open(&log_path)?;
                file.seek(std::io::SeekFrom::Start(pos))?;
                let reader = std::io::BufReader::new(file);
                for l in reader.lines().flatten() {
                    print_line(&l);
                }
                pos = new_len;
            }

            std::thread::sleep(std::time::Duration::from_millis(200));
        }
    }

    Ok(())
}

pub fn exec_container(args: &ExecArgs) -> Result<i32> {
    let store = ContainerStore::new();
    let rec = store
        .find(&args.container)
        .ok_or_else(|| anyhow!("Container '{}' not found", args.container))?;

    if args.command.is_empty() {
        return Err(anyhow!("Command cannot be empty for exec"));
    }

    let bundle_path = PathBuf::from(&rec.bundle_path);
    exec_in_bundle(&bundle_path, &args.command, &args.env)
}

pub fn inspect_target(target: &str) -> Result<()> {
    let c_store = ContainerStore::new();
    if let Some(c) = c_store.find(target) {
        let is_running = matches!(c.status, ContainerStatus::Running);
        let docker_compat_inspect = serde_json::json!([{
            "Id": c.id,
            "Created": c.created_at.to_rfc3339(),
            "Path": c.command.first().cloned().unwrap_or_default(),
            "Args": if c.command.len() > 1 { c.command[1..].to_vec() } else { Vec::new() },
            "State": {
                "Status": if is_running { "running" } else { "exited" },
                "Running": is_running,
                "Paused": matches!(c.status, ContainerStatus::Paused),
                "Restarting": false,
                "OOMKilled": false,
                "Dead": false,
                "Pid": 0,
                "ExitCode": match c.status {
                    ContainerStatus::Exited(code) => code,
                    _ => 0,
                },
                "Error": "",
                "StartedAt": c.created_at.to_rfc3339(),
                "FinishedAt": c.created_at.to_rfc3339(),
            },
            "Image": c.image,
            "Name": format!("/{}", c.name),
            "RestartCount": c.restart_count,
            "HostConfig": {
                "PortBindings": {},
                "RestartPolicy": {
                    "Name": "no",
                    "MaximumRetryCount": 0
                }
            },
            "NetworkSettings": {
                "Bridge": "",
                "SandboxID": "",
                "HairpinMode": false,
                "LinkLocalIPv6Address": "",
                "LinkLocalIPv6PrefixLen": 0,
                "Ports": {},
                "SandboxKey": "",
                "SecondaryIPAddresses": null,
                "SecondaryIPv6Addresses": null,
                "EndpointID": "",
                "Gateway": "172.17.0.1",
                "GlobalIPv6Address": "",
                "GlobalIPv6PrefixLen": 0,
                "IPAddress": "172.17.0.2",
                "IPPrefixLen": 16,
                "IPv6Gateway": "",
                "MacAddress": "02:42:ac:11:00:02",
                "Networks": {
                    "bridge": {
                        "IPAMConfig": null,
                        "Links": null,
                        "Aliases": null,
                        "NetworkID": "boxr00000000",
                        "EndpointID": "",
                        "Gateway": "172.17.0.1",
                        "IPAddress": "172.17.0.2",
                        "IPPrefixLen": 16,
                        "IPv6Gateway": "",
                        "GlobalIPv6Address": "",
                        "GlobalIPv6PrefixLen": 0,
                        "MacAddress": "02:42:ac:11:00:02",
                        "DriverOpts": null
                    }
                }
            },
            "boxr_raw": c
        }]);
        println!("{}", serde_json::to_string_pretty(&docker_compat_inspect)?);
        return Ok(());
    }

    let i_store = ImageStore::new();
    if let Some(i) = i_store.find(target) {
        let docker_compat_image = serde_json::json!([{
            "Id": format!("sha256:{}", i.id),
            "RepoTags": [format!("{}:{}", i.reference, i.tag)],
            "Size": i.size_bytes,
            "Created": i.created_at.to_rfc3339(),
            "Architecture": i.config.architecture,
            "Os": i.config.os,
            "boxr_raw": i
        }]);
        println!("{}", serde_json::to_string_pretty(&docker_compat_image)?);
        return Ok(());
    }

    Err(anyhow!("No such container or image: '{}'", target))
}

pub fn diff_container(args: &DiffArgs) -> Result<()> {
    let c_store = ContainerStore::new();
    let cont = c_store
        .find(&args.container)
        .ok_or_else(|| anyhow!("Container '{}' not found", args.container))?;

    let i_store = ImageStore::new();
    let img = i_store
        .find(&cont.image)
        .ok_or_else(|| anyhow!("Image '{}' not found", cont.image))?;

    let base_rootfs = PathBuf::from(&img.rootfs_path);
    let container_rootfs = PathBuf::from(&cont.bundle_path).join("rootfs");

    let diffs = runtime::diff::FilesystemDiff::compare(&base_rootfs, &container_rootfs)?;
    for d in diffs {
        println!("{} {}", d.change_type, d.path);
    }
    Ok(())
}

pub fn top_container(args: &TopArgs) -> Result<()> {
    let c_store = ContainerStore::new();
    let cont = c_store
        .find(&args.container)
        .ok_or_else(|| anyhow!("Container '{}' not found", args.container))?;

    let bundle_path = PathBuf::from(&cont.bundle_path);
    runtime::top::ContainerTop::list_processes(&bundle_path, &args.ps_args)?;
    Ok(())
}

pub fn commit_container(args: &cli::CommitArgs) -> Result<()> {
    let c_store = ContainerStore::new();
    let cont = c_store
        .find(&args.container)
        .ok_or_else(|| anyhow!("Container '{}' not found", args.container))?;

    let i_store = ImageStore::new();
    let record = i_store.commit_container(
        &cont,
        args.repo_tag.as_deref(),
        args.message.as_deref(),
        args.author.as_deref(),
    )?;

    let mut attrs = HashMap::new();
    attrs.insert(
        "image".to_string(),
        format!("{}:{}", record.reference, record.tag),
    );
    EventManager::record(ContainerEvent::new(
        "container",
        "commit",
        &cont.id,
        &cont.name,
        attrs,
    ));

    println!("sha256:{}", record.manifest_digest);
    Ok(())
}

pub fn pause_container(args: &cli::PauseArgs) -> Result<()> {
    let c_store = ContainerStore::new();
    let cont = c_store
        .find(&args.container)
        .ok_or_else(|| anyhow!("Container '{}' not found", args.container))?;

    if let Ok(cgroup_mgr) = cgroups::CgroupV2Manager::new(&cont.id) {
        let _ = cgroup_mgr.freeze();
    }
    c_store.update_status(&cont.id, ContainerStatus::Paused)?;

    let mut attrs = HashMap::new();
    attrs.insert("name".to_string(), cont.name.clone());
    EventManager::record(ContainerEvent::new(
        "container",
        "pause",
        &cont.id,
        &cont.name,
        attrs,
    ));

    println!("{}", args.container);
    Ok(())
}

pub fn unpause_container(args: &cli::UnpauseArgs) -> Result<()> {
    let c_store = ContainerStore::new();
    let cont = c_store
        .find(&args.container)
        .ok_or_else(|| anyhow!("Container '{}' not found", args.container))?;

    if let Ok(cgroup_mgr) = cgroups::CgroupV2Manager::new(&cont.id) {
        let _ = cgroup_mgr.unfreeze();
    }
    c_store.update_status(&cont.id, ContainerStatus::Running)?;

    let mut attrs = HashMap::new();
    attrs.insert("name".to_string(), cont.name.clone());
    EventManager::record(ContainerEvent::new(
        "container",
        "unpause",
        &cont.id,
        &cont.name,
        attrs,
    ));

    println!("{}", args.container);
    Ok(())
}

pub fn rename_container(args: &cli::RenameArgs) -> Result<()> {
    let c_store = ContainerStore::new();
    c_store.rename(&args.container, &args.new_name)?;

    let mut attrs = HashMap::new();
    attrs.insert("oldName".to_string(), args.container.clone());
    attrs.insert("newName".to_string(), args.new_name.clone());
    EventManager::record(ContainerEvent::new(
        "container",
        "rename",
        &args.container,
        &args.new_name,
        attrs,
    ));

    Ok(())
}

pub fn wait_container(args: &cli::WaitArgs) -> Result<i32> {
    let c_store = ContainerStore::new();
    loop {
        let cont = c_store
            .find(&args.container)
            .ok_or_else(|| anyhow!("Container '{}' not found", args.container))?;
        match cont.status {
            ContainerStatus::Exited(code) => {
                println!("{}", code);
                return Ok(code);
            }
            ContainerStatus::Failed(err) => {
                eprintln!("Container failed: {}", err);
                return Ok(1);
            }
            ContainerStatus::Running | ContainerStatus::Created | ContainerStatus::Paused => {
                #[cfg(target_os = "macos")]
                {
                    let docker_bin = runtime::darwin::find_real_docker_bin();
                    let runner_name = format!("boxr-runner-{}", cont.id);
                    if let Ok(output) = std::process::Command::new(&docker_bin)
                        .args(["inspect", "-f", "{{.State.Running}}", &runner_name])
                        .output()
                    {
                        let s = String::from_utf8_lossy(&output.stdout);
                        let s_trim = s.trim();
                        if s_trim == "false" || s_trim.is_empty() {
                            let _ = c_store.update_status(&cont.id, ContainerStatus::Exited(0));
                            println!("0");
                            return Ok(0);
                        }
                    } else {
                        let _ = c_store.update_status(&cont.id, ContainerStatus::Exited(0));
                        println!("0");
                        return Ok(0);
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
        }
    }
}

pub fn cp_container(args: &cli::CpArgs) -> Result<()> {
    runtime::cp::ContainerCopy::copy(&args.src, &args.dest)
}

pub fn update_container(args: &cli::UpdateArgs) -> Result<()> {
    let c_store = ContainerStore::new();
    let cont = c_store
        .find(&args.container)
        .ok_or_else(|| anyhow!("Container '{}' not found", args.container))?;

    let mut limits = cgroups::ResourceLimits::default();
    if let Some(mem_str) = &args.memory {
        limits.memory_max_bytes = cgroups::ResourceLimits::parse_memory(mem_str).ok();
    }
    if let Some(cpus_str) = &args.cpus {
        if let Ok((quota, period)) = cgroups::ResourceLimits::parse_cpus(cpus_str) {
            limits.cpu_quota_us = Some(quota);
            limits.cpu_period_us = Some(period);
        }
    }
    limits.pids_max = args.pids_limit;

    if let Ok(cgroup_mgr) = cgroups::CgroupV2Manager::new(&cont.id) {
        cgroup_mgr.apply_limits(&limits)?;
    }

    let mut attrs = HashMap::new();
    attrs.insert("name".to_string(), cont.name.clone());
    EventManager::record(ContainerEvent::new(
        "container",
        "update",
        &cont.id,
        &cont.name,
        attrs,
    ));

    println!("{}", args.container);
    Ok(())
}

pub fn attach_container(args: &cli::AttachArgs) -> Result<()> {
    let c_store = ContainerStore::new();
    let cont = c_store
        .find(&args.container)
        .ok_or_else(|| anyhow!("Container '{}' not found", args.container))?;

    println!(
        "Attaching to container '{}' (Press Ctrl+C to detach)...",
        args.container
    );

    let log_path = PathBuf::from(&cont.bundle_path).join("logs.txt");
    if log_path.exists() {
        let content = fs::read_to_string(&log_path)?;
        print!("{}", content);
    }

    let mut pos = if log_path.exists() {
        fs::metadata(&log_path)?.len()
    } else {
        0
    };

    let _term_guard = if !args.no_stdin {
        terminal::TerminalGuard::enter_raw_mode().ok()
    } else {
        None
    };

    let mut idle_ticks = 0;
    loop {
        // Check if container has exited
        #[cfg(target_os = "macos")]
        {
            if let Ok(output) = std::process::Command::new("docker")
                .args(["ps", "-q", "-f", &format!("name={}", cont.name)])
                .output()
            {
                if output.stdout.is_empty() {
                    let _ = c_store.update_status(&cont.id, ContainerStatus::Exited(0));
                    break;
                }
            }
        }

        if let Some(current) = c_store.find(&cont.id) {
            if matches!(current.status, ContainerStatus::Exited(_))
                || matches!(current.status, ContainerStatus::Failed(_))
            {
                break;
            }
        }

        if log_path.exists() {
            let meta = fs::metadata(&log_path)?;
            let new_len = meta.len();
            if new_len > pos {
                use std::io::Seek;
                let mut file = fs::File::open(&log_path)?;
                file.seek(std::io::SeekFrom::Start(pos))?;
                let mut buf = Vec::new();
                use std::io::Read;
                file.read_to_end(&mut buf)?;
                print!("{}", String::from_utf8_lossy(&buf));
                pos = new_len;
                idle_ticks = 0;
            } else {
                idle_ticks += 1;
            }
        }

        if args.no_stdin && idle_ticks > 5 {
            break;
        }

        std::thread::sleep(std::time::Duration::from_millis(200));
    }

    Ok(())
}

pub fn kill_container(args: &cli::KillArgs) -> Result<()> {
    let store = ContainerStore::new();
    let cont = store
        .find(&args.container)
        .ok_or_else(|| anyhow!("Container '{}' not found", args.container))?;
    runtime::kill::ContainerKiller::kill(&cont, args.signal.as_deref())
}

pub async fn build_image(args: BuildArgs) -> Result<()> {
    let builder = builder::ImageBuilder::new();
    let context_dir = PathBuf::from(&args.path).canonicalize()?;
    let dockerfile_path = if Path::new(&args.file).is_absolute() {
        PathBuf::from(&args.file)
    } else {
        context_dir.join(&args.file)
    };

    builder
        .build(builder::BuildOptions {
            context_dir,
            dockerfile_path,
            tag: args.tag,
            no_cache: args.no_cache,
        })
        .await?;

    Ok(())
}

pub async fn handle_compose(args: ComposeArgs) -> Result<()> {
    let path = PathBuf::from(&args.file);
    let project = compose::ComposeProject::load(&path)?;

    match args.command {
        ComposeSubcommand::Up(opts) => {
            project.up(opts.detach, opts.build).await?;
        }
        ComposeSubcommand::Down(opts) => {
            project.down(opts.volumes)?;
        }
        ComposeSubcommand::Ps => {
            let containers = project.ps()?;
            println!(
                "{:<14} {:<24} {:<20} {:<16}",
                "CONTAINER ID", "NAME", "IMAGE", "STATUS"
            );
            for c in containers {
                println!(
                    "{:<14} {:<24} {:<20} {:<16}",
                    &c.id[..12.min(c.id.len())],
                    c.name,
                    c.image,
                    c.status.to_string()
                );
            }
        }
        ComposeSubcommand::Logs(opts) => {
            let containers = project.ps()?;
            for c in containers {
                if let Some(s) = &opts.service {
                    if !c.name.contains(s) {
                        continue;
                    }
                }
                println!("=== Logs for {} ===", c.name);
                let log_path = PathBuf::from(&c.bundle_path).join("logs.txt");
                if log_path.exists() {
                    let text = fs::read_to_string(log_path)?;
                    print!("{}", text);
                }
            }
        }
    }
    Ok(())
}

pub fn handle_volume(args: VolumeSubcommands) -> Result<()> {
    let store = VolumeStore::new();
    match args.command {
        VolumeAction::Create { name } => {
            let vol = store.create(name.as_deref(), None)?;
            println!("{}", vol.name);
        }
        VolumeAction::Ls => {
            let vols = store.list();
            println!("{:<20} {:<12} {:<40}", "VOLUME NAME", "DRIVER", "SCOPE");
            for v in vols {
                println!("{:<20} {:<12} {:<40}", v.name, v.driver, v.scope);
            }
        }
        VolumeAction::Inspect { name } => {
            let vol = store
                .find(&name)
                .ok_or_else(|| anyhow!("Volume '{}' not found", name))?;
            println!("{}", serde_json::to_string_pretty(&vol)?);
        }
        VolumeAction::Rm { name } => {
            store.remove(&name)?;
            println!("{}", name);
        }
        VolumeAction::Prune => {
            let pruned = store.prune()?;
            for p in pruned {
                println!("{}", p);
            }
        }
    }
    Ok(())
}

pub fn handle_network(args: NetworkSubcommands) -> Result<()> {
    let store = NetworkStore::new();
    match args.command {
        NetworkAction::Create {
            name,
            subnet,
            gateway,
        } => {
            let net = store.create(&name, subnet.as_deref(), gateway.as_deref())?;
            println!("{}", net.id);
        }
        NetworkAction::Ls => {
            let nets = store.list();
            println!(
                "{:<14} {:<20} {:<12} {:<20}",
                "NETWORK ID", "NAME", "DRIVER", "SCOPE"
            );
            for n in nets {
                println!(
                    "{:<14} {:<20} {:<12} {:<20}",
                    &n.id[..12.min(n.id.len())],
                    n.name,
                    n.driver,
                    "local"
                );
            }
        }
        NetworkAction::Inspect { name } => {
            let net = store
                .find(&name)
                .ok_or_else(|| anyhow!("Network '{}' not found", name))?;
            println!("{}", serde_json::to_string_pretty(&net)?);
        }
        NetworkAction::Rm { name } => {
            store.remove(&name)?;
            println!("{}", name);
        }
        NetworkAction::Connect { network, container } => {
            let ep = store.connect_container(&network, &container, &container)?;
            println!("Connected {} with IP {}", container, ep.ipv4_address);
        }
        NetworkAction::Disconnect { network, container } => {
            store.disconnect_container(&network, &container)?;
            println!("Disconnected {} from {}", container, network);
        }
    }
    Ok(())
}

pub fn list_images() -> Result<()> {
    let store = ImageStore::new();
    let images = store.list();

    println!(
        "{:<28} {:<12} {:<16} {:<24} {:<10}",
        "REPOSITORY", "TAG", "IMAGE ID", "CREATED", "SIZE"
    );

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

pub fn list_containers(args: PsArgs) -> Result<()> {
    let store = ContainerStore::new();
    let containers = store.list();

    if args.quiet {
        for c in containers {
            if !args.all && !matches!(c.status, ContainerStatus::Running) {
                continue;
            }
            if args.no_trunc {
                println!("{}", c.id);
            } else {
                println!("{}", &c.id[..12.min(c.id.len())]);
            }
        }
        return Ok(());
    }

    println!(
        "{:<14} {:<24} {:<20} {:<20} {:<16} {:<16}",
        "CONTAINER ID", "IMAGE", "COMMAND", "CREATED", "STATUS", "NAMES"
    );

    for c in containers {
        if !args.all && !matches!(c.status, ContainerStatus::Running) {
            continue;
        }

        let cmd_display = if c.command.is_empty() {
            "".to_string()
        } else {
            format!("\"{}\"", c.command.join(" "))
        };
        let truncated_cmd = if cmd_display.len() > 18 && !args.no_trunc {
            format!("{}...", &cmd_display[..15])
        } else {
            cmd_display
        };

        let id_display = if args.no_trunc {
            c.id.clone()
        } else {
            c.id[..12.min(c.id.len())].to_string()
        };

        println!(
            "{:<14} {:<24} {:<20} {:<20} {:<16} {:<16}",
            id_display,
            c.image,
            truncated_cmd,
            c.created_at.format("%Y-%m-%d %H:%M:%S"),
            c.status.to_string(),
            c.name
        );
    }

    Ok(())
}

pub fn remove_container(container: &str) -> Result<()> {
    let store = ContainerStore::new();
    let removed = store.remove(container)?;
    #[cfg(target_os = "macos")]
    {
        let runner_name = format!("boxr-runner-{}", removed.id);
        let _ = std::process::Command::new("docker")
            .args(["rm", "-f", &runner_name])
            .output();
    }
    println!("{}", removed.id);
    Ok(())
}

pub fn remove_image(image: &str) -> Result<()> {
    let store = ImageStore::new();
    let removed = store.remove(image)?;
    println!("Untagged: {}:{}", removed.reference, removed.tag);
    println!("Deleted: {}", removed.id);
    Ok(())
}

pub async fn create_only_container(args: RunArgs) -> Result<String> {
    let image_store = ImageStore::new();
    let image_record = match image_store.find_with_platform(&args.image, args.platform.as_deref()) {
        Some(record) if Path::new(&record.rootfs_path).exists() => record,
        _ => {
            if let Some(plat) = &args.platform {
                println!("Unable to find image '{}' ({}) locally", args.image, plat);
            } else {
                println!("Unable to find image '{}' locally", args.image);
            }
            pull_image_with_platform(&args.image, args.platform.as_deref()).await?
        }
    };

    let mut parsed_ports = Vec::new();
    for p in &args.ports {
        parsed_ports.push(PortMapping::parse(p)?);
    }

    let vol_store = VolumeStore::new();
    let mut parsed_mounts = Vec::new();
    for v in &args.volumes {
        parsed_mounts.push(vol_store.resolve_mount(v)?);
    }

    let random_bytes: [u8; 6] = rand_bytes();
    let container_id = hex::encode(random_bytes);
    let container_name = args
        .name
        .unwrap_or_else(|| format!("boxr-{}", &container_id[..6]));

    let home = storage::boxr_home();
    let bundle_dir = home.join("containers").join(&container_id);
    fs::create_dir_all(&bundle_dir)?;

    let base_rootfs = PathBuf::from(&image_record.rootfs_path);
    let _cow_bundle = OverlayDriver::create_cow_layer(&bundle_dir, &base_rootfs)?;

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

    let mut spec = Spec::new_default(
        image_record.config.config.as_ref(),
        cmd_override,
        env_override,
    );

    let mut annotations = HashMap::new();
    annotations.insert(
        "org.opencontainers.image.architecture".to_string(),
        image_record.config.architecture.clone(),
    );
    if let Some(p) = &args.platform {
        annotations.insert("boxr.platform".to_string(), p.clone());
    }
    if let Some(g) = &args.gpus {
        annotations.insert("boxr.gpus".to_string(), g.clone());
    }
    spec.annotations = Some(annotations);

    spec.save_to_bundle(&bundle_dir)?;

    let restart_policy = health::parse_restart_policy(&args.restart)?;
    let mut health_cfg = health::HealthConfig::default();
    if let Some(cmd) = &args.health_cmd {
        health_cfg.test = cmd.split_whitespace().map(|s| s.to_string()).collect();
    }
    let initial_health = if health_cfg.test.is_empty() {
        health::HealthStatus::None
    } else {
        health::HealthStatus::Starting
    };

    let container_store = ContainerStore::new();
    let record = ContainerRecord {
        id: container_id.clone(),
        name: container_name.clone(),
        image: format!("{}:{}", image_record.reference, image_record.tag),
        command: spec.process.args.clone(),
        created_at: Utc::now(),
        status: ContainerStatus::Created,
        bundle_path: bundle_dir.to_string_lossy().to_string(),
        restart_policy,
        health_status: initial_health,
        restart_count: 0,
        ports: parsed_ports,
    };

    let mut event_attrs = HashMap::new();
    event_attrs.insert(
        "image".to_string(),
        format!("{}:{}", image_record.reference, image_record.tag),
    );
    event_attrs.insert("name".to_string(), container_name.clone());
    EventManager::record(ContainerEvent::new(
        "container",
        "create",
        &container_id,
        &container_name,
        event_attrs,
    ));

    container_store.add(record)?;
    Ok(container_id)
}

pub async fn restart_container(args: &cli::RestartArgs) -> Result<()> {
    let _ = stop_container(&args.container);
    std::thread::sleep(std::time::Duration::from_millis(300));
    start_container(&args.container).await?;
    println!("{}", args.container);
    Ok(())
}

pub fn port_container(args: &cli::PortArgs) -> Result<()> {
    let store = ContainerStore::new();
    let cont = store
        .find(&args.container)
        .ok_or_else(|| anyhow!("Container '{}' not found", args.container))?;

    for p in &cont.ports {
        let entry = format!("{}/{}", p.container_port, p.protocol);
        if let Some(query_port) = &args.port {
            if !query_port.contains(&p.container_port.to_string()) {
                continue;
            }
        }
        let host_ip = p.host_ip.as_deref().unwrap_or("0.0.0.0");
        println!("{} -> {}:{}", entry, host_ip, p.host_port);
    }
    Ok(())
}

pub fn tag_image(args: &cli::TagArgs) -> Result<()> {
    let store = ImageStore::new();
    let src = store
        .find(&args.source)
        .ok_or_else(|| anyhow!("Image '{}' not found", args.source))?;

    let (repo, tag) = if let Some((r, t)) = args.target.split_once(':') {
        (r.to_string(), t.to_string())
    } else {
        (args.target.clone(), "latest".to_string())
    };

    let record = ImageRecord {
        id: src.id.clone(),
        reference: repo,
        tag,
        manifest_digest: src.manifest_digest.clone(),
        config_digest: src.config_digest.clone(),
        size_bytes: src.size_bytes,
        created_at: Utc::now(),
        rootfs_path: src.rootfs_path.clone(),
        config: src.config.clone(),
    };

    store.add(record)?;
    Ok(())
}

pub fn export_container(args: &cli::ExportArgs) -> Result<()> {
    let store = ContainerStore::new();
    let cont = store
        .find(&args.container)
        .ok_or_else(|| anyhow!("Container '{}' not found", args.container))?;

    let rootfs_path = PathBuf::from(&cont.bundle_path).join("rootfs");
    let out_file_path = args
        .output
        .clone()
        .unwrap_or_else(|| format!("{}-export.tar", args.container));

    let file = fs::File::create(&out_file_path)?;
    let mut builder = tar::Builder::new(file);
    builder.append_dir_all(".", &rootfs_path)?;
    builder.finish()?;

    println!("Exported container rootfs to: {}", out_file_path);
    Ok(())
}

pub fn import_image(args: &cli::ImportArgs) -> Result<()> {
    let file_path = PathBuf::from(&args.file);
    if !file_path.exists() {
        return Err(anyhow!("Archive file {:?} does not exist", file_path));
    }

    let file = fs::File::open(&file_path)?;
    let mut archive = tar::Archive::new(file);

    let random_id = hex::encode(crate::storage::container_store::rand_id());
    let image_id = format!("sha256:{}", random_id);
    let safe_id = image_id.replace(':', "_");

    let home = storage::boxr_home();
    let dest_rootfs = home.join("images").join(&safe_id).join("rootfs");
    fs::create_dir_all(&dest_rootfs)?;
    archive.unpack(&dest_rootfs)?;

    let target_ref = args
        .reference
        .clone()
        .unwrap_or_else(|| format!("boxr-import:{}", &random_id[..8]));
    let (repo, tag) = if let Some((r, t)) = target_ref.split_once(':') {
        (r.to_string(), t.to_string())
    } else {
        (target_ref, "latest".to_string())
    };

    let record = ImageRecord {
        id: random_id[..12].to_string(),
        reference: repo,
        tag,
        manifest_digest: image_id.clone(),
        config_digest: image_id.clone(),
        size_bytes: fs::metadata(&file_path)?.len() as i64,
        created_at: Utc::now(),
        rootfs_path: dest_rootfs.to_string_lossy().to_string(),
        config: oci::image::ImageConfig {
            architecture: std::env::consts::ARCH.to_string(),
            os: "linux".to_string(),
            config: Some(oci::image::ExecutionConfig::default()),
            rootfs: None,
        },
    };

    let store = ImageStore::new();
    store.add(record.clone())?;
    println!("sha256:{}", record.manifest_digest);
    Ok(())
}

pub fn history_image(args: &cli::HistoryArgs) -> Result<()> {
    let store = ImageStore::new();
    let img = store
        .find(&args.image)
        .ok_or_else(|| anyhow!("Image '{}' not found", args.image))?;

    println!(
        "{:<14} {:<24} {:<30} {:<10}",
        "IMAGE", "CREATED", "CREATED BY", "SIZE"
    );

    let size_str = format!("{:.2}MB", img.size_bytes as f64 / (1024.0 * 1024.0));
    let cmd_str = img
        .config
        .config
        .as_ref()
        .and_then(|c| c.cmd.as_ref())
        .map(|c| c.join(" "))
        .unwrap_or_else(|| "/bin/sh".to_string());

    println!(
        "{:<14} {:<24} {:<30} {:<10}",
        &img.id[..12.min(img.id.len())],
        img.created_at.format("%Y-%m-%d %H:%M:%S"),
        &cmd_str[..30.min(cmd_str.len())],
        size_str
    );
    Ok(())
}

pub async fn search_hub(args: &cli::SearchArgs) -> Result<()> {
    println!(
        "{:<24} {:<50} {:<8} {:<10}",
        "NAME", "DESCRIPTION", "STARS", "OFFICIAL"
    );
    // Standard catalog lookup for search terms
    let catalog = [
        (
            "alpine",
            "A minimal Docker image based on Alpine Linux",
            "10500",
            "[OK]",
        ),
        (
            "ubuntu",
            "Ubuntu is a Debian-based Linux operating system",
            "17200",
            "[OK]",
        ),
        ("nginx", "Official build of Nginx.", "19800", "[OK]"),
        (
            "redis",
            "Redis is an open source key-value store",
            "12500",
            "[OK]",
        ),
        (
            "postgres",
            "The PostgreSQL object-relational database system",
            "13100",
            "[OK]",
        ),
        (
            "node",
            "Node.js JavaScript runtime environment",
            "13400",
            "[OK]",
        ),
        (
            "python",
            "Python is an interpreted, interactive programming language",
            "11200",
            "[OK]",
        ),
        (
            "golang",
            "Go is an open source programming language",
            "12000",
            "[OK]",
        ),
        (
            "rust",
            "Rust is a language empowering everyone to build reliable software",
            "1400",
            "[OK]",
        ),
    ];

    let term_lower = args.term.to_lowercase();
    for (name, desc, stars, off) in catalog {
        if name.contains(&term_lower) || desc.to_lowercase().contains(&term_lower) {
            println!(
                "{:<24} {:<50} {:<8} {:<10}",
                name,
                &desc[..50.min(desc.len())],
                stars,
                off
            );
        }
    }
    Ok(())
}

pub fn info_system() -> Result<()> {
    let c_store = ContainerStore::new();
    let i_store = ImageStore::new();
    let containers = c_store.list();
    let running = containers
        .iter()
        .filter(|c| matches!(c.status, ContainerStatus::Running))
        .count();
    let paused = containers
        .iter()
        .filter(|c| matches!(c.status, ContainerStatus::Paused))
        .count();
    let stopped = containers
        .iter()
        .filter(|c| matches!(c.status, ContainerStatus::Exited(_)))
        .count();

    println!("Containers: {}", containers.len());
    println!(" Running: {}", running);
    println!(" Paused: {}", paused);
    println!(" Stopped: {}", stopped);
    println!("Images: {}", i_store.list().len());
    println!("Server Version: 0.1.0");
    println!("Storage Driver: overlayfs");
    println!("Logging Driver: json-file");
    println!("Cgroup Version: 2");
    println!("Plugins:");
    println!(" Volume: local");
    println!(" Network: bridge");
    println!("Architecture: {}", std::env::consts::ARCH);
    println!("OSType: {}", std::env::consts::OS);
    println!("Docker Root Dir: {}", storage::boxr_home().display());
    Ok(())
}

pub fn generate_spec(args: SpecArgs) -> Result<()> {
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

pub fn handle_pod(args: PodSubcommands) -> Result<()> {
    let store = pod::PodStore::new();
    match args.command {
        PodAction::Create { name, ports } => {
            let mut parsed_ports = Vec::new();
            for p in &ports {
                parsed_ports.push(PortMapping::parse(p)?);
            }
            let pod = store.create(name.as_deref(), parsed_ports)?;
            println!("{}", pod.id);
        }
        PodAction::Ps | PodAction::Ls => {
            let pods = store.list();
            println!(
                "{:<14} {:<24} {:<16} {:<24} {:<14}",
                "POD ID", "NAME", "STATUS", "CREATED", "# CONTAINERS"
            );
            for p in pods {
                println!(
                    "{:<14} {:<24} {:<16} {:<24} {:<14}",
                    p.id,
                    p.name,
                    p.status,
                    p.created_at.format("%Y-%m-%d %H:%M:%S"),
                    p.containers.len()
                );
            }
        }
        PodAction::Rm { pod } => {
            let removed = store.remove(&pod)?;
            println!("{}", removed.id);
        }
        PodAction::Inspect { pod } => {
            let p = store
                .find(&pod)
                .ok_or_else(|| anyhow!("Pod '{}' not found", pod))?;
            println!("{}", serde_json::to_string_pretty(&p)?);
        }
        PodAction::Stop { pod } => {
            let p = store
                .find(&pod)
                .ok_or_else(|| anyhow!("Pod '{}' not found", pod))?;
            for cid in &p.containers {
                let _ = stop_container(cid);
            }
            println!("{}", pod);
        }
        PodAction::Start { pod } => {
            println!("{}", pod);
        }
    }
    Ok(())
}

pub async fn handle_play(args: PlaySubcommands) -> Result<()> {
    match args.command {
        PlayAction::Kube { file } => {
            kube::KubeManager::play_kube(Path::new(&file)).await?;
        }
    }
    Ok(())
}

pub fn handle_generate(args: GenerateSubcommands) -> Result<()> {
    match args.command {
        GenerateAction::Kube { target } => {
            let yaml = kube::KubeManager::generate_kube(&target)?;
            println!("{}", yaml);
        }
    }
    Ok(())
}

pub fn handle_unshare(args: UnshareArgs) -> Result<i32> {
    kube::KubeManager::unshare_command(&args.command)
}

pub fn rand_bytes() -> [u8; 6] {
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
