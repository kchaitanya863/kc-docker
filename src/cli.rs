use clap::{Args, Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "boxr",
    author = "Boxr Contributors",
    version,
    about = "A fast, lightweight OCI container engine and runtime written in Rust",
    long_about = "boxr is a complete Open Container Initiative (OCI) compliant container engine, image builder, compose orchestrator, and runtime written in Rust."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Pull an image from an OCI registry (Docker Hub, GHCR, Quay, etc.)
    Pull(PullArgs),

    /// Run a command in a new container
    Run(RunArgs),

    /// Create a new container without starting it
    Create(RunArgs),

    /// Restart one or more containers
    Restart(RestartArgs),

    /// List port mappings or a specific mapping for the container
    Port(PortArgs),

    /// Create a tag TARGET_IMAGE that refers to SOURCE_IMAGE
    Tag(TagArgs),

    /// Export a container's filesystem as a tar archive
    Export(ExportArgs),

    /// Import the contents from a tarball to create a filesystem image
    Import(ImportArgs),

    /// Show the history of an image
    History(HistoryArgs),

    /// Search Docker Hub for images
    Search(SearchArgs),

    /// Display system-wide information
    Info,

    /// Show the boxr version information
    Version,

    /// Stop a running container
    Stop(StopArgs),

    /// Start a stopped container
    Start(StartArgs),

    /// Fetch the logs of a container
    Logs(LogsArgs),

    /// Run a command in an existing container
    Exec(ExecArgs),

    /// Return low-level information on Boxr objects (containers, images)
    Inspect(InspectArgs),

    /// Build an image from a Dockerfile
    Build(BuildArgs),

    /// Define and run multi-container applications with Boxr Compose
    Compose(ComposeArgs),

    /// Manage containers
    Container(ContainerSubcommands),

    /// Manage images
    Image(ImageSubcommands),

    /// Manage volumes
    Volume(VolumeSubcommands),

    /// Manage networks
    Network(NetworkSubcommands),

    /// Run the Boxr daemon background API server
    Daemon(DaemonArgs),

    /// List local images
    Images(ImagesArgs),

    /// List containers
    Ps(PsArgs),

    /// Save one or more images to a tar archive
    Save(SaveArgs),

    /// Load an image from a tar archive
    Load(LoadArgs),

    /// Push an image to an OCI registry
    Push(PushArgs),

    /// Log in to an OCI registry
    Login(LoginArgs),

    /// Log out from an OCI registry
    Logout(LogoutArgs),

    /// Manage builds and build cache
    Builder(BuilderSubcommands),

    /// Inspect changes to files or directories on a container's filesystem
    Diff(DiffArgs),

    /// Display the running processes of a container
    Top(TopArgs),

    /// Create a new image from a container's changes
    Commit(CommitArgs),

    /// Pause all processes within one or more containers
    Pause(PauseArgs),

    /// Unpause all processes within one or more containers
    Unpause(UnpauseArgs),

    /// Rename a container
    Rename(RenameArgs),

    /// Block until one or more containers stop, then print their exit codes
    Wait(WaitArgs),

    /// Copy files/folders between a container and the local filesystem
    Cp(CpArgs),

    /// Update configuration of one or more containers
    Update(UpdateArgs),

    /// Attach local standard input, output, and error streams to a running container
    Attach(AttachArgs),

    /// Kill one or more running containers
    Kill(KillArgs),

    /// Manage Boxr system (disk usage, prune)
    System(SystemSubcommands),

    /// Remove one or more containers
    Rm(RmArgs),

    /// Remove one or more images
    Rmi(RmiArgs),

    /// Display a live stream of container(s) resource usage statistics
    Stats(StatsArgs),

    /// Get real time events from the server
    Events(EventsArgs),

    /// Generate shell completion scripts (bash, zsh, fish)
    Completion(CompletionArgs),

    /// Manage pods (groups of containers sharing network and namespaces)
    Pod(PodSubcommands),

    /// Play a pod from a structured file (e.g. Kubernetes YAML)
    Play(PlaySubcommands),

    /// Generate structured data (e.g. Kubernetes YAML) from containers or pods
    Generate(GenerateSubcommands),

    /// Run a command in a new user namespace
    Unshare(UnshareArgs),

    /// Docker CLI drop-in alias / wrapper
    Alias(AliasArgs),

    /// Generate a standard OCI runtime specification (config.json)
    Spec(SpecArgs),

    /// Manage Docker contexts
    Context(ContextSubcommands),

    /// Manage Docker image manifests and manifest lists
    Manifest(ManifestSubcommands),

    /// Manage the Boxr background daemon service (launchd on macOS, systemd on Linux)
    Service(ServiceArgs),
}

#[derive(Args, Debug)]
pub struct ServiceArgs {
    #[command(subcommand)]
    pub action: ServiceAction,
}

#[derive(Subcommand, Debug)]
pub enum ServiceAction {
    /// Install daemon service with autostart at login / boot
    Install,
    /// Start the background service
    Start,
    /// Stop the background service
    Stop,
    /// View the live service status
    Status,
    /// Uninstall the service definition
    Uninstall,
}

#[derive(Args, Debug, Clone)]
pub struct PullArgs {
    /// Set platform if server is multi-platform (e.g. linux/amd64, linux/arm64)
    #[arg(long = "platform")]
    pub platform: Option<String>,

    pub image: String,
}

#[derive(Args, Debug, Clone)]
pub struct RunArgs {
    #[arg(short = 'i', long = "interactive")]
    pub interactive: bool,

    #[arg(short = 't', long = "tty")]
    pub tty: bool,

    #[arg(short = 'd', long = "detach")]
    pub detach: bool,

    #[arg(long = "rm")]
    pub rm: bool,

    #[arg(long = "name")]
    pub name: Option<String>,

    #[arg(short = 'e', long = "env")]
    pub env: Vec<String>,

    #[arg(short = 'p', long = "publish")]
    pub ports: Vec<String>,

    #[arg(short = 'v', long = "volume")]
    pub volumes: Vec<String>,

    /// Working directory inside the container
    #[arg(short = 'w', long = "workdir")]
    pub workdir: Option<String>,

    #[arg(short = 'm', long = "memory")]
    pub memory: Option<String>,

    /// Set metadata on container (format: <key>=<value>)
    #[arg(short = 'l', long = "label")]
    pub labels: Vec<String>,

    /// Set custom DNS servers
    #[arg(long = "dns")]
    pub dns: Vec<String>,

    /// Write the container ID to the file
    #[arg(long = "cidfile")]
    pub cidfile: Option<String>,

    #[arg(long = "cpus")]
    pub cpus: Option<String>,

    #[arg(long = "pids-limit")]
    pub pids_limit: Option<i64>,

    #[arg(long = "rootless", default_value_t = true)]
    pub rootless: bool,

    #[arg(long = "restart", default_value = "no")]
    pub restart: String,

    #[arg(long = "health-cmd")]
    pub health_cmd: Option<String>,

    /// Set platform if server is multi-platform (e.g. linux/amd64, linux/arm64)
    #[arg(long = "platform")]
    pub platform: Option<String>,

    /// Connect a container to a network (pasta, bridge, host, none)
    #[arg(long = "network", alias = "net", default_value = "auto")]
    pub network: String,

    /// Give extended privileges to this container
    #[arg(long = "privileged")]
    pub privileged: bool,

    /// GPU devices to add to the container ('all' to pass-through available GPUs)
    #[arg(long = "gpus")]
    pub gpus: Option<String>,

    /// Overwrite the default ENTRYPOINT of the image
    #[arg(long = "entrypoint")]
    pub entrypoint: Option<String>,

    /// Read in a file of environment variables
    #[arg(long = "env-file")]
    pub env_file: Option<String>,

    /// Username or UID (format: <name|uid>[:<group|gid>])
    #[arg(short = 'u', long = "user")]
    pub user: Option<String>,

    /// Container host name
    #[arg(long = "hostname")]
    pub hostname: Option<String>,

    /// Add a custom host-to-IP mapping (host:ip)
    #[arg(long = "add-host")]
    pub add_host: Vec<String>,

    /// Size of /dev/shm (e.g. 64m, 1g)
    #[arg(long = "shm-size")]
    pub shm_size: Option<String>,

    /// Add Linux capabilities
    #[arg(long = "cap-add")]
    pub cap_add: Vec<String>,

    /// Drop Linux capabilities
    #[arg(long = "cap-drop")]
    pub cap_drop: Vec<String>,

    /// Mount the container's root filesystem as read only
    #[arg(long = "read-only")]
    pub read_only: bool,

    /// Run an init inside the container that forwards signals and reaps processes
    #[arg(long = "init")]
    pub init: bool,

    /// Mount a tmpfs directory
    #[arg(long = "tmpfs")]
    pub tmpfs: Vec<String>,

    /// Add a host device to the container
    #[arg(long = "device")]
    pub devices: Vec<String>,

    /// Security Options
    #[arg(long = "security-opt")]
    pub security_opt: Vec<String>,

    /// CPU shares (relative weight)
    #[arg(short = 'c', long = "cpu-shares")]
    pub cpu_shares: Option<u64>,

    /// CPUs in which to allow execution (e.g. 0-3, 0,1)
    #[arg(long = "cpuset-cpus")]
    pub cpuset_cpus: Option<String>,

    /// Swap limit equal to memory plus swap
    #[arg(long = "memory-swap")]
    pub memory_swap: Option<String>,

    /// Memory soft limit
    #[arg(long = "memory-reservation")]
    pub memory_reservation: Option<String>,

    /// Set custom DNS search domains
    #[arg(long = "dns-search")]
    pub dns_search: Vec<String>,

    /// Set DNS options
    #[arg(long = "dns-option", alias = "dns-opt")]
    pub dns_option: Vec<String>,

    /// Expose a port or a range of ports
    #[arg(long = "expose")]
    pub expose: Vec<String>,

    /// Sysctl options (format: <key>=<value>)
    #[arg(long = "sysctl")]
    pub sysctl: Vec<String>,

    /// Timeout (in seconds) to stop a container
    #[arg(long = "stop-timeout")]
    pub stop_timeout: Option<u32>,

    /// Signal to stop the container
    #[arg(long = "stop-signal")]
    pub stop_signal: Option<String>,

    /// Add an annotation to the container (format: <key>=<value>)
    #[arg(long = "annotation")]
    pub annotations: Vec<String>,

    /// Ulimit options (format: <type>=<soft>:<hard>)
    #[arg(long = "ulimit")]
    pub ulimits: Vec<String>,

    /// IPC mode to use (host|private|shareable)
    #[arg(long = "ipc")]
    pub ipc: Option<String>,

    /// PID namespace to use (host|container:<id>)
    #[arg(long = "pid")]
    pub pid: Option<String>,

    /// UTS namespace to use (host|private)
    #[arg(long = "uts")]
    pub uts: Option<String>,

    /// User namespace to use (host|private)
    #[arg(long = "userns")]
    pub userns: Option<String>,

    /// Cgroup namespace to use (host|private)
    #[arg(long = "cgroupns")]
    pub cgroupns: Option<String>,

    /// Optional parent cgroup for the container
    #[arg(long = "cgroup-parent")]
    pub cgroup_parent: Option<String>,

    /// Container isolation technology (default|process|hyperv)
    #[arg(long = "isolation")]
    pub isolation: Option<String>,

    /// CPU count (Windows only)
    #[arg(long = "cpu-count")]
    pub cpu_count: Option<i64>,

    /// CPU percent (Windows only)
    #[arg(long = "cpu-percent")]
    pub cpu_percent: Option<i64>,

    /// Maximum IO bandwidth limit for the system drive (Windows only)
    #[arg(long = "io-maxbandwidth")]
    pub io_maxbandwidth: Option<String>,

    /// Maximum IOps limit for the system drive (Windows only)
    #[arg(long = "io-maxiops")]
    pub io_maxiops: Option<u64>,

    /// Publish all exposed ports to random ports
    #[arg(short = 'P', long = "publish-all")]
    pub publish_all: bool,

    /// IPv4 address (e.g., 172.30.100.104)
    #[arg(long = "ip")]
    pub ip: Option<String>,

    /// IPv6 address (e.g., 2001:db8::33)
    #[arg(long = "ip6")]
    pub ip6: Option<String>,

    /// Container MAC address (e.g., 92:d0:c6:0a:29:33)
    #[arg(long = "mac-address")]
    pub mac_address: Option<String>,

    /// Add link to another container
    #[arg(long = "link")]
    pub link: Vec<String>,

    /// Add network-scoped alias for the container
    #[arg(long = "network-alias", alias = "net-alias")]
    pub network_alias: Vec<String>,

    /// Attach a filesystem mount to the container
    #[arg(long = "mount")]
    pub mount: Vec<String>,

    /// Time between running the check (ms|s|m|h)
    #[arg(long = "health-interval")]
    pub health_interval: Option<String>,

    /// Maximum time to allow one check to run (ms|s|m|h)
    #[arg(long = "health-timeout")]
    pub health_timeout: Option<String>,

    /// Consecutive failures needed to report unhealthy
    #[arg(long = "health-retries")]
    pub health_retries: Option<u32>,

    /// Start period for the container to initialize (ms|s|m|h)
    #[arg(long = "health-start-period")]
    pub health_start_period: Option<String>,

    /// Time between running the check during the start period (ms|s|m|h)
    #[arg(long = "health-start-interval")]
    pub health_start_interval: Option<String>,

    /// Disable any container-specified HEALTHCHECK
    #[arg(long = "no-healthcheck")]
    pub no_healthcheck: bool,

    /// Attach to STDIN, STDOUT or STDERR
    #[arg(short = 'a', long = "attach")]
    pub attach: Vec<String>,

    /// Pull image before running (always|missing|never)
    #[arg(long = "pull")]
    pub pull: Option<String>,

    /// Suppress the pull output
    #[arg(short = 'q', long = "quiet")]
    pub quiet: bool,

    /// Logging driver for the container
    #[arg(long = "log-driver")]
    pub log_driver: Option<String>,

    /// Log driver options
    #[arg(long = "log-opt")]
    pub log_opt: Vec<String>,

    /// Disable OOM Killer
    #[arg(long = "oom-kill-disable")]
    pub oom_kill_disable: bool,

    /// Tune host's OOM preferences (-1000 to 1000)
    #[arg(long = "oom-score-adj")]
    pub oom_score_adj: Option<i32>,

    /// Add additional groups to join
    #[arg(long = "group-add")]
    pub group_add: Vec<String>,

    /// Read in a line delimited file of labels
    #[arg(long = "label-file")]
    pub label_file: Option<String>,

    /// Set umask for the container
    #[arg(long = "umask")]
    pub umask: Option<String>,

    /// Container NIS domain name
    #[arg(long = "domainname")]
    pub domainname: Option<String>,

    /// Override the key sequence for detaching a container
    #[arg(long = "detach-keys")]
    pub detach_keys: Option<String>,

    /// Block IO (relative weight), between 10 and 1000, or 0 to disable
    #[arg(long = "blkio-weight")]
    pub blkio_weight: Option<u16>,

    /// Block IO weight (relative device weight)
    #[arg(long = "blkio-weight-device")]
    pub blkio_weight_device: Vec<String>,

    /// Limit CPU CFS (Completely Fair Scheduler) period
    #[arg(long = "cpu-period")]
    pub cpu_period: Option<u64>,

    /// Limit CPU CFS (Completely Fair Scheduler) quota
    #[arg(long = "cpu-quota")]
    pub cpu_quota: Option<i64>,

    /// Limit CPU real-time period in microseconds
    #[arg(long = "cpu-rt-period")]
    pub cpu_rt_period: Option<i64>,

    /// Limit CPU real-time runtime in microseconds
    #[arg(long = "cpu-rt-runtime")]
    pub cpu_rt_runtime: Option<i64>,

    /// MEMs in which to allow execution (0-3, 0,1)
    #[arg(long = "cpuset-mems")]
    pub cpuset_mems: Option<String>,

    /// Add a rule to the cgroup allowed devices list
    #[arg(long = "device-cgroup-rule")]
    pub device_cgroup_rule: Vec<String>,

    /// Limit read rate (bytes per second) from a device
    #[arg(long = "device-read-bps")]
    pub device_read_bps: Vec<String>,

    /// Limit read rate (IO per second) from a device
    #[arg(long = "device-read-iops")]
    pub device_read_iops: Vec<String>,

    /// Limit write rate (bytes per second) to a device
    #[arg(long = "device-write-bps")]
    pub device_write_bps: Vec<String>,

    /// Limit write rate (IO per second) to a device
    #[arg(long = "device-write-iops")]
    pub device_write_iops: Vec<String>,

    /// Container IPv4/IPv6 link-local addresses
    #[arg(long = "link-local-ip")]
    pub link_local_ip: Vec<String>,

    /// Tune container memory swappiness (0 to 100)
    #[arg(long = "memory-swappiness")]
    pub memory_swappiness: Option<i64>,

    /// Runtime to use for this container
    #[arg(long = "runtime")]
    pub runtime: Option<String>,

    /// Proxy received signals to the process
    #[arg(long = "sig-proxy", default_value_t = true)]
    pub sig_proxy: bool,

    /// Storage driver options for the container
    #[arg(long = "storage-opt")]
    pub storage_opt: Vec<String>,

    /// Bind mount Docker API socket and required auth
    #[arg(long = "use-api-socket")]
    pub use_api_socket: bool,

    /// Optional volume driver for the container
    #[arg(long = "volume-driver")]
    pub volume_driver: Option<String>,

    /// Mount volumes from the specified container(s)
    #[arg(long = "volumes-from")]
    pub volumes_from: Vec<String>,

    pub image: String,

    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub command: Vec<String>,
}

#[derive(Args, Debug)]
pub struct SaveArgs {
    #[arg(short = 'o', long = "output")]
    pub output: Option<String>,
    pub image: String,
}

#[derive(Args, Debug)]
pub struct LoadArgs {
    #[arg(short = 'i', long = "input")]
    pub input: Option<String>,
}

#[derive(Args, Debug)]
pub struct PushArgs {
    pub image: String,
}

#[derive(Args, Debug)]
pub struct LoginArgs {
    #[arg(short = 'u', long = "username")]
    pub username: Option<String>,
    #[arg(short = 'p', long = "password")]
    pub password: Option<String>,
    pub server: Option<String>,
}

#[derive(Args, Debug)]
pub struct LogoutArgs {
    pub server: Option<String>,
}

#[derive(Args, Debug)]
pub struct StopArgs {
    /// Seconds to wait before killing the container
    #[arg(short = 't', long = "time")]
    pub time: Option<i32>,

    /// Signal to send to the container
    #[arg(short = 's', long = "signal")]
    pub signal: Option<String>,

    #[arg(required = true)]
    pub containers: Vec<String>,
}

#[derive(Args, Debug)]
pub struct StartArgs {
    /// Attach STDOUT/STDERR and forward signals
    #[arg(short = 'a', long = "attach")]
    pub attach: bool,

    /// Attach container's STDIN
    #[arg(short = 'i', long = "interactive")]
    pub interactive: bool,

    /// Restore from this checkpoint
    #[arg(long = "checkpoint")]
    pub checkpoint: Option<String>,

    /// Use a custom checkpoint storage directory
    #[arg(long = "checkpoint-dir")]
    pub checkpoint_dir: Option<String>,

    /// Override the key sequence for detaching a container
    #[arg(long = "detach-keys")]
    pub detach_keys: Option<String>,

    #[arg(required = true)]
    pub containers: Vec<String>,
}

#[derive(Args, Debug)]
pub struct LogsArgs {
    /// Follow log output
    #[arg(short = 'f', long = "follow")]
    pub follow: bool,

    /// Show timestamps
    #[arg(short = 't', long = "timestamps")]
    pub timestamps: bool,

    /// Show extra details provided to logs
    #[arg(long = "details")]
    pub details: bool,

    /// Show logs since timestamp (e.g. 2013-01-02T13:23:37Z) or relative (e.g. 42m)
    #[arg(long = "since")]
    pub since: Option<String>,

    /// Show logs before a timestamp (e.g. 2013-01-02T13:23:37Z) or relative (e.g. 42m)
    #[arg(long = "until")]
    pub until: Option<String>,

    /// Number of lines to show from the end of the logs
    #[arg(short = 'n', long = "tail")]
    pub tail: Option<usize>,

    pub container: String,
}

#[derive(Args, Debug, Clone)]
pub struct ExecArgs {
    #[arg(short = 'i', long = "interactive")]
    pub interactive: bool,

    /// Allocate a pseudo-TTY
    #[arg(short = 't', long = "tty")]
    pub tty: bool,

    /// Detached mode: run command in the background
    #[arg(short = 'd', long = "detach")]
    pub detach: bool,

    /// Give extended privileges to the command
    #[arg(long = "privileged")]
    pub privileged: bool,

    /// Read in a file of environment variables
    #[arg(long = "env-file")]
    pub env_file: Option<String>,

    /// Override the key sequence for detaching a container
    #[arg(long = "detach-keys")]
    pub detach_keys: Option<String>,

    /// Working directory inside the container
    #[arg(short = 'w', long = "workdir")]
    pub workdir: Option<String>,

    /// Username or UID (format: <name|uid>)
    #[arg(short = 'u', long = "user")]
    pub user: Option<String>,

    #[arg(short = 'e', long = "env")]
    pub env: Vec<String>,

    pub container: String,

    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub command: Vec<String>,
}

#[derive(Args, Debug)]
pub struct InspectArgs {
    /// Format output using a custom template: 'json' or Go template
    #[arg(short = 'f', long = "format")]
    pub format: Option<String>,

    /// Display total file sizes if the type is container
    #[arg(short = 's', long = "size")]
    pub size: bool,

    /// Return JSON for specified type (container|image)
    #[arg(long = "type")]
    pub obj_type: Option<String>,

    pub target: String,
}

#[derive(Args, Debug, Clone)]
pub struct BuildArgs {
    #[arg(short = 't', long = "tag")]
    pub tags: Vec<String>,

    #[arg(short = 'f', long = "file", default_value = "Dockerfile")]
    pub file: String,

    #[arg(long = "no-cache")]
    pub no_cache: bool,

    /// Set build-time variables
    #[arg(long = "build-arg")]
    pub build_args: Vec<String>,

    /// Set the target build stage to build
    #[arg(long = "target")]
    pub target: Option<String>,

    /// Add a custom host-to-IP mapping (host:ip)
    #[arg(long = "add-host")]
    pub add_host: Vec<String>,

    /// Set memory limit for build
    #[arg(short = 'm', long = "memory")]
    pub memory: Option<String>,

    /// Size of /dev/shm
    #[arg(long = "shm-size")]
    pub shm_size: Option<String>,

    /// Always remove intermediate containers
    #[arg(long = "rm", default_value_t = true)]
    pub rm: bool,

    /// Images to consider as cache sources
    #[arg(long = "cache-from")]
    pub cache_from: Vec<String>,

    /// Compress the build context using gzip
    #[arg(long = "compress")]
    pub compress: bool,

    /// Always remove intermediate containers
    #[arg(long = "force-rm")]
    pub force_rm: bool,

    /// Write the image ID to the file
    #[arg(long = "iidfile")]
    pub iidfile: Option<String>,

    /// Set metadata for an image
    #[arg(long = "label")]
    pub labels: Vec<String>,

    /// Set platform if server is multi-platform capable
    #[arg(long = "platform")]
    pub platform: Option<String>,

    /// Always attempt to pull a newer version of the image
    #[arg(long = "pull")]
    pub pull: bool,

    /// Suppress the build output and print image ID on success
    #[arg(short = 'q', long = "quiet")]
    pub quiet: bool,

    /// Ulimit options
    #[arg(long = "ulimit")]
    pub ulimits: Vec<String>,

    /// Container isolation technology
    #[arg(long = "isolation")]
    pub isolation: Option<String>,

    /// Set the parent cgroup for the RUN instructions during build
    #[arg(long = "cgroup-parent")]
    pub cgroup_parent: Option<String>,

    /// Limit the CPU CFS (Completely Fair Scheduler) period
    #[arg(long = "cpu-period")]
    pub cpu_period: Option<u64>,

    /// Limit the CPU CFS (Completely Fair Scheduler) quota
    #[arg(long = "cpu-quota")]
    pub cpu_quota: Option<i64>,

    /// CPU shares (relative weight)
    #[arg(short = 'c', long = "cpu-shares")]
    pub cpu_shares: Option<u64>,

    /// CPUs in which to allow execution (0-3, 0,1)
    #[arg(long = "cpuset-cpus")]
    pub cpuset_cpus: Option<String>,

    /// MEMs in which to allow execution (0-3, 0,1)
    #[arg(long = "cpuset-mems")]
    pub cpuset_mems: Option<String>,

    /// Swap limit equal to memory plus swap
    #[arg(long = "memory-swap")]
    pub memory_swap: Option<String>,

    /// Set the networking mode for the RUN instructions during build
    #[arg(long = "network")]
    pub network: Option<String>,

    /// Security options
    #[arg(long = "security-opt")]
    pub security_opt: Vec<String>,

    /// Squash newly built layers into a single new layer
    #[arg(long = "squash")]
    pub squash: bool,

    #[arg(default_value = ".")]
    pub path: String,
}

#[derive(Args, Debug)]
pub struct BuilderSubcommands {
    #[command(subcommand)]
    pub command: BuilderAction,
}

#[derive(Subcommand, Debug)]
pub enum BuilderAction {
    /// Prune build cache
    Prune,
}

#[derive(Args, Debug)]
pub struct ComposeArgs {
    #[arg(short = 'f', long = "file", default_value = "docker-compose.yml")]
    pub file: String,

    #[command(subcommand)]
    pub command: ComposeSubcommand,
}

#[derive(Subcommand, Debug)]
pub enum ComposeSubcommand {
    Up(ComposeUpArgs),
    Down(ComposeDownArgs),
    Ps,
    Logs(ComposeLogsArgs),
}

#[derive(Args, Debug)]
pub struct ComposeUpArgs {
    #[arg(short = 'd', long = "detach")]
    pub detach: bool,

    #[arg(long = "build")]
    pub build: bool,
}

#[derive(Args, Debug)]
pub struct ComposeDownArgs {
    #[arg(short = 'v', long = "volumes")]
    pub volumes: bool,
}

#[derive(Args, Debug)]
pub struct ComposeLogsArgs {
    pub service: Option<String>,
}

#[derive(Args, Debug)]
pub struct ContainerSubcommands {
    #[command(subcommand)]
    pub command: ContainerAction,
}

#[derive(Subcommand, Debug)]
pub enum ContainerAction {
    Run(RunArgs),
    Create(RunArgs),
    Start(StartArgs),
    Stop(StopArgs),
    Restart(RestartArgs),
    Kill(KillArgs),
    Rm(RmArgs),
    Pause(PauseArgs),
    Unpause(UnpauseArgs),
    Wait(WaitArgs),
    Exec(ExecArgs),
    Attach(AttachArgs),
    Logs(LogsArgs),
    #[command(alias = "ps")]
    Ls(PsArgs),
    Inspect(InspectArgs),
    Top(TopArgs),
    Port(PortArgs),
    Cp(CpArgs),
    Diff(DiffArgs),
    Prune(ContainerPruneArgs),
    Update(UpdateArgs),
}

#[derive(Args, Debug, Clone, Default)]
pub struct ContainerPruneArgs {
    #[arg(short = 'f', long = "force")]
    pub force: bool,
}

#[derive(Args, Debug)]
pub struct ImageSubcommands {
    #[command(subcommand)]
    pub command: ImageAction,
}

#[derive(Subcommand, Debug)]
pub enum ImageAction {
    #[command(alias = "list")]
    Ls(ImagesArgs),
    Build(BuildArgs),
    Pull(PullArgs),
    Push(PushArgs),
    Tag(TagArgs),
    #[command(alias = "rmi")]
    Rm(RmiArgs),
    Inspect(InspectArgs),
    History(HistoryArgs),
    Save(SaveArgs),
    Load(LoadArgs),
    Import(ImportArgs),
    Prune(ImagePruneArgs),
}

#[derive(Args, Debug, Clone, Default)]
pub struct ImagePruneArgs {
    #[arg(short = 'a', long = "all")]
    pub all: bool,

    #[arg(short = 'f', long = "force")]
    pub force: bool,
}

#[derive(Args, Debug)]
pub struct VolumeSubcommands {
    #[command(subcommand)]
    pub command: VolumeAction,
}

#[derive(Subcommand, Debug)]
pub enum VolumeAction {
    Create {
        name: Option<String>,
        /// Specify volume driver name (default "local")
        #[arg(short = 'd', long = "driver", default_value = "local")]
        driver: String,
        /// Set driver specific options
        #[arg(short = 'o', long = "opt")]
        opts: Vec<String>,
        /// Set metadata for a volume
        #[arg(long = "label")]
        labels: Vec<String>,
        /// Cluster Volume availability (active, pause, drain)
        #[arg(long = "availability")]
        availability: Option<String>,
        /// Cluster Volume group
        #[arg(long = "group")]
        group: Option<String>,
        /// Minimum size of the Cluster Volume in bytes
        #[arg(long = "limit-bytes")]
        limit_bytes: Option<String>,
        /// Maximum size of the Cluster Volume in bytes
        #[arg(long = "required-bytes")]
        required_bytes: Option<String>,
        /// Cluster Volume access scope (single, multi)
        #[arg(long = "scope")]
        scope: Option<String>,
        /// Cluster Volume secrets
        #[arg(long = "secret")]
        secret: Vec<String>,
        /// Cluster Volume access sharing
        #[arg(long = "sharing")]
        sharing: Option<String>,
        /// Topology that the Cluster Volume would be preferred in
        #[arg(long = "topology-preferred")]
        topology_preferred: Vec<String>,
        /// Topology that the Cluster Volume must be accessible from
        #[arg(long = "topology-required")]
        topology_required: Vec<String>,
        /// Cluster Volume access type (mount, block)
        #[arg(long = "type")]
        vol_type: Option<String>,
    },
    Ls,
    Inspect {
        name: String,
    },
    Rm {
        name: String,
    },
    Prune {
        #[arg(short = 'f', long = "force")]
        force: bool,
    },
}

#[derive(Args, Debug)]
pub struct NetworkSubcommands {
    #[command(subcommand)]
    pub command: NetworkAction,
}

#[derive(Subcommand, Debug)]
pub enum NetworkAction {
    Create {
        name: String,
        /// Driver to manage the Network (default "bridge")
        #[arg(short = 'd', long = "driver", default_value = "bridge")]
        driver: String,
        /// Subnet in CIDR format
        #[arg(long = "subnet")]
        subnet: Option<String>,
        /// IPv4 or IPv6 Gateway for the master subnet
        #[arg(long = "gateway")]
        gateway: Option<String>,
        /// Restrict external access to the network
        #[arg(long = "internal")]
        internal: bool,
        /// Enable manual container attachment
        #[arg(long = "attachable")]
        attachable: bool,
        /// Set metadata on a network
        #[arg(long = "label")]
        labels: Vec<String>,
        /// Auxiliary IPv4 or IPv6 addresses
        #[arg(long = "aux-address")]
        aux_address: Vec<String>,
        /// The network from which to copy the configuration
        #[arg(long = "config-from")]
        config_from: Option<String>,
        /// Create a configuration only network
        #[arg(long = "config-only")]
        config_only: bool,
        /// Create swarm routing-mesh network
        #[arg(long = "ingress")]
        ingress: bool,
        /// Allocate container ip from a sub-range
        #[arg(long = "ip-range")]
        ip_range: Option<String>,
        /// IP Address Management Driver
        #[arg(long = "ipam-driver")]
        ipam_driver: Option<String>,
        /// Set IPAM driver specific options
        #[arg(long = "ipam-opt")]
        ipam_opt: Vec<String>,
        /// Enable or disable IPv4 address assignment
        #[arg(long = "ipv4")]
        ipv4: bool,
        /// Enable or disable IPv6 address assignment
        #[arg(long = "ipv6")]
        ipv6: bool,
        /// Set driver specific options
        #[arg(short = 'o', long = "opt")]
        opts: Vec<String>,
        /// Control the network's scope
        #[arg(long = "scope")]
        scope: Option<String>,
    },
    Ls,
    Inspect {
        name: String,
    },
    Rm {
        name: String,
    },
    Prune {
        #[arg(short = 'f', long = "force")]
        force: bool,
    },
    Connect {
        network: String,
        container: String,
    },
    Disconnect {
        network: String,
        container: String,
    },
}

#[derive(Args, Debug)]
pub struct DaemonArgs {
    #[arg(short = 's', long = "socket")]
    pub socket: Option<String>,
}

#[derive(Args, Debug, Clone)]
pub struct PsArgs {
    /// Show all containers (default shows just running)
    #[arg(short = 'a', long = "all")]
    pub all: bool,

    /// Only display numeric IDs
    #[arg(short = 'q', long = "quiet")]
    pub quiet: bool,

    /// Display total file sizes
    #[arg(short = 's', long = "size")]
    pub size: bool,

    /// Don't truncate output
    #[arg(long = "no-trunc")]
    pub no_trunc: bool,

    /// Format output using a custom template (e.g. json, table)
    #[arg(long = "format")]
    pub format: Option<String>,

    /// Show n last created containers (includes all states)
    #[arg(short = 'n', long = "last")]
    pub last: Option<usize>,

    /// Show the latest created container (includes all states)
    #[arg(short = 'l', long = "latest")]
    pub latest: bool,

    /// Filter output based on conditions provided
    #[arg(short = 'f', long = "filter")]
    pub filter: Vec<String>,
}

#[derive(Args, Debug, Clone, Default)]
pub struct ImagesArgs {
    /// Only show numeric IDs
    #[arg(short = 'q', long = "quiet")]
    pub quiet: bool,

    /// Show all images (default hides intermediate images)
    #[arg(short = 'a', long = "all")]
    pub all: bool,

    /// Show digests
    #[arg(long = "digests")]
    pub digests: bool,

    /// Don't truncate output
    #[arg(long = "no-trunc")]
    pub no_trunc: bool,

    /// List multi-platform images as a tree (EXPERIMENTAL)
    #[arg(long = "tree")]
    pub tree: bool,

    /// Format output using a custom template (e.g. json, table)
    #[arg(long = "format")]
    pub format: Option<String>,

    /// Filter output based on conditions provided
    #[arg(short = 'f', long = "filter")]
    pub filter: Vec<String>,
}

#[derive(Args, Debug)]
pub struct RmArgs {
    /// Force the removal of a running container
    #[arg(short = 'f', long = "force")]
    pub force: bool,

    /// Remove anonymous volumes associated with the container
    #[arg(short = 'v', long = "volumes")]
    pub volumes: bool,

    /// Remove the specified link
    #[arg(short = 'l', long = "link")]
    pub link: bool,

    /// Container IDs or names to remove
    #[arg(required = true)]
    pub containers: Vec<String>,
}

#[derive(Args, Debug)]
pub struct RmiArgs {
    /// Force removal of the image
    #[arg(short = 'f', long = "force")]
    pub force: bool,

    /// Do not delete untagged parents
    #[arg(long = "no-prune")]
    pub no_prune: bool,

    /// Remove only the given platform variant
    #[arg(long = "platform")]
    pub platform: Option<String>,

    /// Image references or IDs to remove
    #[arg(required = true)]
    pub images: Vec<String>,
}

#[derive(Args, Debug)]
pub struct SpecArgs {
    /// Path to bundle directory where config.json should be created (default: current directory)
    #[arg(short = 'b', long = "bundle")]
    pub bundle: Option<String>,
}

#[derive(Args, Debug)]
pub struct StatsArgs {
    /// Disable streaming stats and only pull the first result
    #[arg(long = "no-stream")]
    pub no_stream: bool,

    /// Target container IDs or names
    pub containers: Vec<String>,
}

#[derive(Args, Debug)]
pub struct EventsArgs {
    /// Show all events created since timestamp
    #[arg(long = "since")]
    pub since: Option<String>,

    /// Filter output based on conditions provided
    #[arg(short = 'f', long = "filter")]
    pub filter: Option<String>,
}

#[derive(Args, Debug)]
pub struct CompletionArgs {
    /// Target shell (bash, zsh, fish)
    #[arg(default_value = "zsh")]
    pub shell: String,

    /// Install completion script automatically into user shell directory
    #[arg(long = "install")]
    pub install: bool,
}

#[derive(Args, Debug)]
pub struct AliasArgs {
    /// Install wrapper script in ~/.boxr/bin/docker
    #[arg(long = "install")]
    pub install: bool,

    /// Output shell alias command
    #[arg(long = "eval")]
    pub eval: bool,
}

#[derive(Args, Debug)]
pub struct DiffArgs {
    /// Container to inspect changes on
    pub container: String,
}

#[derive(Args, Debug)]
pub struct ContextSubcommands {
    #[command(subcommand)]
    pub command: ContextAction,
}

#[derive(Subcommand, Debug)]
pub enum ContextAction {
    /// List contexts
    #[command(alias = "list")]
    Ls,
    /// Print current context
    Show,
    /// Set the current docker context
    Use { name: String },
    /// Display detailed information on one or more contexts
    Inspect { name: Option<String> },
    /// Create a context
    Create {
        name: String,
        #[arg(long = "description")]
        description: Option<String>,
        #[arg(long = "docker")]
        docker: Option<String>,
        /// Create context from a named context
        #[arg(long = "from")]
        from: Option<String>,
    },
    /// Remove one or more contexts
    Rm { name: String },
}

#[derive(Args, Debug)]
pub struct ManifestSubcommands {
    #[command(subcommand)]
    pub command: ManifestAction,
}

#[derive(Subcommand, Debug)]
pub enum ManifestAction {
    /// Display an image manifest, or manifest list
    Inspect {
        image: String,
        #[arg(long = "insecure")]
        insecure: bool,
    },
    /// Create a local manifest list for annotating and pushing to a registry
    Create {
        target: String,
        #[arg(required = true)]
        sources: Vec<String>,
    },
    /// Push a manifest list to a repository
    Push {
        target: String,
        #[arg(long = "insecure")]
        insecure: bool,
        #[arg(long = "purge")]
        purge: bool,
    },
}

#[derive(Args, Debug)]
pub struct TopArgs {
    /// Container to inspect processes on
    pub container: String,

    /// Optional ps arguments
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub ps_args: Vec<String>,
}

#[derive(Args, Debug)]
pub struct CommitArgs {
    /// Commit message
    #[arg(short = 'm', long = "message")]
    pub message: Option<String>,

    /// Author (e.g., "John Doe <john@example.com>")
    #[arg(short = 'a', long = "author")]
    pub author: Option<String>,

    /// Pause container during commit
    #[arg(short = 'p', long = "pause", default_value_t = true)]
    pub pause: bool,

    /// Container to commit
    pub container: String,

    /// Repository and optional tag (e.g., "my-image:v1")
    pub repo_tag: Option<String>,
}

#[derive(Args, Debug)]
pub struct PauseArgs {
    /// Container to pause
    pub container: String,
}

#[derive(Args, Debug)]
pub struct UnpauseArgs {
    /// Container to unpause
    pub container: String,
}

#[derive(Args, Debug)]
pub struct RenameArgs {
    /// Container to rename
    pub container: String,

    /// New name for the container
    pub new_name: String,
}

#[derive(Args, Debug)]
pub struct WaitArgs {
    /// Container to wait on
    pub container: String,
}

#[derive(Args, Debug)]
pub struct CpArgs {
    /// Source path (e.g. "my-container:/app/file" or "./local-file")
    pub src: String,

    /// Destination path (e.g. "./dest" or "my-container:/app/dest")
    pub dest: String,
}

#[derive(Args, Debug)]
pub struct UpdateArgs {
    /// Memory limit
    #[arg(short = 'm', long = "memory")]
    pub memory: Option<String>,

    /// Memory soft limit
    #[arg(long = "memory-reservation")]
    pub memory_reservation: Option<String>,

    /// Swap limit equal to memory plus swap
    #[arg(long = "memory-swap")]
    pub memory_swap: Option<String>,

    /// CPU limit
    #[arg(long = "cpus")]
    pub cpus: Option<String>,

    /// CPU shares (relative weight)
    #[arg(short = 'c', long = "cpu-shares")]
    pub cpu_shares: Option<u64>,

    /// Limit CPU CFS (Completely Fair Scheduler) period
    #[arg(long = "cpu-period")]
    pub cpu_period: Option<u64>,

    /// Limit CPU CFS (Completely Fair Scheduler) quota
    #[arg(long = "cpu-quota")]
    pub cpu_quota: Option<i64>,

    /// CPUs in which to allow execution (0-3, 0,1)
    #[arg(long = "cpuset-cpus")]
    pub cpuset_cpus: Option<String>,

    /// MEMs in which to allow execution (0-3, 0,1)
    #[arg(long = "cpuset-mems")]
    pub cpuset_mems: Option<String>,

    /// Block IO (relative weight), between 10 and 1000, or 0 to disable
    #[arg(long = "blkio-weight")]
    pub blkio_weight: Option<u16>,

    /// Limit the CPU real-time period in microseconds
    #[arg(long = "cpu-rt-period")]
    pub cpu_rt_period: Option<i64>,

    /// Limit the CPU real-time runtime in microseconds
    #[arg(long = "cpu-rt-runtime")]
    pub cpu_rt_runtime: Option<i64>,

    /// Maximum number of PIDs
    #[arg(long = "pids-limit")]
    pub pids_limit: Option<i64>,

    /// Restart policy to apply when a container exits
    #[arg(long = "restart")]
    pub restart: Option<String>,

    /// Container to update
    pub container: String,
}

#[derive(Args, Debug)]
pub struct AttachArgs {
    /// Do not attach STDIN
    #[arg(long = "no-stdin")]
    pub no_stdin: bool,

    /// Container to attach to
    pub container: String,
}

#[derive(Args, Debug)]
pub struct KillArgs {
    /// Signal to send (e.g. SIGHUP, SIGTERM, SIGKILL)
    #[arg(short = 's', long = "signal")]
    pub signal: Option<String>,

    /// Container to kill
    pub container: String,
}

#[derive(Args, Debug)]
pub struct SystemSubcommands {
    #[command(subcommand)]
    pub command: SystemAction,
}

#[derive(Subcommand, Debug)]
pub enum SystemAction {
    /// Show boxr disk usage
    Df,
    /// Remove unused data
    Prune {
        #[arg(short = 'a', long = "all")]
        all: bool,
        #[arg(short = 'f', long = "force")]
        force: bool,
        #[arg(long = "volumes")]
        volumes: bool,
    },
}

#[derive(Args, Debug)]
pub struct RestartArgs {
    /// Seconds to wait before killing the container
    #[arg(short = 't', long = "time", default_value_t = 10)]
    pub time: u32,

    /// Container to restart
    pub container: String,
}

#[derive(Args, Debug)]
pub struct PortArgs {
    /// Container to inspect ports on
    pub container: String,

    /// Optional private port / protocol (e.g. 80/tcp)
    pub port: Option<String>,
}

#[derive(Args, Debug)]
pub struct TagArgs {
    /// Source image
    pub source: String,

    /// Target image and tag
    pub target: String,
}

#[derive(Args, Debug)]
pub struct ExportArgs {
    /// Write to a file, instead of STDOUT
    #[arg(short = 'o', long = "output")]
    pub output: Option<String>,

    /// Container to export
    pub container: String,
}

#[derive(Args, Debug)]
pub struct ImportArgs {
    /// The URL or - to read from STDIN
    pub file: String,

    /// Repository and optional tag to apply
    pub reference: Option<String>,
}

#[derive(Args, Debug)]
pub struct HistoryArgs {
    /// Image to show history for
    pub image: String,
}

#[derive(Args, Debug)]
pub struct SearchArgs {
    /// Search term
    pub term: String,
}

#[derive(Args, Debug)]
pub struct PodSubcommands {
    #[command(subcommand)]
    pub command: PodAction,
}

#[derive(Subcommand, Debug)]
pub enum PodAction {
    /// Create a pod
    Create {
        /// Pod name
        #[arg(long = "name")]
        name: Option<String>,
        /// Publish port (e.g. 8080:80)
        #[arg(short = 'p', long = "publish")]
        ports: Vec<String>,
    },
    /// List pods
    Ps,
    /// List pods
    Ls,
    /// Remove a pod
    Rm { pod: String },
    /// Inspect a pod
    Inspect { pod: String },
    /// Stop a pod
    Stop { pod: String },
    /// Start a pod
    Start { pod: String },
}

#[derive(Args, Debug)]
pub struct PlaySubcommands {
    #[command(subcommand)]
    pub command: PlayAction,
}

#[derive(Subcommand, Debug)]
pub enum PlayAction {
    /// Play a pod from a Kubernetes YAML file
    Kube {
        /// Path to Kubernetes YAML file
        file: String,
    },
}

#[derive(Args, Debug)]
pub struct GenerateSubcommands {
    #[command(subcommand)]
    pub command: GenerateAction,
}

#[derive(Subcommand, Debug)]
pub enum GenerateAction {
    /// Generate Kubernetes Pod YAML for a container or pod
    Kube {
        /// Container or pod to generate YAML for
        target: String,
    },
}

#[derive(Args, Debug)]
pub struct UnshareArgs {
    /// Command to run inside new user namespace
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub command: Vec<String>,
}
