use clap::{Args, Subcommand};

#[derive(Args, Debug)]
pub struct DaemonArgs {
    #[arg(short = 's', long = "socket")]
    pub socket: Option<String>,
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

    #[arg(short = 'a', long = "all")]
    pub all: bool,

    #[arg(long = "format")]
    pub format: Option<String>,

    #[arg(long = "no-trunc")]
    pub no_trunc: bool,

    /// Target container IDs or names
    pub containers: Vec<String>,
}

#[derive(Args, Debug)]
pub struct EventsArgs {
    /// Show all events created since timestamp
    #[arg(long = "since")]
    pub since: Option<String>,

    #[arg(long = "until")]
    pub until: Option<String>,

    #[arg(short = 'f', long = "filter")]
    pub filter: Option<String>,

    #[arg(long = "format")]
    pub format: Option<String>,
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
    /// Import a context
    Import {
        name: String,
        source: String,
    },
    /// Export a context
    Export {
        name: String,
        #[arg(short = 'o', long = "output")]
        output: Option<String>,
    },
    /// Update a context
    Update {
        name: String,
        #[arg(long = "description")]
        description: Option<String>,
        #[arg(long = "docker")]
        docker: Option<String>,
    },
}

#[derive(Args, Debug)]
pub struct SystemSubcommands {
    #[command(subcommand)]
    pub command: SystemAction,
}

#[derive(Args, Debug, Clone, Default)]
pub struct SystemDfArgs {
    #[arg(short = 'f', long = "format")]
    pub format: Option<String>,
    #[arg(short = 'v', long = "verbose")]
    pub verbose: bool,
}

#[derive(Subcommand, Debug)]
pub enum SystemAction {
    /// Show boxr disk usage
    Df(SystemDfArgs),
    /// Display system-wide information
    Info(FormatArgs),
    /// Get real time events from the server
    Events(EventsArgs),
    /// Remove unused data
    Prune {
        #[arg(short = 'a', long = "all")]
        all: bool,
        #[arg(short = 'f', long = "force")]
        force: bool,
        #[arg(long = "volumes")]
        volumes: bool,
        #[arg(long = "filter")]
        filter: Vec<String>,
    },
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

#[derive(Args, Debug, Default, Clone)]
pub struct FormatArgs {
    /// Format output using a custom template: 'json' or Go template
    #[arg(short = 'f', long = "format")]
    pub format: Option<String>,
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
