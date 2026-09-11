mod auth;
mod builder;
mod cgroups;
mod cli;
mod completions;
mod compose;
mod daemon;
mod events;
mod health;
mod network;
mod oci;
mod runtime;
mod security;
mod stats;
mod storage;
mod terminal;
mod volume;

use anyhow::{anyhow, Result};
use chrono::Utc;
use clap::Parser;
use cli::{
    AttachArgs, BuildArgs, BuilderAction, Cli, Commands, CommitArgs, ComposeArgs, ComposeSubcommand,
    CpArgs, DiffArgs, ExecArgs, LogsArgs, NetworkAction, NetworkSubcommands, PauseArgs, PsArgs,
    RenameArgs, RunArgs, SpecArgs, TopArgs, UnpauseArgs, UpdateArgs, VolumeAction, VolumeSubcommands,
    WaitArgs,
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
    ensure_directories, ContainerRecord, ContainerStatus, ContainerStore, ImageRecord, ImageStore,
    OverlayDriver,
};
use volume::VolumeStore;

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
        Commands::Stop(args) => {
            stop_container(&args.container)?;
        }
        Commands::Start(args) => {
            start_container(&args.container).await?;
        }
        Commands::Logs(args) => {
            container_logs(&args)?;
        }
        Commands::Exec(args) => {
            let code = exec_container(&args)?;
            std::process::exit(code);
        }
        Commands::Inspect(args) => {
            inspect_target(&args.target)?;
        }
        Commands::Build(args) => {
            build_image(args).await?;
        }
        Commands::Compose(args) => {
            handle_compose(args).await?;
        }
        Commands::Save(args) => {
            let output_path = args.output.map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(format!("{}.tar", args.image.replace('/', "_").replace(':', "_"))));
            auth::ImageArchiver::save(&args.image, &output_path)?;
        }
        Commands::Load(args) => {
            let input_path = args.input.map(PathBuf::from)
                .ok_or_else(|| anyhow!("Input tar archive (-i/--input) is required for load"))?;
            auth::ImageArchiver::load(&input_path)?;
        }
        Commands::Push(args) => {
            auth::RegistryPusher::push(&args.image).await?;
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
        }
        Commands::Logout(args) => {
            let server = args.server.as_deref().unwrap_or("docker.io");
            auth::CredentialStore::new().logout(server)?;
            println!("Logout Succeeded for {}", server);
        }
        Commands::Volume(args) => {
            handle_volume(args)?;
        }
        Commands::Network(args) => {
            handle_network(args)?;
        }
        Commands::Daemon(args) => {
            daemon::start_daemon(args.socket.as_deref()).await?;
        }
        Commands::Builder(args) => match args.command {
            BuilderAction::Prune => {
                let count = builder::BuildCache::prune()?;
                println!("Total reclaimed build cache entries: {}", count);
            }
        },
        Commands::Stats(args) => {
            stats::StatsCollector::display_stats(&args.containers, args.no_stream)?;
        }
        Commands::Events(args) => {
            events::EventManager::stream_events(args.since.as_deref(), args.filter.as_deref())?;
        }
        Commands::Completion(args) => {
            let shell = completions::ShellType::parse(&args.shell)?;
            println!("{}", completions::CompletionGenerator::generate(shell));
        }
        Commands::Alias(args) => {
            if args.install {
                let bin_path = completions::CompletionGenerator::install_docker_wrapper()?;
                println!("Installed docker wrapper script in: {}/docker", bin_path);
                println!("Add to your PATH:\n  export PATH=\"{}:$PATH\"", bin_path);
            } else {
                println!("alias docker=\"boxr\"");
            }
        }
        Commands::Diff(args) => {
            diff_container(&args)?;
        }
        Commands::Top(args) => {
            top_container(&args)?;
        }
        Commands::Commit(args) => {
            commit_container(&args)?;
        }
        Commands::Pause(args) => {
            pause_container(&args)?;
        }
        Commands::Unpause(args) => {
            unpause_container(&args)?;
        }
        Commands::Rename(args) => {
            rename_container(&args)?;
        }
        Commands::Wait(args) => {
            let code = wait_container(&args)?;
            std::process::exit(code);
        }
        Commands::Cp(args) => {
            cp_container(&args)?;
        }
        Commands::Update(args) => {
            update_container(&args)?;
        }
        Commands::Attach(args) => {
            attach_container(&args)?;
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

pub async fn pull_image(image_str: &str) -> Result<ImageRecord> {
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

pub async fn run_container(args: RunArgs) -> Result<i32> {
    let image_store = ImageStore::new();
    let image_record = match image_store.find(&args.image) {
        Some(record) => record,
        None => {
            println!("Unable to find image '{}' locally", args.image);
            pull_image(&args.image).await?
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
    let container_name = args.name.unwrap_or_else(|| format!("boxr-{}", &container_id[..6]));

    let home = storage::boxr_home();
    let bundle_dir = home.join("containers").join(&container_id);
    fs::create_dir_all(&bundle_dir)?;

    // Create Copy-On-Write layer (OverlayFS / fast hardlink tree)
    let base_rootfs = PathBuf::from(&image_record.rootfs_path);
    let cow_bundle = OverlayDriver::create_cow_layer(&bundle_dir, &base_rootfs)?;

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

    if let Ok(cgroup_mgr) = cgroups::CgroupV2Manager::new(&container_id) {
        let _ = cgroup_mgr.apply_limits(&limits);
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

    let spec = Spec::new_default(
        image_record.config.config.as_ref(),
        cmd_override,
        env_override,
    );

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
    };
    let mut event_attrs = HashMap::new();
    event_attrs.insert("image".to_string(), format!("{}:{}", image_record.reference, image_record.tag));
    event_attrs.insert("name".to_string(), container_name.clone());

    EventManager::record(ContainerEvent::new(
        "container", "create", &container_id, &container_name, event_attrs.clone()
    ));

    container_store.add(record)?;

    if args.detach {
        println!("{}", container_id);
        if !parsed_ports.is_empty() {
            let _ = network::rootless::PortForwardManager::start_forwarding(&parsed_ports).await;
        }
    }

    EventManager::record(ContainerEvent::new(
        "container", "start", &container_id, &container_name, event_attrs.clone()
    ));

    // Enter raw terminal mode if interactive TTY was requested
    let _term_guard = if args.interactive && args.tty {
        terminal::TerminalGuard::enter_raw_mode().ok()
    } else {
        None
    };

    // Execute the container with restart policy support
    let mut exit_code = execute_bundle(&bundle_dir, &spec, &parsed_mounts, &parsed_ports, args.detach)?;
    let mut restart_count = 0;

    while !args.detach {
        let should_restart = match &restart_policy {
            health::RestartPolicy::Always => true,
            health::RestartPolicy::OnFailure { max_retries } => exit_code != 0 && restart_count < *max_retries,
            _ => false,
        };

        if should_restart {
            restart_count += 1;
            println!("Container {} exited with code {}, restarting (attempt {})...", container_id, exit_code, restart_count);
            exit_code = execute_bundle(&bundle_dir, &spec, &parsed_mounts, &parsed_ports, false)?;
        } else {
            break;
        }
    }

    event_attrs.insert("exitCode".to_string(), exit_code.to_string());
    EventManager::record(ContainerEvent::new(
        "container", "die", &container_id, &container_name, event_attrs
    ));

    // Health check evaluation if configured
    if !health_cfg.test.is_empty() {
        let mut health_res = health::HealthCheckResult::default();
        let _ = health::check_container_health(&bundle_dir, &health_cfg, &mut health_res);
    }

    if !args.detach {
        if args.rm {
            let _ = OverlayDriver::cleanup(&cow_bundle);
            let _ = container_store.remove(&container_id);
        } else {
            let _ = container_store.update_status(&container_id, ContainerStatus::Exited(exit_code));
        }
    }

    Ok(exit_code)
}

fn stop_container(container: &str) -> Result<()> {
    let store = ContainerStore::new();
    store.update_status(container, ContainerStatus::Exited(0))?;
    println!("{}", container);
    Ok(())
}

async fn start_container(container: &str) -> Result<()> {
    let store = ContainerStore::new();
    let rec = store.find(container).ok_or_else(|| anyhow!("Container '{}' not found", container))?;

    let bundle_path = PathBuf::from(&rec.bundle_path);
    let config_file = bundle_path.join("config.json");
    let content = fs::read_to_string(&config_file)?;
    let spec: Spec = serde_json::from_str(&content)?;

    store.update_status(&rec.id, ContainerStatus::Running)?;
    let code = execute_bundle(&bundle_path, &spec, &[], &[], false)?;
    store.update_status(&rec.id, ContainerStatus::Exited(code))?;
    Ok(())
}

fn container_logs(args: &LogsArgs) -> Result<()> {
    let store = ContainerStore::new();
    let rec = store.find(&args.container).ok_or_else(|| anyhow!("Container '{}' not found", args.container))?;

    let log_path = PathBuf::from(&rec.bundle_path).join("logs.txt");
    if log_path.exists() {
        let content = fs::read_to_string(log_path)?;
        print!("{}", content);
    } else {
        println!("No logs available for container {}", args.container);
    }
    Ok(())
}

fn exec_container(args: &ExecArgs) -> Result<i32> {
    let store = ContainerStore::new();
    let rec = store.find(&args.container).ok_or_else(|| anyhow!("Container '{}' not found", args.container))?;

    if args.command.is_empty() {
        return Err(anyhow!("Command cannot be empty for exec"));
    }

    let bundle_path = PathBuf::from(&rec.bundle_path);
    exec_in_bundle(&bundle_path, &args.command, &args.env)
}

fn inspect_target(target: &str) -> Result<()> {
    let c_store = ContainerStore::new();
    if let Some(c) = c_store.find(target) {
        println!("{}", serde_json::to_string_pretty(&c)?);
        return Ok(());
    }

    let i_store = ImageStore::new();
    if let Some(i) = i_store.find(target) {
        println!("{}", serde_json::to_string_pretty(&i)?);
        return Ok(());
    }

    Err(anyhow!("No such container or image: '{}'", target))
}

fn diff_container(args: &DiffArgs) -> Result<()> {
    let c_store = ContainerStore::new();
    let cont = c_store.find(&args.container).ok_or_else(|| anyhow!("Container '{}' not found", args.container))?;

    let i_store = ImageStore::new();
    let img = i_store.find(&cont.image).ok_or_else(|| anyhow!("Image '{}' not found", cont.image))?;

    let base_rootfs = PathBuf::from(&img.rootfs_path);
    let container_rootfs = PathBuf::from(&cont.bundle_path).join("rootfs");

    let diffs = runtime::diff::FilesystemDiff::compare(&base_rootfs, &container_rootfs)?;
    for d in diffs {
        println!("{} {}", d.change_type, d.path);
    }
    Ok(())
}

fn top_container(args: &TopArgs) -> Result<()> {
    let c_store = ContainerStore::new();
    let cont = c_store.find(&args.container).ok_or_else(|| anyhow!("Container '{}' not found", args.container))?;

    let bundle_path = PathBuf::from(&cont.bundle_path);
    runtime::top::ContainerTop::list_processes(&bundle_path, &args.ps_args)?;
    Ok(())
}

fn commit_container(args: &CommitArgs) -> Result<()> {
    let c_store = ContainerStore::new();
    let cont = c_store.find(&args.container).ok_or_else(|| anyhow!("Container '{}' not found", args.container))?;

    let i_store = ImageStore::new();
    let record = i_store.commit_container(&cont, args.repo_tag.as_deref(), args.message.as_deref(), args.author.as_deref())?;

    let mut attrs = HashMap::new();
    attrs.insert("image".to_string(), format!("{}:{}", record.reference, record.tag));
    EventManager::record(ContainerEvent::new("container", "commit", &cont.id, &cont.name, attrs));

    println!("sha256:{}", record.manifest_digest);
    Ok(())
}

fn pause_container(args: &PauseArgs) -> Result<()> {
    let c_store = ContainerStore::new();
    let cont = c_store.find(&args.container).ok_or_else(|| anyhow!("Container '{}' not found", args.container))?;

    if let Ok(cgroup_mgr) = cgroups::CgroupV2Manager::new(&cont.id) {
        let _ = cgroup_mgr.freeze();
    }
    c_store.update_status(&cont.id, ContainerStatus::Paused)?;

    let mut attrs = HashMap::new();
    attrs.insert("name".to_string(), cont.name.clone());
    EventManager::record(ContainerEvent::new("container", "pause", &cont.id, &cont.name, attrs));

    println!("{}", args.container);
    Ok(())
}

fn unpause_container(args: &UnpauseArgs) -> Result<()> {
    let c_store = ContainerStore::new();
    let cont = c_store.find(&args.container).ok_or_else(|| anyhow!("Container '{}' not found", args.container))?;

    if let Ok(cgroup_mgr) = cgroups::CgroupV2Manager::new(&cont.id) {
        let _ = cgroup_mgr.unfreeze();
    }
    c_store.update_status(&cont.id, ContainerStatus::Running)?;

    let mut attrs = HashMap::new();
    attrs.insert("name".to_string(), cont.name.clone());
    EventManager::record(ContainerEvent::new("container", "unpause", &cont.id, &cont.name, attrs));

    println!("{}", args.container);
    Ok(())
}

fn rename_container(args: &RenameArgs) -> Result<()> {
    let c_store = ContainerStore::new();
    c_store.rename(&args.container, &args.new_name)?;

    let mut attrs = HashMap::new();
    attrs.insert("oldName".to_string(), args.container.clone());
    attrs.insert("newName".to_string(), args.new_name.clone());
    EventManager::record(ContainerEvent::new("container", "rename", &args.container, &args.new_name, attrs));

    Ok(())
}

fn wait_container(args: &WaitArgs) -> Result<i32> {
    let c_store = ContainerStore::new();
    loop {
        let cont = c_store.find(&args.container).ok_or_else(|| anyhow!("Container '{}' not found", args.container))?;
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
                    // Check if underlying process has finished
                    let log_file = PathBuf::from(&cont.bundle_path).join("logs.txt");
                    if log_file.exists() {
                        let _ = c_store.update_status(&cont.id, ContainerStatus::Exited(0));
                        println!("0");
                        return Ok(0);
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(300));
            }
        }
    }
}

fn cp_container(args: &CpArgs) -> Result<()> {
    runtime::cp::ContainerCopy::copy(&args.src, &args.dest)
}

fn update_container(args: &UpdateArgs) -> Result<()> {
    let c_store = ContainerStore::new();
    let cont = c_store.find(&args.container).ok_or_else(|| anyhow!("Container '{}' not found", args.container))?;

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
    EventManager::record(ContainerEvent::new("container", "update", &cont.id, &cont.name, attrs));

    println!("{}", args.container);
    Ok(())
}

fn attach_container(args: &AttachArgs) -> Result<()> {
    let c_store = ContainerStore::new();
    let cont = c_store.find(&args.container).ok_or_else(|| anyhow!("Container '{}' not found", args.container))?;

    println!("Attaching to container '{}' (Press Ctrl+C to detach)...", args.container);

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
            if matches!(current.status, ContainerStatus::Exited(_)) || matches!(current.status, ContainerStatus::Failed(_)) {
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

async fn build_image(args: BuildArgs) -> Result<()> {
    let builder = builder::ImageBuilder::new();
    let context_dir = PathBuf::from(&args.path).canonicalize()?;
    let dockerfile_path = if Path::new(&args.file).is_absolute() {
        PathBuf::from(&args.file)
    } else {
        context_dir.join(&args.file)
    };

    builder.build(builder::BuildOptions {
        context_dir,
        dockerfile_path,
        tag: args.tag,
        no_cache: args.no_cache,
    }).await?;

    Ok(())
}

async fn handle_compose(args: ComposeArgs) -> Result<()> {
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
            println!("{:<14} {:<24} {:<20} {:<16}", "CONTAINER ID", "NAME", "IMAGE", "STATUS");
            for c in containers {
                println!("{:<14} {:<24} {:<20} {:<16}", &c.id[..12.min(c.id.len())], c.name, c.image, c.status.to_string());
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

fn handle_volume(args: VolumeSubcommands) -> Result<()> {
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
            let vol = store.find(&name).ok_or_else(|| anyhow!("Volume '{}' not found", name))?;
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

fn handle_network(args: NetworkSubcommands) -> Result<()> {
    let store = NetworkStore::new();
    match args.command {
        NetworkAction::Create { name, subnet, gateway } => {
            let net = store.create(&name, subnet.as_deref(), gateway.as_deref())?;
            println!("{}", net.id);
        }
        NetworkAction::Ls => {
            let nets = store.list();
            println!("{:<14} {:<20} {:<12} {:<20}", "NETWORK ID", "NAME", "DRIVER", "SCOPE");
            for n in nets {
                println!("{:<14} {:<20} {:<12} {:<20}", &n.id[..12.min(n.id.len())], n.name, n.driver, "local");
            }
        }
        NetworkAction::Inspect { name } => {
            let net = store.find(&name).ok_or_else(|| anyhow!("Network '{}' not found", name))?;
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

#[allow(dead_code)]
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

