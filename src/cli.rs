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

    #[arg(required = true)]
    pub containers: Vec<String>,
}

#[derive(Args, Debug)]
pub struct StartArgs {
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
    Create { name: Option<String> },
    Ls,
    Inspect { name: String },
    Rm { name: String },
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
        #[arg(long = "subnet")]
        subnet: Option<String>,
        #[arg(long = "gateway")]
        gateway: Option<String>,
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

    /// Container IDs or names to remove
    #[arg(required = true)]
    pub containers: Vec<String>,
}

#[derive(Args, Debug)]
pub struct RmiArgs {
    /// Force removal of the image
    #[arg(short = 'f', long = "force")]
    pub force: bool,

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

    /// CPU limit
    #[arg(long = "cpus")]
    pub cpus: Option<String>,

    /// Maximum number of PIDs
    #[arg(long = "pids-limit")]
    pub pids_limit: Option<i64>,

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
