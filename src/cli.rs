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

    /// Manage volumes
    Volume(VolumeSubcommands),

    /// Manage networks
    Network(NetworkSubcommands),

    /// Run the Boxr daemon background API server
    Daemon(DaemonArgs),

    /// List local images
    Images,

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
}

#[derive(Args, Debug)]
pub struct PullArgs {
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

    #[arg(long = "memory")]
    pub memory: Option<String>,

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
    pub container: String,
}

#[derive(Args, Debug)]
pub struct StartArgs {
    pub container: String,
}

#[derive(Args, Debug)]
pub struct LogsArgs {
    /// Follow log output
    #[arg(short = 'f', long = "follow")]
    pub follow: bool,

    /// Show timestamps
    #[arg(short = 't', long = "timestamps")]
    pub timestamps: bool,

    /// Number of lines to show from the end of the logs
    #[arg(short = 'n', long = "tail")]
    pub tail: Option<usize>,

    pub container: String,
}

#[derive(Args, Debug)]
pub struct ExecArgs {
    #[arg(short = 'i', long = "interactive")]
    pub interactive: bool,

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

#[derive(Args, Debug)]
pub struct BuildArgs {
    #[arg(short = 't', long = "tag")]
    pub tag: Option<String>,

    #[arg(short = 'f', long = "file", default_value = "Dockerfile")]
    pub file: String,

    #[arg(long = "no-cache")]
    pub no_cache: bool,

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
    Prune,
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
    Inspect { name: String },
    Rm { name: String },
    Connect { network: String, container: String },
    Disconnect { network: String, container: String },
}

#[derive(Args, Debug)]
pub struct DaemonArgs {
    #[arg(short = 's', long = "socket")]
    pub socket: Option<String>,
}

#[derive(Args, Debug)]
pub struct PsArgs {
    /// Show all containers (default shows just running)
    #[arg(short = 'a', long = "all")]
    pub all: bool,
}

#[derive(Args, Debug)]
pub struct RmArgs {
    /// Container ID or name to remove
    pub container: String,
}

#[derive(Args, Debug)]
pub struct RmiArgs {
    /// Image reference or ID to remove
    pub image: String,
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
    pub shell: String,
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
    #[arg(long = "memory")]
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
    Rm {
        pod: String,
    },
    /// Inspect a pod
    Inspect {
        pod: String,
    },
    /// Stop a pod
    Stop {
        pod: String,
    },
    /// Start a pod
    Start {
        pod: String,
    },
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
