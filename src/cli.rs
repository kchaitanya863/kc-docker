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

    /// Remove one or more containers
    Rm(RmArgs),

    /// Remove one or more images
    Rmi(RmiArgs),

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
    #[arg(short = 'f', long = "follow")]
    pub follow: bool,

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

    #[arg(default_value = ".")]
    pub path: String,
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
