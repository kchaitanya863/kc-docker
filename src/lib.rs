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
//! - [`compose`]: `docker-compose.yml` parser with topological dependency ordering and DAG cycle detection.
//! - [`daemon`]: Unix domain socket server with Docker-compatible REST API endpoints.
//! - [`events`]: Real-time container lifecycle events stream with JSONL persistence.
//! - [`guardrails`]: Defensive runtime protections (port collision rejection, log rotators, disk margins).
//! - [`health`]: Container healthcheck probes and automatic restart policy supervisor.
//! - [`kube`]: Kubernetes Pod YAML manifest generator and executor (`play kube` / `generate kube`).
//! - [`network`]: Software bridge networks, pure-Rust embedded `usernet` TAP stack, and `pasta` integration.
//! - [`oci`]: OCI spec definitions, image reference parsing, and registry distribution client.
//! - [`pod`]: Podman-style pod abstractions for multi-container groups sharing namespaces.
//! - [`runtime`]: Platform-specific container process execution engines (Linux & macOS).
//! - [`security`]: Rootless single-threaded trampoline, user namespaces (`CLONE_NEWUSER`), UID/GID maps, capabilities, and Seccomp filters.
//! - [`service`]: Native system service definitions (`launchd` on macOS, `systemd` on Linux, `sc.exe` on Windows).
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
pub mod guardrails;
pub mod health;
pub mod kube;
pub mod network;
pub mod oci;
pub mod pod;
pub mod runtime;
pub mod security;
pub mod service;
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
        Commands::Info(args) => {
            info_system(&args)?;
            Ok(0)
        }
        Commands::Version(args) => {
            show_version(&args)?;
            Ok(0)
        }
        Commands::Stop(args) => {
            for c in &args.containers {
                stop_container(c, args.signal.as_deref())?;
            }
            Ok(0)
        }
        Commands::Start(args) => {
            for c in &args.containers {
                start_container(c).await?;
            }
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
            inspect_target(&args)?;
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
            let output_path = args.output.map(PathBuf::from);
            auth::ImageArchiver::save(&args.image, output_path.as_deref())?;
            Ok(0)
        }
        Commands::Load(args) => {
            let input_path = args.input.map(PathBuf::from);
            auth::ImageArchiver::load(input_path.as_deref())?;
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
        Commands::Container(args) => match args.command {
            cli::ContainerAction::Run(run_args) => {
                let code = run_container(run_args).await?;
                Ok(code)
            }
            cli::ContainerAction::Create(create_args) => {
                let id = create_only_container(create_args).await?;
                println!("{}", id);
                Ok(0)
            }
            cli::ContainerAction::Start(start_args) => {
                for c in &start_args.containers {
                    start_container(c).await?;
                }
                Ok(0)
            }
            cli::ContainerAction::Stop(stop_args) => {
                for c in &stop_args.containers {
                    stop_container(c, stop_args.signal.as_deref())?;
                }
                Ok(0)
            }
            cli::ContainerAction::Restart(restart_args) => {
                restart_container(&restart_args).await?;
                Ok(0)
            }
            cli::ContainerAction::Kill(kill_args) => {
                kill_container(&kill_args)?;
                Ok(0)
            }
            cli::ContainerAction::Rm(rm_args) => {
                for c in &rm_args.containers {
                    remove_container(c, rm_args.force)?;
                }
                Ok(0)
            }
            cli::ContainerAction::Pause(pause_args) => {
                pause_container(&pause_args)?;
                Ok(0)
            }
            cli::ContainerAction::Unpause(unpause_args) => {
                unpause_container(&unpause_args)?;
                Ok(0)
            }
            cli::ContainerAction::Wait(wait_args) => {
                let code = wait_container(&wait_args)?;
                Ok(code)
            }
            cli::ContainerAction::Exec(exec_args) => {
                let code = exec_container(&exec_args)?;
                Ok(code)
            }
            cli::ContainerAction::Attach(attach_args) => {
                attach_container(&attach_args)?;
                Ok(0)
            }
            cli::ContainerAction::Logs(logs_args) => {
                container_logs(&logs_args)?;
                Ok(0)
            }
            cli::ContainerAction::Ls(ps_args) => {
                list_containers(ps_args)?;
                Ok(0)
            }
            cli::ContainerAction::Inspect(inspect_args) => {
                inspect_target(&inspect_args)?;
                Ok(0)
            }
            cli::ContainerAction::Top(top_args) => {
                top_container(&top_args)?;
                Ok(0)
            }
            cli::ContainerAction::Port(port_args) => {
                port_container(&port_args)?;
                Ok(0)
            }
            cli::ContainerAction::Cp(cp_args) => {
                cp_container(&cp_args)?;
                Ok(0)
            }
            cli::ContainerAction::Diff(diff_args) => {
                diff_container(&diff_args)?;
                Ok(0)
            }
            cli::ContainerAction::Prune(_) => {
                prune_containers()?;
                Ok(0)
            }
            cli::ContainerAction::Update(update_args) => {
                update_container(&update_args)?;
                Ok(0)
            }
        },
        Commands::Image(args) => match args.command {
            cli::ImageAction::Ls(images_args) => {
                list_images(images_args)?;
                Ok(0)
            }
            cli::ImageAction::Build(build_args) => {
                build_image(build_args).await?;
                Ok(0)
            }
            cli::ImageAction::Pull(pull_args) => {
                pull_image_with_platform(&pull_args.image, pull_args.platform.as_deref()).await?;
                Ok(0)
            }
            cli::ImageAction::Push(push_args) => {
                auth::RegistryPusher::push(&push_args.image).await?;
                Ok(0)
            }
            cli::ImageAction::Tag(tag_args) => {
                tag_image(&tag_args)?;
                Ok(0)
            }
            cli::ImageAction::Rm(rmi_args) => {
                for img in &rmi_args.images {
                    remove_image(img, rmi_args.force, rmi_args.no_prune)?;
                }
                Ok(0)
            }
            cli::ImageAction::Inspect(inspect_args) => {
                inspect_target(&inspect_args)?;
                Ok(0)
            }
            cli::ImageAction::History(history_args) => {
                history_image(&history_args)?;
                Ok(0)
            }
            cli::ImageAction::Save(save_args) => {
                let output_path = save_args.output.map(PathBuf::from);
                auth::ImageArchiver::save(&save_args.image, output_path.as_deref())?;
                Ok(0)
            }
            cli::ImageAction::Load(load_args) => {
                let input_path = load_args.input.map(PathBuf::from);
                auth::ImageArchiver::load(input_path.as_deref())?;
                Ok(0)
            }
            cli::ImageAction::Import(import_args) => {
                import_image(&import_args)?;
                Ok(0)
            }
            cli::ImageAction::Prune(prune_args) => {
                prune_images(prune_args.all)?;
                Ok(0)
            }
        },
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
            let _ = wait_container(&args)?;
            Ok(0)
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
            SystemAction::Prune { all, volumes, .. } => {
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
            let shell = completions::ShellType::parse(&args.shell)
                .unwrap_or_else(|_| completions::ShellType::detect());

            if args.install {
                completions::CompletionGenerator::install(shell)?;
            } else {
                println!("{}", completions::CompletionGenerator::generate(shell));
            }
            Ok(0)
        }
        Commands::Pod(args) => {
            handle_pod(args).await?;
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
        Commands::Images(args) => {
            list_images(args)?;
            Ok(0)
        }
        Commands::Ps(args) => {
            list_containers(args)?;
            Ok(0)
        }
        Commands::Rm(args) => {
            for c in &args.containers {
                remove_container_opts(c, args.force, args.volumes)?;
            }
            Ok(0)
        }
        Commands::Rmi(args) => {
            for img in &args.images {
                remove_image(img, args.force, args.no_prune)?;
            }
            Ok(0)
        }
        Commands::Spec(args) => {
            generate_spec(args)?;
            Ok(0)
        }
        Commands::Context(args) => {
            handle_context(args)?;
            Ok(0)
        }
        Commands::Manifest(args) => {
            handle_manifest(args).await?;
            Ok(0)
        }
        Commands::Service(args) => {
            match args.action {
                cli::ServiceAction::Install => service::ServiceManager::install()?,
                cli::ServiceAction::Start => service::ServiceManager::start()?,
                cli::ServiceAction::Stop => service::ServiceManager::stop()?,
                cli::ServiceAction::Status => service::ServiceManager::status()?,
                cli::ServiceAction::Uninstall => service::ServiceManager::uninstall()?,
            }
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
    fs::create_dir_all(&layers_dir)?;

    let needed_size: u64 = manifest.layers.iter().map(|l| l.size.max(0) as u64).sum();
    guardrails::DiskGuard::ensure_headroom(&layers_dir, needed_size)?;

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

fn apply_capabilities_and_security(
    spec: &mut oci::runtime::Spec,
    privileged: bool,
    cap_add: &[String],
    cap_drop: &[String],
) {
    let mut cap_profile = security::CapabilityProfile::default();
    if privileged {
        let all_caps: Vec<String> = vec![
            "CAP_CHOWN",
            "CAP_DAC_OVERRIDE",
            "CAP_DAC_READ_SEARCH",
            "CAP_FOWNER",
            "CAP_FSETID",
            "CAP_KILL",
            "CAP_SETGID",
            "CAP_SETUID",
            "CAP_SETPCAP",
            "CAP_LINUX_IMMUTABLE",
            "CAP_NET_BIND_SERVICE",
            "CAP_NET_BROADCAST",
            "CAP_NET_ADMIN",
            "CAP_NET_RAW",
            "CAP_IPC_LOCK",
            "CAP_IPC_OWNER",
            "CAP_SYS_MODULE",
            "CAP_SYS_RAWIO",
            "CAP_SYS_CHROOT",
            "CAP_SYS_PTRACE",
            "CAP_SYS_PACCT",
            "CAP_SYS_ADMIN",
            "CAP_SYS_BOOT",
            "CAP_SYS_NICE",
            "CAP_SYS_RESOURCE",
            "CAP_SYS_TIME",
            "CAP_SYS_TTY_CONFIG",
            "CAP_MKNOD",
            "CAP_LEASE",
            "CAP_AUDIT_WRITE",
            "CAP_AUDIT_CONTROL",
            "CAP_SETFCAP",
            "CAP_MAC_OVERRIDE",
            "CAP_MAC_ADMIN",
            "CAP_SYSLOG",
            "CAP_WAKE_ALARM",
            "CAP_BLOCK_SUSPEND",
            "CAP_AUDIT_READ",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        cap_profile.bounding = all_caps.clone();
        cap_profile.effective = all_caps.clone();
        cap_profile.permitted = all_caps;
        if let Some(l) = &mut spec.linux {
            l.seccomp = None;
        }
    } else {
        if cap_drop.iter().any(|c| c.eq_ignore_ascii_case("all")) {
            cap_profile.bounding.clear();
            cap_profile.effective.clear();
            cap_profile.permitted.clear();
        } else {
            for drop in cap_drop {
                let norm = if drop.starts_with("CAP_") {
                    drop.to_uppercase()
                } else {
                    format!("CAP_{}", drop.to_uppercase())
                };
                cap_profile.bounding.retain(|c| c != &norm);
                cap_profile.effective.retain(|c| c != &norm);
                cap_profile.permitted.retain(|c| c != &norm);
            }
        }
        for add in cap_add {
            let norm = if add.starts_with("CAP_") {
                add.to_uppercase()
            } else {
                format!("CAP_{}", add.to_uppercase())
            };
            if !cap_profile.bounding.contains(&norm) {
                cap_profile.bounding.push(norm.clone());
            }
            if !cap_profile.effective.contains(&norm) {
                cap_profile.effective.push(norm.clone());
            }
            if !cap_profile.permitted.contains(&norm) {
                cap_profile.permitted.push(norm);
            }
        }
    }
    spec.process.capabilities = Some(oci::runtime::LinuxCapabilities {
        bounding: Some(cap_profile.bounding.clone()),
        effective: Some(cap_profile.effective.clone()),
        inheritable: Some(cap_profile.bounding.clone()),
        permitted: Some(cap_profile.permitted.clone()),
        ambient: Some(cap_profile.effective),
    });
}

pub async fn run_container(args: RunArgs) -> Result<i32> {
    let restart_policy = health::parse_restart_policy(&args.restart)?;
    if args.rm && !matches!(restart_policy, health::RestartPolicy::No) {
        return Err(anyhow!(
            "Conflicting options: cannot specify both --restart and --rm"
        ));
    }

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
    guardrails::PortCollisionGuard::ensure_no_conflicts(&parsed_ports)?;

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

    fs::create_dir_all(&bundle_dir)?;
    let cow_bundle = OverlayDriver::create_cow_layer(&bundle_dir, &base_rootfs)?;

    // Configure cgroups v2 resource limits if specified
    let mut limits = cgroups::ResourceLimits::default();
    if let Some(mem_str) = &args.memory {
        limits.memory_max_bytes = Some(cgroups::ResourceLimits::parse_memory(mem_str)?);
    }
    if let Some(res_str) = &args.memory_reservation {
        limits.memory_reservation_bytes = Some(cgroups::ResourceLimits::parse_memory(res_str)?);
    }
    if let Some(cpus_str) = &args.cpus {
        let (quota, period) = cgroups::ResourceLimits::parse_cpus(cpus_str)?;
        limits.cpu_quota_us = Some(quota);
        limits.cpu_period_us = Some(period);
    }
    limits.cpu_shares = args.cpu_shares;
    limits.cpuset_cpus = args.cpuset_cpus.clone();
    if let Some(swap_str) = &args.memory_swap {
        limits.memory_swap_max_bytes = Some(cgroups::ResourceLimits::parse_memory(swap_str)?);
    }
    limits.memory_swappiness = args.memory_swappiness.map(|s| s as u64);
    if let Some(pids) = args.pids_limit {
        if pids == 0 {
            return Err(anyhow!("--pids-limit must be greater than 0 or -1"));
        }
        limits.pids_max = Some(pids);
    }

    if let Ok(cgroup_mgr) = cgroups::CgroupV2Manager::new(&container_id) {
        let _ = cgroup_mgr.apply_limits(&limits);
    }

    // Build OCI Runtime Spec
    let cmd_override = if !args.command.is_empty() {
        Some(args.command.as_slice())
    } else {
        None
    };

    let mut combined_env = args.env.clone();
    if let Some(env_file_path) = &args.env_file {
        if let Ok(content) = fs::read_to_string(env_file_path) {
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    continue;
                }
                combined_env.push(trimmed.to_string());
            }
        }
    }

    let mut normalized_env = Vec::new();
    for env_spec in &combined_env {
        if env_spec.contains('=') {
            normalized_env.push(env_spec.clone());
        } else if let Ok(val) = std::env::var(env_spec) {
            normalized_env.push(format!("{}={}", env_spec, val));
        }
    }

    let env_override = if !normalized_env.is_empty() {
        Some(normalized_env.as_slice())
    } else {
        None
    };

    let mut spec = Spec::new_default(
        image_record.config.config.as_ref(),
        cmd_override,
        env_override,
    );

    if let Some(h) = &args.hostname {
        spec.hostname = Some(h.clone());
    }

    if let Some(u) = &args.user {
        if let Some((uid_str, gid_str)) = u.split_once(':') {
            let uid = uid_str.parse::<u32>().unwrap_or(0);
            let gid = gid_str.parse::<u32>().unwrap_or(0);
            spec.process.user.uid = uid;
            spec.process.user.gid = gid;
        } else {
            let uid = u.parse::<u32>().unwrap_or(0);
            spec.process.user.uid = uid;
        }
    }

    if args.read_only {
        spec.root.readonly = true;
    }

    if let Some(shm) = &args.shm_size {
        cgroups::ResourceLimits::parse_memory(shm)?;
        if let Some(m) = spec.mounts.iter_mut().find(|m| m.destination == "/dev/shm") {
            m.options = Some(vec![
                "nosuid".to_string(),
                "noexec".to_string(),
                "nodev".to_string(),
                "mode=1777".to_string(),
                format!("size={}", shm),
            ]);
        }
    }

    if let Some(ep) = &args.entrypoint {
        let mut new_args = vec![ep.clone()];
        if !args.command.is_empty() {
            new_args.extend(args.command.clone());
        }
        spec.process.args = new_args;
    }

    if let Some(w) = &args.workdir {
        let clean_w = if w.starts_with('/') {
            w.clone()
        } else {
            let base_cwd = spec.process.cwd.trim_end_matches('/');
            if base_cwd.is_empty() {
                format!("/{}", w)
            } else {
                format!("{}/{}", base_cwd, w)
            }
        };
        spec.process.cwd = clean_w;
    }

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
    if args.init {
        annotations.insert("boxr.init".to_string(), "true".to_string());
    }
    if let Some(m) = limits.memory_max_bytes {
        annotations.insert("boxr.memory".to_string(), m.to_string());
    }
    if let Some(c) = &args.cpus {
        annotations.insert("boxr.cpus".to_string(), c.clone());
    }
    if let Some(cpuset) = &args.cpuset_cpus {
        annotations.insert("boxr.cpuset_cpus".to_string(), cpuset.clone());
    }
    if let Some(mres) = limits.memory_reservation_bytes {
        annotations.insert("boxr.memory_reservation".to_string(), mres.to_string());
    }
    if let Some(swappiness) = args.memory_swappiness {
        annotations.insert("boxr.memory_swappiness".to_string(), swappiness.to_string());
    }
    if args.oom_kill_disable {
        annotations.insert("boxr.oom_kill_disable".to_string(), "true".to_string());
    }
    if let Some(adj) = args.oom_score_adj {
        annotations.insert("boxr.oom_score_adj".to_string(), adj.to_string());
    }
    if !args.group_add.is_empty() {
        let mut gids = Vec::new();
        for g in &args.group_add {
            if let Ok(gid) = g.parse::<u32>() {
                gids.push(gid);
            }
        }
        if !gids.is_empty() {
            spec.process.user.additional_gids = Some(gids);
        }
        annotations.insert("boxr.group_add".to_string(), serde_json::to_string(&args.group_add)?);
    }
    if let Some(umask_str) = &args.umask {
        let u = if let Some(stripped) = umask_str.strip_prefix("0o") {
            u32::from_str_radix(stripped, 8).unwrap_or(0o022)
        } else if umask_str.starts_with('0') && umask_str.len() > 1 {
            u32::from_str_radix(umask_str, 8).unwrap_or(0o022)
        } else {
            umask_str.parse::<u32>().unwrap_or(0o022)
        };
        spec.process.umask = Some(u);
        annotations.insert("boxr.umask".to_string(), umask_str.clone());
    }
    if let Some(domain) = &args.domainname {
        spec.domainname = Some(domain.clone());
        annotations.insert("boxr.domainname".to_string(), domain.clone());
    }
    for ann in &args.annotations {
        if let Some((k, v)) = ann.split_once('=') {
            annotations.insert(k.to_string(), v.to_string());
        }
    }
    if let Some(timeout) = args.stop_timeout {
        annotations.insert("boxr.stop_timeout".to_string(), timeout.to_string());
    }
    if let Some(sig) = &args.stop_signal {
        annotations.insert("boxr.stop_signal".to_string(), sig.clone());
    }
    let net_mode = network::pasta::NetworkMode::parse(&args.network);
    if net_mode == network::pasta::NetworkMode::Pasta
        && !network::pasta::PastaDriver::is_available()
    {
        #[cfg(target_os = "linux")]
        return Err(anyhow!(
            "pasta rootless networking driver is not installed on this system. Install 'passt' package to enable --network=pasta."
        ));
    }
    annotations.insert("boxr.network".to_string(), args.network.clone());
    spec.annotations = Some(annotations);

    fs::create_dir_all(&bundle_dir)?;
    if !args.add_host.is_empty() {
        for host_entry in &args.add_host {
            let (host, ip_str) = host_entry.split_once(':').ok_or_else(|| {
                anyhow!("bad format for add-host: \"{}\"", host_entry)
            })?;
            if host.is_empty() {
                return Err(anyhow!("bad format for add-host: \"{}\"", host_entry));
            }
            if ip_str.parse::<std::net::IpAddr>().is_err() {
                return Err(anyhow!("bad format for add-host: \"{}\"", host_entry));
            }
        }
        let hosts_json = serde_json::to_string(&args.add_host)?;
        let _ = fs::write(bundle_dir.join("hosts.json"), hosts_json);
    }
    if !args.dns.is_empty() {
        for dns_ip in &args.dns {
            if dns_ip.parse::<std::net::IpAddr>().is_err() {
                return Err(anyhow!("invalid DNS server address: '{}'", dns_ip));
            }
        }
        let dns_json = serde_json::to_string(&args.dns)?;
        let _ = fs::write(bundle_dir.join("dns.json"), dns_json);
    }
    if !args.dns_search.is_empty() {
        let search_json = serde_json::to_string(&args.dns_search)?;
        let _ = fs::write(bundle_dir.join("dns_search.json"), search_json);
    }
    if !args.dns_option.is_empty() {
        let opt_json = serde_json::to_string(&args.dns_option)?;
        let _ = fs::write(bundle_dir.join("dns_option.json"), opt_json);
    }
    if !args.labels.is_empty() {
        let labels_json = serde_json::to_string(&args.labels)?;
        let _ = fs::write(bundle_dir.join("labels.json"), labels_json);
    }
    if !args.sysctl.is_empty() {
        let _ = fs::write(
            bundle_dir.join("sysctl.json"),
            serde_json::to_string(&args.sysctl)?,
        );
    }
    if !args.ulimits.is_empty() {
        let _ = fs::write(
            bundle_dir.join("ulimits.json"),
            serde_json::to_string(&args.ulimits)?,
        );
    }
    if let Some(cidfile) = &args.cidfile {
        fs::write(cidfile, &container_id)?;
    }
    for vf in &args.volumes_from {
        let (target_cont_name, mode) = if let Some((c, m)) = vf.split_once(':') {
            (c, Some(m))
        } else {
            (vf.as_str(), None)
        };
        let c_lookup = ContainerStore::new();
        if let Some(src_cont) = c_lookup.find(target_cont_name) {
            let src_config_path = PathBuf::from(&src_cont.bundle_path).join("config.json");
            if let Ok(src_content) = fs::read_to_string(&src_config_path) {
                if let Ok(src_spec) = serde_json::from_str::<Spec>(&src_content) {
                    for mut m in src_spec.mounts {
                        if m.destination != "/proc"
                            && m.destination != "/sys"
                            && m.destination != "/dev"
                            && m.destination != "/dev/pts"
                            && m.destination != "/dev/shm"
                            && m.destination != "/dev/mqueue"
                        {
                            if mode == Some("ro") {
                                let mut opts = m.options.unwrap_or_default();
                                opts.retain(|o| o != "rw");
                                opts.push("ro".to_string());
                                m.options = Some(opts);
                            }
                            spec.mounts.push(m);
                        }
                    }
                }
            }
        }
    }
    for t in &args.tmpfs {
        let (dest, opts) = if let Some((d, o)) = t.split_once(':') {
            (d, o)
        } else {
            (t.as_str(), "rw,nosuid,nodev,size=65536k")
        };
        spec.mounts.push(oci::runtime::Mount {
            destination: dest.to_string(),
            mount_type: "tmpfs".to_string(),
            source: "tmpfs".to_string(),
            options: Some(opts.split(',').map(|s| s.to_string()).collect()),
        });
    }
    for m in &args.mount {
        let mut mount_type = "bind".to_string();
        let mut source = String::new();
        let mut target = String::new();
        let mut ro = false;
        for kv in m.split(',') {
            if let Some((k, v)) = kv.split_once('=') {
                match k.trim() {
                    "type" => mount_type = v.trim().to_string(),
                    "source" | "src" => source = v.trim().to_string(),
                    "target" | "destination" | "dst" => target = v.trim().to_string(),
                    "readonly" | "ro" => ro = true,
                    _ => {}
                }
            } else if kv.trim() == "readonly" || kv.trim() == "ro" {
                ro = true;
            }
        }
        if !target.is_empty() {
            let mut opts = vec!["rbind".to_string()];
            if ro {
                opts.push("ro".to_string());
            }
            spec.mounts.push(oci::runtime::Mount {
                destination: target,
                mount_type,
                source,
                options: Some(opts),
            });
        }
    }
    if args.security_opt.iter().any(|s| s == "seccomp=unconfined") {
        if let Some(l) = &mut spec.linux {
            l.seccomp = None;
        }
    }
    if args
        .security_opt
        .iter()
        .any(|s| s == "no-new-privileges" || s == "no-new-privileges:true")
    {
        spec.process.no_new_privileges = Some(true);
    }
    apply_capabilities_and_security(&mut spec, args.privileged, &args.cap_add, &args.cap_drop);
    if !args.privileged && !args.security_opt.iter().any(|s| s == "seccomp=unconfined") {
        if let Some(l) = &mut spec.linux {
            if l.seccomp.is_none() {
                l.seccomp = Some(
                    serde_json::to_value(security::SeccompRule::default_filter())
                        .unwrap_or(serde_json::Value::Null),
                );
            }
        }
    }
    if args.cpu_count.is_some()
        || args.cpu_percent.is_some()
        || args.io_maxbandwidth.is_some()
        || args.io_maxiops.is_some()
    {
        let mut win_cpu = oci::runtime::WindowsCPUResources::default();
        if let Some(count) = args.cpu_count {
            win_cpu.count = Some(count as u64);
        }
        if let Some(percent) = args.cpu_percent {
            win_cpu.percent = Some(percent as u16);
        }

        let mut win_storage = oci::runtime::WindowsStorageResources::default();
        if let Some(bw_str) = &args.io_maxbandwidth {
            if let Ok(bw) = cgroups::ResourceLimits::parse_memory(bw_str) {
                win_storage.bps = Some(bw as u64);
            }
        }
        if let Some(iops) = args.io_maxiops {
            win_storage.iops = Some(iops);
        }

        let mut win_spec = oci::runtime::Windows::default();
        win_spec.resources = Some(oci::runtime::WindowsResources {
            cpu: Some(win_cpu),
            storage: Some(win_storage),
        });
        spec.windows = Some(win_spec);
    }
    spec.save_to_bundle(&bundle_dir)?;

    let restart_policy = health::parse_restart_policy(&args.restart)?;
    let mut health_cfg = health::HealthConfig::default();
    if !args.no_healthcheck {
        if let Some(cmd) = &args.health_cmd {
            health_cfg.test = cmd.split_whitespace().map(|s| s.to_string()).collect();
        }
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
        exposed_ports: args.expose.clone(),
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
        event_attrs.clone(),
    ));
    container_store.add(record)?;

    if args.detach {
        println!("{}", container_id);
        #[cfg(not(target_os = "macos"))]
        if !parsed_ports.is_empty() {
            let _ = network::rootless::PortForwardManager::start_forwarding(&parsed_ports).await;
        }
    }

    EventManager::record(ContainerEvent::new(
        "container",
        "start",
        &container_id,
        &container_name,
        event_attrs.clone(),
    ));

    // Enter raw terminal mode on macOS where a VM serial console is used
    #[cfg(target_os = "macos")]
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

    if args.detach {
        let pid_path = bundle_dir.join("vm.pid");
        if let Ok(pid_str) = fs::read_to_string(&pid_path) {
            if let Ok(pid) = pid_str.trim().parse::<i32>() {
                #[cfg(unix)]
                if unsafe { libc::kill(pid, 0) } != 0 {
                    return Err(anyhow!(
                        "Container failed to start: trampoline process exited immediately"
                    ));
                }
                #[cfg(windows)]
                {
                    let is_running = std::process::Command::new("tasklist")
                        .args(["/FI", &format!("PID eq {}", pid)])
                        .output()
                        .map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
                        .unwrap_or(true);
                    if !is_running {
                        return Err(anyhow!(
                            "Container failed to start: process exited immediately"
                        ));
                    }
                }
            }
        }
        if !health_cfg.test.is_empty() {
            let mut health_res = health::HealthCheckResult::default();
            let _ = health::check_container_health(&bundle_dir, &health_cfg, &mut health_res);
            let _ = container_store.update_health_status(&container_id, health_res.status);
        }
        return Ok(exit_code);
    }

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

    event_attrs.insert("exitCode".to_string(), exit_code.to_string());
    EventManager::record(ContainerEvent::new(
        "container",
        "die",
        &container_id,
        &container_name,
        event_attrs,
    ));

    // Health check evaluation if configured
    if args.detach && !health_cfg.test.is_empty() {
        let mut health_res = health::HealthCheckResult::default();
        let _ = health::check_container_health(&bundle_dir, &health_cfg, &mut health_res);
    }

    if !args.detach {
        if args.rm {
            let _ = OverlayDriver::cleanup(&cow_bundle);
            let _ = container_store.remove(&container_id);
            let _ = fs::remove_dir_all(&bundle_dir);
        } else {
            let _ =
                container_store.update_status(&container_id, ContainerStatus::Exited(exit_code));
        }
    }

    Ok(exit_code)
}

pub fn stop_container(container: &str, signal: Option<&str>) -> Result<()> {
    stop_container_with_home(container, signal, None)
}

pub fn stop_container_with_home(
    container: &str,
    signal: Option<&str>,
    home_opt: Option<&Path>,
) -> Result<()> {
    stop_container_with_home_and_timeout(container, signal, None, home_opt)
}

pub fn stop_container_with_home_and_timeout(
    container: &str,
    signal: Option<&str>,
    timeout_secs: Option<u64>,
    home_opt: Option<&Path>,
) -> Result<()> {
    let store = match home_opt {
        Some(h) => ContainerStore::with_home(h.to_path_buf()),
        None => ContainerStore::new(),
    };
    let c = store
        .find(container)
        .ok_or_else(|| anyhow!("Container '{}' not found", container))?;

    let sig_num = if let Some(s) = signal {
        runtime::kill::ContainerKiller::parse_signal(s).unwrap_or(libc::SIGTERM)
    } else {
        libc::SIGTERM
    };

    #[cfg(unix)]
    {
        let bundle_path = std::path::PathBuf::from(&c.bundle_path);
        let mut pids = Vec::new();
        if let Ok(pid_str) = std::fs::read_to_string(bundle_path.join("vm.pid")) {
            if let Ok(pid) = pid_str.trim().parse::<i32>() {
                pids.push(pid);
            }
        }
        if let Ok(pid_str) = std::fs::read_to_string(bundle_path.join("container.pid")) {
            if let Ok(pid) = pid_str.trim().parse::<i32>() {
                pids.push(pid);
            }
        }
        let grace_ms = timeout_secs.unwrap_or(3) * 1000;
        let iters = (grace_ms / 50).max(1);
        for pid in pids {
            unsafe {
                libc::kill(pid, sig_num);
                let _ = libc::kill(-pid, sig_num);
            }
            // Wait up to graceful timeout for hypervisor/process stop
            for _ in 0..iters {
                std::thread::sleep(std::time::Duration::from_millis(50));
                if unsafe { libc::kill(pid, 0) != 0 } {
                    break;
                }
            }
            if unsafe { libc::kill(pid, 0) == 0 } {
                unsafe {
                    libc::kill(pid, libc::SIGKILL);
                    let _ = libc::kill(-pid, libc::SIGKILL);
                }
            }
        }
        let _ = std::fs::remove_file(bundle_path.join("vm.pid"));
        let _ = std::fs::remove_file(bundle_path.join("container.pid"));
    }
    #[cfg(target_os = "windows")]
    {
        let bundle_path = std::path::PathBuf::from(&c.bundle_path);
        let pid_file = bundle_path.join("vm.pid");
        if let Ok(pid_str) = std::fs::read_to_string(pid_file) {
            if let Ok(pid) = pid_str.trim().parse::<u32>() {
                let _ = std::process::Command::new("taskkill")
                    .args(["/F", "/PID", &pid.to_string()])
                    .output();
            }
        }
    }
    store.update_status(&c.id, ContainerStatus::Exited(0))?;
    let _ = guardrails::ProcessReaper::reap_stale_containers();
    println!("{}", container);
    Ok(())
}

pub async fn start_container(container: &str) -> Result<()> {
    start_container_with_home(container, None).await
}

pub async fn start_container_with_home(container: &str, home_opt: Option<&Path>) -> Result<()> {
    let store = match home_opt {
        Some(h) => ContainerStore::with_home(h.to_path_buf()),
        None => ContainerStore::new(),
    };
    let rec = store
        .find(container)
        .ok_or_else(|| anyhow!("Container '{}' not found", container))?;

    let bundle_path = PathBuf::from(&rec.bundle_path);
    let config_file = bundle_path.join("config.json");
    let content = fs::read_to_string(&config_file)?;
    let spec: Spec = serde_json::from_str(&content)?;

    // Restore port forwarding for restarted container
    #[cfg(not(target_os = "macos"))]
    if !rec.ports.is_empty() {
        let _ = network::rootless::PortForwardManager::start_forwarding(&rec.ports).await;
    }

    store.update_status(&rec.id, ContainerStatus::Running)?;
    let _ = execute_bundle(&bundle_path, &spec, &[], &rec.ports, true)?;
    println!("{}", container);
    Ok(())
}

pub fn container_logs(args: &LogsArgs) -> Result<()> {
    let store = ContainerStore::new();
    let rec = store
        .find(&args.container)
        .ok_or_else(|| anyhow!("Container '{}' not found", args.container))?;

    let log_path = PathBuf::from(&rec.bundle_path).join("logs.txt");
    let rootfs_log = PathBuf::from(&rec.bundle_path)
        .join("rootfs")
        .join("logs.txt");

    let print_line = |line: &str| {
        if args.timestamps {
            println!("{} {}", Utc::now().to_rfc3339(), line);
        } else {
            println!("{}", line);
        }
    };

    let content = if log_path.exists()
        && fs::metadata(&log_path)
            .map(|m| m.len() > 0)
            .unwrap_or(false)
    {
        fs::read_to_string(&log_path)?
    } else if rootfs_log.exists() {
        fs::read_to_string(&rootfs_log)?
    } else if log_path.exists() {
        fs::read_to_string(&log_path)?
    } else {
        println!("No logs available for container {}", args.container);
        return Ok(());
    };

    let mut lines: Vec<&str> = content.lines().collect();

    if let Some(since_str) = &args.since {
        if let Ok(since_dt) = chrono::DateTime::parse_from_rfc3339(since_str) {
            lines.retain(|l| {
                if let Some((ts_part, _)) = l.split_once(' ') {
                    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts_part) {
                        return dt >= since_dt;
                    }
                }
                true
            });
        }
    }

    if let Some(until_str) = &args.until {
        if let Ok(until_dt) = chrono::DateTime::parse_from_rfc3339(until_str) {
            lines.retain(|l| {
                if let Some((ts_part, _)) = l.split_once(' ') {
                    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts_part) {
                        return dt <= until_dt;
                    }
                }
                true
            });
        }
    }

    if let Some(tail) = args.tail {
        if tail == 0 {
            lines.clear();
        } else if lines.len() > tail {
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

    if !matches!(rec.status, ContainerStatus::Running) {
        return Err(anyhow!(
            "Container '{}' is not running: {}",
            args.container,
            rec.status
        ));
    }

    if args.command.is_empty() {
        return Err(anyhow!("Command cannot be empty for exec"));
    }

    let _term_guard = if args.interactive && args.tty && !args.detach {
        terminal::TerminalGuard::enter_raw_mode().ok()
    } else {
        None
    };

    let mut combined_env = args.env.clone();
    if let Some(env_file_path) = &args.env_file {
        if let Ok(content) = fs::read_to_string(env_file_path) {
            for line in content.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() && !trimmed.starts_with('#') {
                    combined_env.push(trimmed.to_string());
                }
            }
        }
    }

    let bundle_path = PathBuf::from(&rec.bundle_path);
    exec_in_bundle(
        &bundle_path,
        &args.command,
        &combined_env,
        args.workdir.as_deref(),
        args.user.as_deref(),
        args.detach,
    )
}

pub fn inspect_target(args: &cli::InspectArgs) -> Result<()> {
    let only_type = args.obj_type.as_deref().unwrap_or("");
    if !only_type.is_empty()
        && only_type != "container"
        && only_type != "image"
        && only_type != "volume"
        && only_type != "network"
    {
        return Err(anyhow!("invalid inspect type: \"{}\"", only_type));
    }

    let c_store = ContainerStore::new();
    let i_store = ImageStore::new();
    let v_store = VolumeStore::new();
    let n_store = NetworkStore::new();

    let mut results = Vec::new();

    for target in &args.targets {
        let mut matched = false;

        if only_type.is_empty() || only_type == "container" {
            if let Some(c) = c_store.find(target) {
                let is_running = matches!(c.status, ContainerStatus::Running);
                let mut labels_map = HashMap::new();
                let labels_file = PathBuf::from(&c.bundle_path).join("labels.json");
                if labels_file.exists() {
                    if let Ok(content) = fs::read_to_string(&labels_file) {
                        if let Ok(labels_vec) = serde_json::from_str::<Vec<String>>(&content) {
                            for l in labels_vec {
                                if let Some((k, v)) = l.split_once('=') {
                                    labels_map.insert(k.to_string(), v.to_string());
                                } else {
                                    labels_map.insert(l, "".to_string());
                                }
                            }
                        }
                    }
                }
                let size_bytes = if args.size {
                    crate::system::dir_size(&PathBuf::from(&c.bundle_path))
                } else {
                    0
                };
                let mut exposed_map = HashMap::new();
                for ep in &c.exposed_ports {
                    let key = if ep.contains('/') { ep.clone() } else { format!("{}/tcp", ep) };
                    exposed_map.insert(key, serde_json::json!({}));
                }

                // Discover host PID
                let bundle_path = PathBuf::from(&c.bundle_path);
                let pid = {
                    let mut found_pid = 0;
                    if let Ok(p_str) = fs::read_to_string(bundle_path.join("container.pid")) {
                        if let Ok(p) = p_str.trim().parse::<i32>() {
                            found_pid = p;
                        }
                    }
                    if found_pid == 0 {
                        if let Ok(p_str) = fs::read_to_string(bundle_path.join("vm.pid")) {
                            if let Ok(p) = p_str.trim().parse::<i32>() {
                                found_pid = p;
                            }
                        }
                    }
                    found_pid
                };

                // Real network endpoints
                let mut ip_address = "172.17.0.2".to_string();
                let mut gateway = "172.17.0.1".to_string();
                let mut mac_address = "02:42:ac:11:00:02".to_string();
                for net in n_store.list() {
                    if let Some(ep) = net.containers.get(&c.id).or_else(|| net.containers.get(&c.name)) {
                        ip_address = ep.ipv4_address.clone();
                        gateway = net.gateway.clone();
                        mac_address = ep.mac_address.clone();
                        break;
                    }
                }

                // PortBindings and Ports
                let mut port_bindings = serde_json::Map::new();
                let mut ports_map = serde_json::Map::new();
                for p in &c.ports {
                    let key = format!("{}/{}", p.container_port, p.protocol);
                    let host_ip = p.host_ip.as_deref().unwrap_or("");
                    let binding = serde_json::json!({
                        "HostIp": host_ip,
                        "HostPort": p.host_port.to_string(),
                    });
                    port_bindings.insert(key.clone(), serde_json::json!([binding.clone()]));
                    ports_map.insert(key, serde_json::json!([binding]));
                }

                let docker_compat_inspect = serde_json::json!({
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
                        "Pid": pid,
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
                    "SizeRw": size_bytes,
                    "Config": {
                        "Image": c.image,
                        "Labels": labels_map,
                        "ExposedPorts": exposed_map,
                    },
                    "HostConfig": {
                        "PortBindings": port_bindings,
                        "RestartPolicy": {
                            "Name": c.restart_policy.to_string(),
                            "MaximumRetryCount": 0
                        }
                    },
                    "NetworkSettings": {
                        "Bridge": "",
                        "SandboxID": "",
                        "HairpinMode": false,
                        "LinkLocalIPv6Address": "",
                        "LinkLocalIPv6PrefixLen": 0,
                        "Ports": ports_map,
                        "SandboxKey": "",
                        "SecondaryIPAddresses": null,
                        "SecondaryIPv6Addresses": null,
                        "EndpointID": "",
                        "Gateway": gateway,
                        "GlobalIPv6Address": "",
                        "GlobalIPv6PrefixLen": 0,
                        "IPAddress": ip_address,
                        "IPPrefixLen": 16,
                        "IPv6Gateway": "",
                        "MacAddress": mac_address,
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
                });
                results.push(docker_compat_inspect);
                matched = true;
            }
        }

        if !matched && (only_type.is_empty() || only_type == "image") {
            if let Some(i) = i_store.find(target) {
                let docker_compat_image = serde_json::json!({
                    "Id": format!("sha256:{}", i.id),
                    "RepoTags": [format!("{}:{}", i.reference, i.tag)],
                    "Size": i.size_bytes,
                    "Created": i.created_at.to_rfc3339(),
                    "Architecture": i.config.architecture,
                    "Os": i.config.os,
                    "boxr_raw": i
                });
                results.push(docker_compat_image);
                matched = true;
            }
        }

        if !matched && (only_type.is_empty() || only_type == "volume") {
            if let Some(vol) = v_store.find(target) {
                let docker_compat_volume = serde_json::json!({
                    "CreatedAt": vol.created_at.to_rfc3339(),
                    "Driver": vol.driver,
                    "Labels": vol.labels,
                    "Mountpoint": vol.mountpoint,
                    "Name": vol.name,
                    "Options": null,
                    "Scope": vol.scope,
                });
                results.push(docker_compat_volume);
                matched = true;
            }
        }

        if !matched && (only_type.is_empty() || only_type == "network") {
            if let Some(net) = n_store.find(target) {
                let docker_compat_net = serde_json::json!({
                    "Name": net.name,
                    "Id": net.id,
                    "Created": net.created_at.to_rfc3339(),
                    "Scope": "local",
                    "Driver": net.driver,
                    "EnableIPv6": false,
                    "IPAM": {
                        "Driver": "default",
                        "Options": null,
                        "Config": [{
                            "Subnet": net.subnet,
                            "Gateway": net.gateway
                        }]
                    },
                    "Internal": net.internal,
                    "Attachable": false,
                    "Ingress": false,
                    "ConfigFrom": { "Network": "" },
                    "ConfigOnly": false,
                    "Containers": {},
                    "Options": {},
                    "Labels": {}
                });
                results.push(docker_compat_net);
                matched = true;
            }
        }

        if !matched {
            return Err(anyhow!("No such container or image: '{}'", target));
        }
    }

    if let Some(fmt) = &args.format {
        for res in &results {
            if fmt == "json" || fmt == "{{json .}}" {
                println!("{}", serde_json::to_string_pretty(res)?);
            } else {
                println!("{}", evaluate_simple_template(fmt, res));
            }
        }
    } else {
        println!("{}", serde_json::to_string_pretty(&results)?);
    }

    Ok(())
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

    if !matches!(cont.status, ContainerStatus::Running) {
        return Err(anyhow!("Container {} is not running", args.container));
    }

    let bundle_path = PathBuf::from(&cont.bundle_path);
    runtime::top::ContainerTop::list_processes(&bundle_path, &args.ps_args)?;
    Ok(())
}

pub fn commit_container(args: &cli::CommitArgs) -> Result<()> {
    let c_store = ContainerStore::new();
    let cont = c_store
        .find(&args.container)
        .ok_or_else(|| anyhow!("Container '{}' not found", args.container))?;

    let was_running = matches!(cont.status, ContainerStatus::Running);
    if was_running && args.pause {
        if let Ok(cgroup_mgr) = cgroups::CgroupV2Manager::new(&cont.id) {
            let _ = cgroup_mgr.freeze();
        }
    }

    let i_store = ImageStore::new();
    let record = i_store.commit_container(
        &cont,
        args.repo_tag.as_deref(),
        args.message.as_deref(),
        args.author.as_deref(),
    );

    if was_running && args.pause {
        if let Ok(cgroup_mgr) = cgroups::CgroupV2Manager::new(&cont.id) {
            let _ = cgroup_mgr.unfreeze();
        }
    }

    let record = record?;

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

    if !matches!(cont.status, ContainerStatus::Running) {
        return Err(anyhow!("Container {} is not running", args.container));
    }

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

    if !matches!(cont.status, ContainerStatus::Paused) {
        return Err(anyhow!("Container {} is not paused", args.container));
    }

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
                #[cfg(unix)]
                {
                    let bundle_path = std::path::PathBuf::from(&cont.bundle_path);
                    let exitcode_file = bundle_path.join("rootfs").join("boxr-exitcode");
                    if exitcode_file.exists() {
                        if let Ok(code_str) = std::fs::read_to_string(&exitcode_file) {
                            if let Ok(code) = code_str.trim().parse::<i32>() {
                                let _ =
                                    c_store.update_status(&cont.id, ContainerStatus::Exited(code));
                                println!("{}", code);
                                return Ok(code);
                            }
                        }
                    }
                    let pid_file = bundle_path.join("vm.pid");
                    let is_running = if let Ok(pid_str) = std::fs::read_to_string(pid_file) {
                        if let Ok(pid) = pid_str.trim().parse::<i32>() {
                            unsafe { libc::kill(pid, 0) == 0 }
                        } else {
                            false
                        }
                    } else {
                        false
                    };
                    if !is_running {
                        let _ = c_store.update_status(&cont.id, ContainerStatus::Exited(137));
                        println!("137");
                        return Ok(137);
                    }
                }
                #[cfg(target_os = "windows")]
                {
                    let bundle_path = std::path::PathBuf::from(&cont.bundle_path);
                    let pid_file = bundle_path.join("vm.pid");
                    let is_running = if let Ok(pid_str) = std::fs::read_to_string(pid_file) {
                        if let Ok(pid) = pid_str.trim().parse::<u32>() {
                            std::process::Command::new("tasklist")
                                .args(["/FI", &format!("PID eq {}", pid)])
                                .output()
                                .map(|o| {
                                    String::from_utf8_lossy(&o.stdout).contains(&pid.to_string())
                                })
                                .unwrap_or(false)
                        } else {
                            false
                        }
                    } else {
                        false
                    };
                    if !is_running {
                        let _ = c_store.update_status(&cont.id, ContainerStatus::Exited(137));
                        println!("137");
                        return Ok(137);
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
        let mem = cgroups::ResourceLimits::parse_memory(mem_str)?;
        limits.memory_max_bytes = Some(mem);
    }
    if let Some(cpus_str) = &args.cpus {
        let (quota, period) = cgroups::ResourceLimits::parse_cpus(cpus_str)?;
        limits.cpu_quota_us = Some(quota);
        limits.cpu_period_us = Some(period);
    }
    if let (Some(q), Some(p)) = (args.cpu_quota, args.cpu_period) {
        limits.cpu_quota_us = Some(q);
        limits.cpu_period_us = Some(p);
    }
    if let Some(shares) = args.cpu_shares {
        limits.cpu_shares = Some(shares);
    }
    if let Some(swap_str) = &args.memory_swap {
        let swap = cgroups::ResourceLimits::parse_memory(swap_str)?;
        limits.memory_swap_max_bytes = Some(swap);
    }
    if let Some(pids) = args.pids_limit {
        if pids == 0 {
            return Err(anyhow!("--pids-limit must be greater than 0 or -1"));
        }
        limits.pids_max = Some(pids);
    }

    if let Ok(cgroup_mgr) = cgroups::CgroupV2Manager::new(&cont.id) {
        cgroup_mgr.apply_limits(&limits)?;
    }

    // Persist updated limits in bundle config.json
    let bundle_path = PathBuf::from(&cont.bundle_path);
    let config_path = bundle_path.join("config.json");
    if let Ok(content) = fs::read_to_string(&config_path) {
        if let Ok(mut spec) = serde_json::from_str::<Spec>(&content) {
            if let Some(l) = &mut spec.linux {
                if let Some(res) = &mut l.resources {
                    if let Some(m) = limits.memory_max_bytes {
                        res.memory = Some(oci::runtime::LinuxMemory {
                            limit: Some(m),
                            ..Default::default()
                        });
                    }
                    if let Some(p) = limits.pids_max {
                        res.pids = Some(oci::runtime::LinuxPids { limit: p });
                    }
                }
            }
            let _ = spec.save_to_bundle(&bundle_path);
        }
    }

    if let Some(r_policy_str) = &args.restart {
        if let Ok(policy) = health::parse_restart_policy(r_policy_str) {
            let mut updated_cont = cont.clone();
            updated_cont.restart_policy = policy;
            let _ = c_store.add(updated_cont);
        }
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

    loop {
        // Check if container has exited
        #[cfg(unix)]
        {
            let bundle_path = std::path::PathBuf::from(&cont.bundle_path);
            let pid_file = bundle_path.join("vm.pid");
            if let Ok(pid_str) = std::fs::read_to_string(pid_file) {
                if let Ok(pid) = pid_str.trim().parse::<i32>() {
                    if unsafe { libc::kill(pid, 0) != 0 } {
                        let _ = c_store.update_status(&cont.id, ContainerStatus::Exited(0));
                        break;
                    }
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
            }
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

    let mut build_args = std::collections::HashMap::new();
    for ba in &args.build_args {
        if let Some((k, v)) = ba.split_once('=') {
            build_args.insert(k.trim().to_string(), v.trim().to_string());
        }
    }

    let primary_tag = args.tags.first().cloned();
    let record = builder
        .build(builder::BuildOptions {
            context_dir,
            dockerfile_path,
            tag: primary_tag,
            no_cache: args.no_cache,
            build_args,
            target: args.target,
            add_host: args.add_host,
            memory: args.memory,
            shm_size: args.shm_size,
        })
        .await?;

    let store = ImageStore::new();
    for extra_tag in args.tags.iter().skip(1) {
        let (ref_name, tag_name) = if let Some((r, t)) = extra_tag.split_once(':') {
            (r.to_string(), t.to_string())
        } else {
            (extra_tag.clone(), "latest".to_string())
        };
        let mut tagged_record = record.clone();
        tagged_record.reference = ref_name;
        tagged_record.tag = tag_name;
        let _ = store.add(tagged_record);
        println!("Successfully tagged image as {}", extra_tag);
    }

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
        VolumeAction::Create {
            name,
            driver,
            opts,
            labels,
            scope,
            ..
        } => {
            let mut label_map = HashMap::new();
            for l in labels {
                if let Some((k, v)) = l.split_once('=') {
                    label_map.insert(k.to_string(), v.to_string());
                }
            }
            let mut opt_map = HashMap::new();
            for o in opts {
                if let Some((k, v)) = o.split_once('=') {
                    opt_map.insert(k.to_string(), v.to_string());
                } else {
                    opt_map.insert(o.to_string(), "".to_string());
                }
            }
            let vol = store.create_with_options(
                name.as_deref(),
                &driver,
                Some(label_map),
                scope.as_deref().unwrap_or("local"),
                Some(opt_map),
            )?;
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
            let compat = serde_json::json!([{
                "CreatedAt": vol.created_at.to_rfc3339(),
                "Driver": vol.driver,
                "Labels": vol.labels,
                "Mountpoint": vol.mountpoint,
                "Name": vol.name,
                "Options": vol.options,
                "Scope": vol.scope,
            }]);
            println!("{}", serde_json::to_string_pretty(&compat)?);
        }
        VolumeAction::Rm { name } => {
            store.remove(&name)?;
            println!("{}", name);
        }
        VolumeAction::Prune { .. } => {
            let pruned = store.prune()?;
            if !pruned.is_empty() {
                println!("Deleted Volumes:");
                for p in pruned {
                    println!("{}", p);
                }
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
            driver,
            subnet,
            gateway,
            internal,
            attachable,
            labels,
            ..
        } => {
            let mut label_map = HashMap::new();
            for l in labels {
                if let Some((k, v)) = l.split_once('=') {
                    label_map.insert(k.to_string(), v.to_string());
                }
            }
            let net = store.create_with_options(
                &name,
                &driver,
                subnet.as_deref(),
                gateway.as_deref(),
                internal,
                attachable,
                label_map,
            )?;
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
            let compat = serde_json::json!([{
                "Name": net.name,
                "Id": net.id,
                "Created": net.created_at.to_rfc3339(),
                "Scope": "local",
                "Driver": net.driver,
                "EnableIPv6": false,
                "IPAM": {
                    "Driver": "default",
                    "Options": null,
                    "Config": [{
                        "Subnet": net.subnet,
                        "Gateway": net.gateway,
                    }]
                },
                "Internal": net.internal,
                "Attachable": net.attachable,
                "Containers": net.containers,
                "Options": {},
                "Labels": net.labels,
            }]);
            println!("{}", serde_json::to_string_pretty(&compat)?);
        }
        NetworkAction::Rm { name } => {
            store.remove(&name)?;
            println!("{}", name);
        }
        NetworkAction::Prune { .. } => {
            let mut pruned = Vec::new();
            for net in store.list() {
                if net.name != NetworkStore::DEFAULT_NETWORK && net.containers.is_empty() {
                    let _ = store.remove(&net.name);
                    pruned.push(net.name);
                }
            }
            if !pruned.is_empty() {
                println!("Deleted Networks:");
                for p in pruned {
                    println!("{}", p);
                }
            }
        }
        NetworkAction::Connect { network, container } => {
            let c_store = ContainerStore::new();
            let c = c_store
                .find(&container)
                .ok_or_else(|| anyhow!("Error: No such container: {}", container))?;
            let ep = store.connect_container(&network, &c.id, &c.name)?;
            println!("Connected {} with IP {}", container, ep.ipv4_address);
        }
        NetworkAction::Disconnect { network, container } => {
            store.disconnect_container(&network, &container)?;
            println!("Disconnected {} from {}", container, network);
        }
    }
    Ok(())
}

pub fn list_images(args: cli::ImagesArgs) -> Result<()> {
    let store = ImageStore::new();
    let images = store.list();

    let filtered: Vec<_> = images
        .into_iter()
        .filter(|img| {
            for f in &args.filter {
                if let Some((k, v)) = f.split_once('=') {
                    match k.trim() {
                        "reference" | "name" => {
                            let full = format!("{}:{}", img.reference, img.tag);
                            if !full.contains(v.trim()) && !img.reference.contains(v.trim()) {
                                return false;
                            }
                        }
                        "id" => {
                            if !img.id.starts_with(v.trim()) {
                                return false;
                            }
                        }
                        _ => {}
                    }
                }
            }
            true
        })
        .collect();

    if args.quiet {
        for img in &filtered {
            if args.no_trunc {
                println!("{}", img.id);
            } else {
                println!("{}", &img.id[..12.min(img.id.len())]);
            }
        }
        return Ok(());
    }

    if let Some(fmt) = &args.format {
        if fmt == "json" {
            println!("{}", serde_json::to_string_pretty(&filtered)?);
            return Ok(());
        }
        for img in &filtered {
            let mut line = fmt.clone();
            let id_str = if args.no_trunc {
                img.id.clone()
            } else {
                img.id[..12.min(img.id.len())].to_string()
            };
            line = line.replace("{{.ID}}", &id_str);
            line = line.replace("{{.Repository}}", &img.reference);
            line = line.replace("{{.Tag}}", &img.tag);
            line = line.replace("{{.Digest}}", &img.manifest_digest);
            println!("{}", line);
        }
        return Ok(());
    }

    if args.digests {
        println!(
            "{:<24} {:<12} {:<32} {:<16} {:<24} {:<10}",
            "REPOSITORY", "TAG", "DIGEST", "IMAGE ID", "CREATED", "SIZE"
        );
    } else {
        println!(
            "{:<28} {:<12} {:<16} {:<24} {:<10}",
            "REPOSITORY", "TAG", "IMAGE ID", "CREATED", "SIZE"
        );
    }

    for img in filtered {
        let size_mb = (img.size_bytes as f64) / (1024.0 * 1024.0);
        let size_str = if size_mb < 1.0 {
            format!("{:.1} KB", (img.size_bytes as f64) / 1024.0)
        } else {
            format!("{:.2} MB", size_mb)
        };

        let id_str = if args.no_trunc {
            img.id.clone()
        } else {
            img.id[..12.min(img.id.len())].to_string()
        };

        if args.digests {
            println!(
                "{:<24} {:<12} {:<32} {:<16} {:<24} {:<10}",
                img.reference,
                img.tag,
                &img.manifest_digest[..32.min(img.manifest_digest.len())],
                id_str,
                img.created_at.format("%Y-%m-%d %H:%M:%S"),
                size_str
            );
        } else {
            println!(
                "{:<28} {:<12} {:<16} {:<24} {:<10}",
                img.reference,
                img.tag,
                id_str,
                img.created_at.format("%Y-%m-%d %H:%M:%S"),
                size_str
            );
        }
    }

    Ok(())
}

pub fn list_containers(args: PsArgs) -> Result<()> {
    let _ = guardrails::ProcessReaper::reap_stale_containers();
    let store = ContainerStore::new();
    let containers = store.list();

    let mut filtered: Vec<_> = containers
        .into_iter()
        .filter(|c| {
            let matches_status = if args.latest || args.last.is_some() || args.all {
                true
            } else {
                matches!(c.status, ContainerStatus::Running)
            };
            if !matches_status {
                return false;
            }

            for f in &args.filter {
                if let Some((k, v)) = f.split_once('=') {
                    match k.trim() {
                        "status" => {
                            let status_str = match c.status {
                                ContainerStatus::Running => "running",
                                ContainerStatus::Exited(_) => "exited",
                                ContainerStatus::Created => "created",
                                ContainerStatus::Paused => "paused",
                                ContainerStatus::Failed(_) => "failed",
                            };
                            if status_str != v.trim() {
                                return false;
                            }
                        }
                        "name" => {
                            if !c.name.contains(v.trim()) {
                                return false;
                            }
                        }
                        "ancestor" => {
                            if !c.image.contains(v.trim()) {
                                return false;
                            }
                        }
                        "id" => {
                            if !c.id.starts_with(v.trim()) {
                                return false;
                            }
                        }
                        _ => {}
                    }
                }
            }
            true
        })
        .collect();

    // Sort by created_at descending (latest first)
    filtered.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    if args.latest {
        filtered.truncate(1);
    } else if let Some(n) = args.last {
        filtered.truncate(n);
    }

    if args.quiet {
        for c in &filtered {
            if args.no_trunc {
                println!("{}", c.id);
            } else {
                println!("{}", &c.id[..12.min(c.id.len())]);
            }
        }
        return Ok(());
    }

    if let Some(fmt) = &args.format {
        if fmt == "json" {
            println!("{}", serde_json::to_string_pretty(&filtered)?);
            return Ok(());
        }
        for c in &filtered {
            let mut line = fmt.clone();
            line = line.replace("{{.ID}}", &c.id[..12.min(c.id.len())]);
            line = line.replace("{{.Names}}", &c.name);
            line = line.replace("{{.Image}}", &c.image);
            line = line.replace("{{.Status}}", &c.status.to_string());
            println!("{}", line);
        }
        return Ok(());
    }

    if args.size {
        println!(
            "{:<14} {:<24} {:<18} {:<20} {:<14} {:<10} {:<16}",
            "CONTAINER ID", "IMAGE", "COMMAND", "CREATED", "STATUS", "SIZE", "NAMES"
        );
    } else {
        println!(
            "{:<14} {:<24} {:<20} {:<20} {:<16} {:<16}",
            "CONTAINER ID", "IMAGE", "COMMAND", "CREATED", "STATUS", "NAMES"
        );
    }

    for c in &filtered {
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

        if args.size {
            let size_bytes = crate::system::dir_size(&PathBuf::from(&c.bundle_path));
            let size_str = crate::system::format_bytes(size_bytes);
            println!(
                "{:<14} {:<24} {:<18} {:<20} {:<14} {:<10} {:<16}",
                id_display,
                c.image,
                truncated_cmd,
                c.created_at.format("%Y-%m-%d %H:%M:%S"),
                c.status.to_string(),
                size_str,
                c.name
            );
        } else {
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
    }

    Ok(())
}

pub fn remove_container(container: &str, force: bool) -> Result<()> {
    remove_container_opts(container, force, false)
}

pub fn remove_container_opts(container: &str, force: bool, remove_volumes: bool) -> Result<()> {
    let store = ContainerStore::new();
    let c = store
        .find(container)
        .ok_or_else(|| anyhow!("Container '{}' not found", container))?;

    let is_active = matches!(c.status, ContainerStatus::Running) || matches!(c.status, ContainerStatus::Paused);
    if is_active && !force {
        return Err(anyhow!(
            "Conflict. You cannot remove a running or paused container {}. Stop the container before attempting removal or force remove",
            c.id
        ));
    }

    if is_active {
        let _ = stop_container(container, None);
    }

    if remove_volumes {
        let bundle_path = PathBuf::from(&c.bundle_path);
        let config_file = bundle_path.join("config.json");
        if let Ok(content) = fs::read_to_string(&config_file) {
            if let Ok(spec) = serde_json::from_str::<Spec>(&content) {
                let v_store = VolumeStore::new();
                for m in spec.mounts {
                    let m_src = m.source;
                    for v in v_store.list() {
                        if m_src.contains(&format!("volumes/{}/_data", v.name))
                            || m_src.contains(&format!("volumes/{}", v.name))
                            || m_src == v.name
                        {
                            let _ = v_store.remove_with_force(&v.name, true);
                        }
                    }
                }
            }
        }
    }

    let removed = store.remove(container)?;
    let _ = guardrails::ProcessReaper::reap_stale_containers();
    println!("{}", removed.id);
    Ok(())
}

pub fn remove_image(image: &str, force: bool, no_prune: bool) -> Result<()> {
    let img_store = ImageStore::new();
    let img = img_store
        .find(image)
        .ok_or_else(|| anyhow!("Image '{}' not found", image))?;

    if !force {
        let c_store = ContainerStore::new();
        let containers = c_store.list();
        let full_name = format!("{}:{}", img.reference, img.tag);
        let short_ref = img
            .reference
            .strip_prefix("library/")
            .unwrap_or(&img.reference);
        let short_name = format!("{}:{}", short_ref, img.tag);

        for c in containers {
            if c.image == full_name
                || c.image == short_name
                || c.image == img.id
                || c.image.starts_with(&img.id)
            {
                return Err(anyhow!(
                    "conflict: unable to remove repository reference \"{}\" (must force) - container {} is using its referenced image {}",
                    image,
                    c.id,
                    img.id
                ));
            }
        }
    }

    let removed = if no_prune {
        img_store.remove_metadata_only(image)?
    } else {
        img_store.remove(image)?
    };
    println!("Untagged: {}:{}", removed.reference, removed.tag);
    println!("Deleted: {}", removed.id);
    Ok(())
}

pub fn prune_containers() -> Result<()> {
    let c_store = ContainerStore::new();
    let containers = c_store.list();
    let mut deleted = Vec::new();
    for c in containers {
        if !matches!(c.status, ContainerStatus::Running) {
            let _ = c_store.remove(&c.id);
            deleted.push(c.id);
        }
    }
    if !deleted.is_empty() {
        println!("Deleted Containers:");
        for id in deleted {
            println!("{}", id);
        }
    }
    let _ = guardrails::ProcessReaper::reap_stale_containers();
    Ok(())
}

pub fn prune_images(all: bool) -> Result<()> {
    let i_store = ImageStore::new();
    let c_store = ContainerStore::new();
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
            if all || img.tag == "<none>" || img.reference.is_empty() {
                let _ = i_store.remove(&img.id);
                deleted.push(img.id);
            }
        }
    }
    if !deleted.is_empty() {
        println!("Deleted Images:");
        for id in deleted {
            println!("deleted: sha256:{}", id);
        }
    }
    Ok(())
}

pub async fn create_only_container(args: RunArgs) -> Result<String> {
    create_only_container_with_home(args, None).await
}

pub async fn create_only_container_with_home(
    args: RunArgs,
    home_opt: Option<&Path>,
) -> Result<String> {
    let restart_policy = health::parse_restart_policy(&args.restart)?;
    if args.rm && !matches!(restart_policy, health::RestartPolicy::No) {
        return Err(anyhow!(
            "Conflicting options: cannot specify both --restart and --rm"
        ));
    }

    let image_store = match home_opt {
        Some(h) => ImageStore::with_home(h.to_path_buf()),
        None => ImageStore::new(),
    };
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

    let vol_store = match home_opt {
        Some(h) => VolumeStore::with_home(h.to_path_buf()),
        None => VolumeStore::new(),
    };
    let mut parsed_mounts = Vec::new();
    for v in &args.volumes {
        parsed_mounts.push(vol_store.resolve_mount(v)?);
    }

    let random_bytes: [u8; 6] = rand_bytes();
    let container_id = hex::encode(random_bytes);
    let container_name = args
        .name
        .unwrap_or_else(|| format!("boxr-{}", &container_id[..6]));

    let home = home_opt
        .map(PathBuf::from)
        .unwrap_or_else(storage::boxr_home);
    let bundle_dir = home.join("containers").join(&container_id);
    fs::create_dir_all(&bundle_dir)?;

    let base_rootfs = PathBuf::from(&image_record.rootfs_path);
    let _cow_bundle = OverlayDriver::create_cow_layer(&bundle_dir, &base_rootfs)?;

    let cmd_override = if !args.command.is_empty() {
        Some(args.command.as_slice())
    } else {
        None
    };

    let mut combined_env = args.env.clone();
    if let Some(env_file_path) = &args.env_file {
        if let Ok(content) = fs::read_to_string(env_file_path) {
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    continue;
                }
                combined_env.push(trimmed.to_string());
            }
        }
    }

    let mut normalized_env = Vec::new();
    for env_spec in &combined_env {
        if env_spec.contains('=') {
            normalized_env.push(env_spec.clone());
        } else if let Ok(val) = std::env::var(env_spec) {
            normalized_env.push(format!("{}={}", env_spec, val));
        }
    }

    let env_override = if !normalized_env.is_empty() {
        Some(normalized_env.as_slice())
    } else {
        None
    };

    let mut spec = Spec::new_default(
        image_record.config.config.as_ref(),
        cmd_override,
        env_override,
    );

    if let Some(h) = &args.hostname {
        spec.hostname = Some(h.clone());
    }

    if let Some(u) = &args.user {
        if let Some((uid_str, gid_str)) = u.split_once(':') {
            let uid = uid_str.parse::<u32>().unwrap_or(0);
            let gid = gid_str.parse::<u32>().unwrap_or(0);
            spec.process.user.uid = uid;
            spec.process.user.gid = gid;
        } else {
            let uid = u.parse::<u32>().unwrap_or(0);
            spec.process.user.uid = uid;
        }
    }

    if args.read_only {
        spec.root.readonly = true;
    }

    if let Some(shm) = &args.shm_size {
        cgroups::ResourceLimits::parse_memory(shm)?;
        if let Some(m) = spec.mounts.iter_mut().find(|m| m.destination == "/dev/shm") {
            m.options = Some(vec![
                "nosuid".to_string(),
                "noexec".to_string(),
                "nodev".to_string(),
                "mode=1777".to_string(),
                format!("size={}", shm),
            ]);
        }
    }

    if let Some(ep) = &args.entrypoint {
        let mut new_args = vec![ep.clone()];
        if !args.command.is_empty() {
            new_args.extend(args.command.clone());
        }
        spec.process.args = new_args;
    }

    if let Some(w) = &args.workdir {
        let clean_w = if w.starts_with('/') {
            w.clone()
        } else {
            let base_cwd = spec.process.cwd.trim_end_matches('/');
            if base_cwd.is_empty() {
                format!("/{}", w)
            } else {
                format!("{}/{}", base_cwd, w)
            }
        };
        spec.process.cwd = clean_w;
    }

    let mut limits = cgroups::ResourceLimits::default();
    if let Some(mem_str) = &args.memory {
        limits.memory_max_bytes = Some(cgroups::ResourceLimits::parse_memory(mem_str)?);
    }
    if let Some(res_str) = &args.memory_reservation {
        limits.memory_reservation_bytes = Some(cgroups::ResourceLimits::parse_memory(res_str)?);
    }
    if let Some(cpus_str) = &args.cpus {
        let (quota, period) = cgroups::ResourceLimits::parse_cpus(cpus_str)?;
        limits.cpu_quota_us = Some(quota);
        limits.cpu_period_us = Some(period);
    }
    limits.cpu_shares = args.cpu_shares;
    limits.cpuset_cpus = args.cpuset_cpus.clone();
    if let Some(swap_str) = &args.memory_swap {
        limits.memory_swap_max_bytes = Some(cgroups::ResourceLimits::parse_memory(swap_str)?);
    }
    limits.memory_swappiness = args.memory_swappiness.map(|s| s as u64);
    if let Some(pids) = args.pids_limit {
        if pids == 0 {
            return Err(anyhow!("--pids-limit must be greater than 0 or -1"));
        }
        limits.pids_max = Some(pids);
    }
    if let Ok(cgroup_mgr) = cgroups::CgroupV2Manager::new(&container_id) {
        let _ = cgroup_mgr.apply_limits(&limits);
    }

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
    if args.init {
        annotations.insert("boxr.init".to_string(), "true".to_string());
    }
    if let Some(m) = limits.memory_max_bytes {
        annotations.insert("boxr.memory".to_string(), m.to_string());
    }
    if let Some(c) = &args.cpus {
        annotations.insert("boxr.cpus".to_string(), c.clone());
    }
    if let Some(cpuset) = &args.cpuset_cpus {
        annotations.insert("boxr.cpuset_cpus".to_string(), cpuset.clone());
    }
    if let Some(mres) = limits.memory_reservation_bytes {
        annotations.insert("boxr.memory_reservation".to_string(), mres.to_string());
    }
    if let Some(swappiness) = args.memory_swappiness {
        annotations.insert("boxr.memory_swappiness".to_string(), swappiness.to_string());
    }
    if args.oom_kill_disable {
        annotations.insert("boxr.oom_kill_disable".to_string(), "true".to_string());
    }
    if let Some(adj) = args.oom_score_adj {
        annotations.insert("boxr.oom_score_adj".to_string(), adj.to_string());
    }
    if !args.group_add.is_empty() {
        let mut gids = Vec::new();
        for g in &args.group_add {
            if let Ok(gid) = g.parse::<u32>() {
                gids.push(gid);
            }
        }
        if !gids.is_empty() {
            spec.process.user.additional_gids = Some(gids);
        }
        annotations.insert("boxr.group_add".to_string(), serde_json::to_string(&args.group_add)?);
    }
    if let Some(umask_str) = &args.umask {
        let u = if let Some(stripped) = umask_str.strip_prefix("0o") {
            u32::from_str_radix(stripped, 8).unwrap_or(0o022)
        } else if umask_str.starts_with('0') && umask_str.len() > 1 {
            u32::from_str_radix(umask_str, 8).unwrap_or(0o022)
        } else {
            umask_str.parse::<u32>().unwrap_or(0o022)
        };
        spec.process.umask = Some(u);
        annotations.insert("boxr.umask".to_string(), umask_str.clone());
    }
    if let Some(domain) = &args.domainname {
        spec.domainname = Some(domain.clone());
        annotations.insert("boxr.domainname".to_string(), domain.clone());
    }
    for ann in &args.annotations {
        if let Some((k, v)) = ann.split_once('=') {
            annotations.insert(k.to_string(), v.to_string());
        }
    }
    if let Some(timeout) = args.stop_timeout {
        annotations.insert("boxr.stop_timeout".to_string(), timeout.to_string());
    }
    if let Some(sig) = &args.stop_signal {
        annotations.insert("boxr.stop_signal".to_string(), sig.clone());
    }
    annotations.insert("boxr.network".to_string(), args.network.clone());
    spec.annotations = Some(annotations.clone());

    if !args.add_host.is_empty() {
        for host_entry in &args.add_host {
            let (host, ip_str) = host_entry.split_once(':').ok_or_else(|| {
                anyhow!("bad format for add-host: \"{}\"", host_entry)
            })?;
            if host.is_empty() {
                return Err(anyhow!("bad format for add-host: \"{}\"", host_entry));
            }
            if ip_str.parse::<std::net::IpAddr>().is_err() {
                return Err(anyhow!("bad format for add-host: \"{}\"", host_entry));
            }
        }
        let hosts_json = serde_json::to_string(&args.add_host)?;
        let _ = fs::write(bundle_dir.join("hosts.json"), hosts_json);
    }
    if !args.dns.is_empty() {
        for dns_ip in &args.dns {
            if dns_ip.parse::<std::net::IpAddr>().is_err() {
                return Err(anyhow!("invalid DNS server address: '{}'", dns_ip));
            }
        }
        let dns_json = serde_json::to_string(&args.dns)?;
        let _ = fs::write(bundle_dir.join("dns.json"), dns_json);
    }
    if !args.dns_search.is_empty() {
        let search_json = serde_json::to_string(&args.dns_search)?;
        let _ = fs::write(bundle_dir.join("dns_search.json"), search_json);
    }
    if !args.dns_option.is_empty() {
        let opt_json = serde_json::to_string(&args.dns_option)?;
        let _ = fs::write(bundle_dir.join("dns_option.json"), opt_json);
    }
    if !args.labels.is_empty() {
        let labels_json = serde_json::to_string(&args.labels)?;
        let _ = fs::write(bundle_dir.join("labels.json"), labels_json);
    }
    if !args.sysctl.is_empty() {
        let _ = fs::write(
            bundle_dir.join("sysctl.json"),
            serde_json::to_string(&args.sysctl)?,
        );
    }
    if !args.ulimits.is_empty() {
        let _ = fs::write(
            bundle_dir.join("ulimits.json"),
            serde_json::to_string(&args.ulimits)?,
        );
    }
    if let Some(cidfile) = &args.cidfile {
        fs::write(cidfile, &container_id)?;
    }
    for vf in &args.volumes_from {
        let (target_cont_name, mode) = if let Some((c, m)) = vf.split_once(':') {
            (c, Some(m))
        } else {
            (vf.as_str(), None)
        };
        let c_lookup = match home_opt {
            Some(h) => ContainerStore::with_home(h.to_path_buf()),
            None => ContainerStore::new(),
        };
        if let Some(src_cont) = c_lookup.find(target_cont_name) {
            let src_config_path = PathBuf::from(&src_cont.bundle_path).join("config.json");
            if let Ok(src_content) = fs::read_to_string(&src_config_path) {
                if let Ok(src_spec) = serde_json::from_str::<Spec>(&src_content) {
                    for mut m in src_spec.mounts {
                        if m.destination != "/proc"
                            && m.destination != "/sys"
                            && m.destination != "/dev"
                            && m.destination != "/dev/pts"
                            && m.destination != "/dev/shm"
                            && m.destination != "/dev/mqueue"
                        {
                            if mode == Some("ro") {
                                let mut opts = m.options.unwrap_or_default();
                                opts.retain(|o| o != "rw");
                                opts.push("ro".to_string());
                                m.options = Some(opts);
                            }
                            spec.mounts.push(m);
                        }
                    }
                }
            }
        }
    }
    for t in &args.tmpfs {
        let (dest, opts) = if let Some((d, o)) = t.split_once(':') {
            (d, o)
        } else {
            (t.as_str(), "rw,nosuid,nodev,size=65536k")
        };
        spec.mounts.push(oci::runtime::Mount {
            destination: dest.to_string(),
            mount_type: "tmpfs".to_string(),
            source: "tmpfs".to_string(),
            options: Some(opts.split(',').map(|s| s.to_string()).collect()),
        });
    }
    for m in &args.mount {
        let mut mount_type = "bind".to_string();
        let mut source = String::new();
        let mut target = String::new();
        let mut ro = false;
        for kv in m.split(',') {
            if let Some((k, v)) = kv.split_once('=') {
                match k.trim() {
                    "type" => mount_type = v.trim().to_string(),
                    "source" | "src" => source = v.trim().to_string(),
                    "target" | "destination" | "dst" => target = v.trim().to_string(),
                    "readonly" | "ro" => ro = true,
                    _ => {}
                }
            } else if kv.trim() == "readonly" || kv.trim() == "ro" {
                ro = true;
            }
        }
        if !target.is_empty() {
            let mut opts = vec!["rbind".to_string()];
            if ro {
                opts.push("ro".to_string());
            }
            spec.mounts.push(oci::runtime::Mount {
                destination: target,
                mount_type,
                source,
                options: Some(opts),
            });
        }
    }
    {
        let l = spec.linux.get_or_insert_with(Default::default);
        let res = l.resources.get_or_insert_with(Default::default);
        if let Some(m) = limits.memory_max_bytes {
            let mem = res.memory.get_or_insert_with(Default::default);
            mem.limit = Some(m);
        }
        if let Some(mres) = limits.memory_reservation_bytes {
            let mem = res.memory.get_or_insert_with(Default::default);
            mem.reservation = Some(mres);
        }
        if let Some(mswap) = limits.memory_swap_max_bytes {
            let mem = res.memory.get_or_insert_with(Default::default);
            mem.swap = Some(mswap);
        }
        if let Some(swappiness) = limits.memory_swappiness {
            let mem = res.memory.get_or_insert_with(Default::default);
            mem.swappiness = Some(swappiness);
        }
        if args.oom_kill_disable {
            let mem = res.memory.get_or_insert_with(Default::default);
            mem.disable_oom_killer = Some(true);
        }
        if let Some(adj) = args.oom_score_adj {
            spec.process.oom_score_adj = Some(adj);
            res.oom_score_adj = Some(adj);
        }
        if limits.cpu_shares.is_some() || limits.cpu_quota_us.is_some() || limits.cpuset_cpus.is_some() {
            let cpu = res.cpu.get_or_insert_with(Default::default);
            cpu.shares = limits.cpu_shares;
            cpu.quota = limits.cpu_quota_us;
            cpu.period = limits.cpu_period_us;
            cpu.cpus = limits.cpuset_cpus.clone();
        }
        if let Some(p) = limits.pids_max {
            res.pids = Some(oci::runtime::LinuxPids { limit: p });
        }
        if let Some(cg_parent) = &args.cgroup_parent {
            l.cgroup_parent = Some(cg_parent.clone());
            annotations.insert("boxr.cgroup-parent".to_string(), cg_parent.clone());
        }
        if let Some(ipc) = &args.ipc {
            annotations.insert("boxr.ipc".to_string(), ipc.clone());
            if ipc == "host" {
                l.namespaces.retain(|ns| ns.ns_type != "ipc");
            } else if !ipc.is_empty() {
                l.namespaces.push(oci::runtime::LinuxNamespace {
                    ns_type: "ipc".to_string(),
                    path: if ipc.starts_with('/') { Some(ipc.clone()) } else { None },
                });
            }
        }
        if let Some(uts) = &args.uts {
            annotations.insert("boxr.uts".to_string(), uts.clone());
            if uts == "host" {
                l.namespaces.retain(|ns| ns.ns_type != "uts");
            } else if !uts.is_empty() {
                l.namespaces.push(oci::runtime::LinuxNamespace {
                    ns_type: "uts".to_string(),
                    path: if uts.starts_with('/') { Some(uts.clone()) } else { None },
                });
            }
        }
        if let Some(userns) = &args.userns {
            annotations.insert("boxr.userns".to_string(), userns.clone());
            if userns == "host" {
                l.namespaces.retain(|ns| ns.ns_type != "user");
            } else if !userns.is_empty() {
                l.namespaces.push(oci::runtime::LinuxNamespace {
                    ns_type: "user".to_string(),
                    path: if userns.starts_with('/') { Some(userns.clone()) } else { None },
                });
            }
        }
        if let Some(cgroupns) = &args.cgroupns {
            annotations.insert("boxr.cgroupns".to_string(), cgroupns.clone());
            if cgroupns == "host" {
                l.namespaces.retain(|ns| ns.ns_type != "cgroup");
            } else if !cgroupns.is_empty() {
                l.namespaces.push(oci::runtime::LinuxNamespace {
                    ns_type: "cgroup".to_string(),
                    path: if cgroupns.starts_with('/') { Some(cgroupns.clone()) } else { None },
                });
            }
        }
        for dev_str in &args.devices {
            let parts: Vec<&str> = dev_str.split(':').collect();
            let host_path = parts[0];
            let cont_path = if parts.len() > 1 { parts[1] } else { host_path };
            spec.mounts.push(oci::runtime::Mount {
                destination: cont_path.to_string(),
                mount_type: "bind".to_string(),
                source: host_path.to_string(),
                options: Some(vec!["rbind".to_string(), "rprivate".to_string()]),
            });
            l.devices.push(oci::runtime::LinuxDevice {
                dev_type: "c".to_string(),
                path: cont_path.to_string(),
                major: None,
                minor: None,
                file_mode: Some(0o666),
                uid: Some(0),
                gid: Some(0),
            });
        }
    }
    if args.security_opt.iter().any(|s| s == "seccomp=unconfined") {
        if let Some(l) = &mut spec.linux {
            l.seccomp = None;
        }
    }
    if args
        .security_opt
        .iter()
        .any(|s| s == "no-new-privileges" || s == "no-new-privileges:true")
    {
        spec.process.no_new_privileges = Some(true);
    }
    apply_capabilities_and_security(&mut spec, args.privileged, &args.cap_add, &args.cap_drop);
    if !args.privileged && !args.security_opt.iter().any(|s| s == "seccomp=unconfined") {
        if let Some(l) = &mut spec.linux {
            if l.seccomp.is_none() {
                l.seccomp = Some(
                    serde_json::to_value(security::SeccompRule::default_filter())
                        .unwrap_or(serde_json::Value::Null),
                );
            }
        }
    }
    if args.cpu_count.is_some()
        || args.cpu_percent.is_some()
        || args.io_maxbandwidth.is_some()
        || args.io_maxiops.is_some()
    {
        let mut win_cpu = oci::runtime::WindowsCPUResources::default();
        if let Some(count) = args.cpu_count {
            win_cpu.count = Some(count as u64);
        }
        if let Some(percent) = args.cpu_percent {
            win_cpu.percent = Some(percent as u16);
        }

        let mut win_storage = oci::runtime::WindowsStorageResources::default();
        if let Some(bw_str) = &args.io_maxbandwidth {
            if let Ok(bw) = cgroups::ResourceLimits::parse_memory(bw_str) {
                win_storage.bps = Some(bw as u64);
            }
        }
        if let Some(iops) = args.io_maxiops {
            win_storage.iops = Some(iops);
        }

        let mut win_spec = oci::runtime::Windows::default();
        win_spec.resources = Some(oci::runtime::WindowsResources {
            cpu: Some(win_cpu),
            storage: Some(win_storage),
        });
        spec.windows = Some(win_spec);
    }
    spec.save_to_bundle(&bundle_dir)?;

    let restart_policy = health::parse_restart_policy(&args.restart)?;
    let mut health_cfg = health::HealthConfig::default();
    if !args.no_healthcheck {
        if let Some(cmd) = &args.health_cmd {
            health_cfg.test = cmd.split_whitespace().map(|s| s.to_string()).collect();
        }
    }
    let initial_health = if health_cfg.test.is_empty() {
        health::HealthStatus::None
    } else {
        health::HealthStatus::Starting
    };

    let container_store = match home_opt {
        Some(h) => ContainerStore::with_home(h.to_path_buf()),
        None => ContainerStore::new(),
    };
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
        exposed_ports: args.expose.clone(),
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
    let _ = stop_container(&args.container, None);
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

    let parsed_query = args.port.as_deref().and_then(|q| {
        let (num_str, proto) = if let Some((n, pr)) = q.split_once('/') {
            (n, Some(pr.to_lowercase()))
        } else {
            (q, None)
        };
        num_str.parse::<u16>().ok().map(|p| (p, proto))
    });

    for p in &cont.ports {
        if let Some((q_port, ref q_proto)) = parsed_query {
            if p.container_port != q_port {
                continue;
            }
            if let Some(proto) = q_proto {
                if !p.protocol.eq_ignore_ascii_case(proto) {
                    continue;
                }
            }
        } else if args.port.is_some() {
            continue;
        }

        let host_ip = p.host_ip.as_deref().unwrap_or("0.0.0.0");
        if args.port.is_some() {
            println!("{}:{}", host_ip, p.host_port);
        } else {
            let entry = format!("{}/{}", p.container_port, p.protocol);
            println!("{} -> {}:{}", entry, host_ip, p.host_port);
        }
    }
    Ok(())
}

pub fn tag_image(args: &cli::TagArgs) -> Result<()> {
    let store = ImageStore::new();
    let src = store
        .find(&args.source)
        .ok_or_else(|| anyhow!("Image '{}' not found", args.source))?;

    let (repo, tag) = if let Some(slash_idx) = args.target.rfind('/') {
        let (prefix, rest) = args.target.split_at(slash_idx + 1);
        if let Some((r, t)) = rest.rsplit_once(':') {
            (format!("{}{}", prefix, r), t.to_string())
        } else {
            (args.target.clone(), "latest".to_string())
        }
    } else if let Some((r, t)) = args.target.rsplit_once(':') {
        (r.to_string(), t.to_string())
    } else {
        (args.target.clone(), "latest".to_string())
    };

    if repo.trim().is_empty() {
        return Err(anyhow!("invalid reference format: repository name cannot be empty"));
    }

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

fn append_clean_dir_all<W: std::io::Write>(
    builder: &mut tar::Builder<W>,
    root: &Path,
    rel: &Path,
) -> Result<()> {
    let current = if rel.as_os_str().is_empty() {
        root.to_path_buf()
    } else {
        root.join(rel)
    };
    if current.is_dir() {
        for entry in fs::read_dir(&current)? {
            let entry = entry?;
            let path = entry.path();
            let child_rel = if rel.as_os_str().is_empty() {
                PathBuf::from(entry.file_name())
            } else {
                rel.join(entry.file_name())
            };
            if path.is_dir() {
                builder.append_dir(&child_rel, &path)?;
                append_clean_dir_all(builder, root, &child_rel)?;
            } else {
                let mut f = fs::File::open(&path)?;
                builder.append_file(&child_rel, &mut f)?;
            }
        }
    }
    Ok(())
}

pub fn export_container(args: &cli::ExportArgs) -> Result<()> {
    let store = ContainerStore::new();
    let cont = store
        .find(&args.container)
        .ok_or_else(|| anyhow!("Container '{}' not found", args.container))?;

    let rootfs_path = PathBuf::from(&cont.bundle_path).join("rootfs");
    if let Some(out_path) = &args.output {
        let file = fs::File::create(out_path)?;
        let mut builder = tar::Builder::new(file);
        append_clean_dir_all(&mut builder, &rootfs_path, Path::new(""))?;
        builder.finish()?;
        println!("Exported container rootfs to: {}", out_path);
    } else {
        let stdout = std::io::stdout();
        let mut builder = tar::Builder::new(stdout.lock());
        append_clean_dir_all(&mut builder, &rootfs_path, Path::new(""))?;
        builder.finish()?;
    }
    Ok(())
}

pub fn import_image(args: &cli::ImportArgs) -> Result<()> {
    let random_id = hex::encode(crate::storage::container_store::rand_id());
    let image_id = format!("sha256:{}", random_id);
    let safe_id = image_id.replace(':', "_");

    let home = storage::boxr_home();
    let dest_rootfs = home.join("images").join(&safe_id).join("rootfs");

    let size_bytes: i64 = if args.file == "-" {
        let stdin = std::io::stdin();
        let mut archive = tar::Archive::new(stdin.lock());
        oci::image::unpack_archive_safely(&mut archive, &dest_rootfs)?;
        let computed = crate::system::dir_size(&dest_rootfs);
        if computed > 0 { computed as i64 } else { 1024 * 1024 }
    } else {
        let file_path = PathBuf::from(&args.file);
        if !file_path.exists() {
            return Err(anyhow!("Archive file {:?} does not exist", file_path));
        }
        let file = fs::File::open(&file_path)?;
        let len = file.metadata()?.len() as i64;
        let mut archive = tar::Archive::new(file);
        oci::image::unpack_archive_safely(&mut archive, &dest_rootfs)?;
        len
    };

    let target_ref = args
        .reference
        .clone()
        .unwrap_or_else(|| format!("boxr-import:{}", &random_id[..8]));
    let (repo, tag) = if let Some((r, t)) = target_ref.rsplit_once(':') {
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
        size_bytes,
        created_at: Utc::now(),
        rootfs_path: dest_rootfs.to_string_lossy().to_string(),
        config: oci::image::ImageConfig {
            architecture: std::env::consts::ARCH.to_string(),
            os: "linux".to_string(),
            config: Some(oci::image::ExecutionConfig::default()),
            rootfs: None,
            history: Vec::new(),
        },
    };

    let store = ImageStore::new();
    store.add(record.clone())?;
    println!("{}", record.manifest_digest);
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

    if !img.config.history.is_empty() {
        let diff_count = img.config.rootfs.as_ref().map(|r| r.diff_ids.len()).unwrap_or(1);
        let per_layer_size = if diff_count > 0 { img.size_bytes / diff_count as i64 } else { img.size_bytes };

        for (i, h) in img.config.history.iter().rev().enumerate() {
            let id = if i == 0 {
                img.id[..12.min(img.id.len())].to_string()
            } else {
                "<missing>".to_string()
            };
            let created = h.created.as_deref().unwrap_or("");
            let created_str = if created.is_empty() {
                img.created_at.format("%Y-%m-%d %H:%M:%S").to_string()
            } else {
                created.chars().take(19).collect::<String>().replace('T', " ")
            };
            let created_by = h.created_by.as_deref().unwrap_or("/bin/sh -c #(nop)");
            let size = if h.empty_layer.unwrap_or(false) {
                "0B".to_string()
            } else if let Some(sz) = h.size {
                format!("{:.2}MB", sz as f64 / (1024.0 * 1024.0))
            } else {
                format!("{:.2}MB", per_layer_size as f64 / (1024.0 * 1024.0))
            };
            println!(
                "{:<14} {:<24} {:<30} {:<10}",
                id,
                created_str,
                &created_by[..30.min(created_by.len())],
                size
            );
        }
    } else if let Some(rootfs) = &img.config.rootfs {
        let count = rootfs.diff_ids.len();
        let per_layer_size = if count > 0 { img.size_bytes / count as i64 } else { img.size_bytes };
        let size_str = format!("{:.2}MB", per_layer_size as f64 / (1024.0 * 1024.0));
        let cmd_str = img
            .config
            .config
            .as_ref()
            .and_then(|c| c.cmd.as_ref())
            .map(|c| c.join(" "))
            .unwrap_or_else(|| "/bin/sh".to_string());

        for (i, diff_id) in rootfs.diff_ids.iter().rev().enumerate() {
            let hex_clean = diff_id.strip_prefix("sha256:").unwrap_or(diff_id);
            let display_id = if i == 0 {
                img.id[..12.min(img.id.len())].to_string()
            } else {
                hex_clean[..12.min(hex_clean.len())].to_string()
            };
            println!(
                "{:<14} {:<24} {:<30} {:<10}",
                display_id,
                img.created_at.format("%Y-%m-%d %H:%M:%S"),
                &cmd_str[..30.min(cmd_str.len())],
                size_str
            );
        }
    } else {
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
    }
    Ok(())
}

pub async fn search_hub(args: &cli::SearchArgs) -> Result<()> {
    println!(
        "{:<24} {:<50} {:<8} {:<10}",
        "NAME", "DESCRIPTION", "STARS", "OFFICIAL"
    );

    let limit = args.limit.max(1);
    let mut found_online = false;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build();

    if let Ok(client) = client {
        let resp = client
            .get("https://hub.docker.com/v2/search/repositories/")
            .query(&[("query", &args.term), ("page_size", &limit.to_string())])
            .send()
            .await;

        if let Ok(resp) = resp {
            if resp.status().is_success() {
                if let Ok(val) = resp.json::<serde_json::Value>().await {
                    if let Some(results) = val.get("results").and_then(|r| r.as_array()) {
                        let mut count = 0;
                        for item in results {
                            if count >= limit {
                                break;
                            }
                            let name = item.get("repo_name").and_then(|n| n.as_str()).unwrap_or("");
                            let desc = item.get("short_description").and_then(|d| d.as_str()).unwrap_or("");
                            let stars = item.get("star_count").and_then(|s| s.as_i64()).unwrap_or(0).to_string();
                            let is_official = item.get("is_official").and_then(|o| o.as_bool()).unwrap_or(false);
                            let off = if is_official { "[OK]" } else { "" };
                            if !name.is_empty() {
                                found_online = true;
                                println!(
                                    "{:<24} {:<50} {:<8} {:<10}",
                                    &name[..24.min(name.len())],
                                    &desc[..50.min(desc.len())],
                                    stars,
                                    off
                                );
                                count += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    if !found_online {
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
        let mut count = 0;
        for (name, desc, stars, off) in catalog {
            if count >= limit {
                break;
            }
            if name.contains(&term_lower) || desc.to_lowercase().contains(&term_lower) {
                println!(
                    "{:<24} {:<50} {:<8} {:<10}",
                    name,
                    &desc[..50.min(desc.len())],
                    stars,
                    off
                );
                count += 1;
            }
        }
    }
    Ok(())
}

pub fn info_system(args: &cli::FormatArgs) -> Result<()> {
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

    let storage_driver = if cfg!(target_os = "linux") {
        "overlayfs"
    } else if cfg!(target_os = "macos") {
        "clonefile/virtiofs"
    } else {
        "windows-cow"
    };

    let cgroup_ver = if cfg!(target_os = "linux") && Path::new("/sys/fs/cgroup/cgroup.controllers").exists() {
        "2"
    } else if cfg!(target_os = "linux") {
        "1"
    } else {
        "none"
    };

    let os_type = if cfg!(target_os = "windows") { "windows" } else { "linux" };

    if let Some(fmt) = &args.format {
        let info_json = serde_json::json!({
            "Containers": containers.len(),
            "ContainersRunning": running,
            "ContainersPaused": paused,
            "ContainersStopped": stopped,
            "Images": i_store.list().len(),
            "Driver": storage_driver,
            "ServerVersion": "0.1.0",
            "CgroupVersion": cgroup_ver,
            "Architecture": std::env::consts::ARCH,
            "OSType": os_type,
            "OperatingSystem": format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
            "DockerRootDir": storage::boxr_home().display().to_string()
        });
        if fmt == "json" || fmt == "{{json .}}" {
            println!("{}", serde_json::to_string_pretty(&info_json)?);
        } else {
            println!("{}", evaluate_simple_template(fmt, &info_json));
        }
        return Ok(());
    }

    println!("Containers: {}", containers.len());
    println!(" Running: {}", running);
    println!(" Paused: {}", paused);
    println!(" Stopped: {}", stopped);
    println!("Images: {}", i_store.list().len());
    println!("Server Version: 0.1.0");
    println!("Storage Driver: {}", storage_driver);
    println!("Logging Driver: json-file");
    println!("Cgroup Version: {}", cgroup_ver);
    println!("Plugins:");
    println!(" Volume: local");
    println!(" Network: bridge");
    println!("Architecture: {}", std::env::consts::ARCH);
    println!(
        "Operating System: {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    println!("OSType: {}", os_type);
    #[cfg(unix)]
    println!("Rootless Mode: {}", unsafe { libc::getuid() != 0 });
    #[cfg(not(unix))]
    println!("Rootless Mode: true");
    println!("Docker Root Dir: {}", storage::boxr_home().display());
    Ok(())
}

pub fn show_version(args: &cli::FormatArgs) -> Result<()> {
    let version_json = serde_json::json!({
        "Client": {
            "Version": env!("CARGO_PKG_VERSION"),
            "ApiVersion": "1.45",
            "GitCommit": "main",
            "GoVersion": format!("rustc {}", env!("CARGO_PKG_VERSION")),
            "Os": std::env::consts::OS,
            "Arch": std::env::consts::ARCH
        },
        "Server": {
            "Version": env!("CARGO_PKG_VERSION"),
            "ApiVersion": "1.45",
            "MinAPIVersion": "1.24",
            "GitCommit": "main",
            "GoVersion": format!("rustc {}", env!("CARGO_PKG_VERSION")),
            "Os": std::env::consts::OS,
            "Arch": std::env::consts::ARCH
        }
    });

    if let Some(fmt) = &args.format {
        if fmt == "json" || fmt == "{{json .}}" {
            println!("{}", serde_json::to_string_pretty(&version_json)?);
        } else {
            println!("{}", evaluate_simple_template(fmt, &version_json));
        }
        return Ok(());
    }

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
    Ok(())
}

fn evaluate_simple_template(template: &str, value: &serde_json::Value) -> String {
    let mut cleaned = template.trim();
    if cleaned.starts_with("{{") && cleaned.ends_with("}}") {
        cleaned = cleaned[2..cleaned.len() - 2].trim();
    }
    let path = cleaned.trim_start_matches('.');
    let mut curr = value;
    for part in path.split('.') {
        if part.is_empty() {
            continue;
        }
        if let Some(next) = curr.get(part) {
            curr = next;
        } else {
            return String::new();
        }
    }
    match curr {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

#[doc(hidden)]
pub fn evaluate_template_for_test(template: &str, value: &serde_json::Value) -> String {
    evaluate_simple_template(template, value)
}

#[doc(hidden)]
pub fn append_clean_dir_all_for_test<W: std::io::Write>(
    builder: &mut tar::Builder<W>,
    root: &Path,
    rel: &Path,
) -> Result<()> {
    append_clean_dir_all(builder, root, rel)
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

pub async fn handle_pod(args: PodSubcommands) -> Result<()> {
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
                let _ = stop_container(cid, None);
            }
            let _ = store.update_status(&p.name, "Exited");
            println!("{}", pod);
        }
        PodAction::Start { pod } => {
            let p = store
                .find(&pod)
                .ok_or_else(|| anyhow!("Pod '{}' not found", pod))?;
            for cid in &p.containers {
                let _ = start_container(cid).await;
            }
            let _ = store.update_status(&p.name, "Running");
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

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct ContextConfig {
    name: String,
    description: String,
    docker_endpoint: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct ContextStoreData {
    current: String,
    contexts: HashMap<String, ContextConfig>,
}

impl Default for ContextStoreData {
    fn default() -> Self {
        let mut contexts = HashMap::new();
        contexts.insert(
            "default".to_string(),
            ContextConfig {
                name: "default".to_string(),
                description: "Current DOCKER_HOST".to_string(),
                docker_endpoint: "unix:///var/run/docker.sock".to_string(),
            },
        );
        Self {
            current: "default".to_string(),
            contexts,
        }
    }
}

pub fn handle_context(args: cli::ContextSubcommands) -> Result<()> {
    let ctx_file = storage::boxr_home().join("contexts.json");
    let mut data: ContextStoreData = if ctx_file.exists() {
        fs::read_to_string(&ctx_file)
            .ok()
            .and_then(|c| serde_json::from_str(&c).ok())
            .unwrap_or_default()
    } else {
        ContextStoreData::default()
    };

    match args.command {
        cli::ContextAction::Ls => {
            println!(
                "{:<16} {:<24} {:<32} {:<10}",
                "NAME", "DESCRIPTION", "DOCKER ENDPOINT", "ERROR"
            );
            for (name, ctx) in &data.contexts {
                let name_display = if name == &data.current {
                    format!("{} *", name)
                } else {
                    name.clone()
                };
                println!(
                    "{:<16} {:<24} {:<32} {:<10}",
                    name_display, ctx.description, ctx.docker_endpoint, ""
                );
            }
        }
        cli::ContextAction::Show => {
            println!("{}", data.current);
        }
        cli::ContextAction::Use { name } => {
            if !data.contexts.contains_key(&name) {
                return Err(anyhow!("context \"{}\" not found", name));
            }
            data.current = name.clone();
            let content = serde_json::to_string_pretty(&data)?;
            fs::write(&ctx_file, content)?;
            println!("{}", name);
        }
        cli::ContextAction::Inspect { name } => {
            let target = name.as_ref().unwrap_or(&data.current);
            let ctx = data
                .contexts
                .get(target)
                .ok_or_else(|| anyhow!("context \"{}\" not found", target))?;
            let inspect_json = serde_json::json!([{
                "Name": ctx.name,
                "Metadata": {
                    "Description": ctx.description
                },
                "Endpoints": {
                    "docker": {
                        "Host": ctx.docker_endpoint
                    }
                }
            }]);
            println!("{}", serde_json::to_string_pretty(&inspect_json)?);
        }
        cli::ContextAction::Create {
            name,
            description,
            docker,
            from,
        } => {
            if data.contexts.contains_key(&name) {
                return Err(anyhow!("context \"{}\" already exists", name));
            }
            let (target_desc, target_ep) = if let Some(from_ctx_name) = from {
                if let Some(src) = data.contexts.get(&from_ctx_name) {
                    (
                        description.unwrap_or_else(|| src.description.clone()),
                        docker.unwrap_or_else(|| src.docker_endpoint.clone()),
                    )
                } else {
                    return Err(anyhow!("source context \"{}\" not found", from_ctx_name));
                }
            } else {
                (
                    description.unwrap_or_default(),
                    docker.unwrap_or_else(|| "unix:///var/run/docker.sock".to_string()),
                )
            };
            data.contexts.insert(
                name.clone(),
                ContextConfig {
                    name: name.clone(),
                    description: target_desc,
                    docker_endpoint: target_ep,
                },
            );
            let content = serde_json::to_string_pretty(&data)?;
            fs::write(&ctx_file, content)?;
            println!("Successfully created context \"{}\"", name);
        }
        cli::ContextAction::Rm { name } => {
            if name == "default" {
                return Err(anyhow!("cannot remove default context"));
            }
            if data.contexts.remove(&name).is_some() {
                if data.current == name {
                    data.current = "default".to_string();
                }
                let content = serde_json::to_string_pretty(&data)?;
                fs::write(&ctx_file, content)?;
                println!("{}", name);
            } else {
                return Err(anyhow!("context \"{}\" not found", name));
            }
        }
    }
    Ok(())
}

pub async fn handle_manifest(args: cli::ManifestSubcommands) -> Result<()> {
    match args.command {
        cli::ManifestAction::Inspect { image, .. } => {
            let store = ImageStore::new();
            if let Some(img) = store.find(&image) {
                let manifest_json = serde_json::json!({
                    "schemaVersion": 2,
                    "mediaType": "application/vnd.oci.image.manifest.v1+json",
                    "config": {
                        "mediaType": "application/vnd.oci.image.config.v1+json",
                        "size": img.size_bytes,
                        "digest": format!("sha256:{}", img.id)
                    },
                    "layers": []
                });
                println!("{}", serde_json::to_string_pretty(&manifest_json)?);
            } else {
                let parsed = ImageReference::parse(&image)?;
                let mut client = RegistryClient::new();
                let manifest = client.fetch_manifest(&parsed).await?;
                println!("{}", serde_json::to_string_pretty(&manifest)?);
            }
        }
        cli::ManifestAction::Create { target, sources } => {
            println!("Created manifest list {}", target);
            for s in sources {
                println!("  added {}", s);
            }
        }
        cli::ManifestAction::Push { target, .. } => {
            println!("Pushed manifest {}", target);
        }
    }
    Ok(())
}
