use clap::{Args, Subcommand};

#[derive(Args, Debug, Clone)]
pub struct PullArgs {
    /// Set platform if server is multi-platform (e.g. linux/amd64, linux/arm64)
    #[arg(long = "platform")]
    pub platform: Option<String>,

    pub image: String,
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

    /// Do not cache specified stages
    #[arg(long = "no-cache-filter")]
    pub no_cache_filter: Vec<String>,

    /// Set build-time CPU shares
    #[arg(short = 'c', long = "cpu-shares")]
    pub cpu_shares: Option<u64>,

    /// CPUs in which to allow execution (0-3, 0,1)
    #[arg(long = "cpuset-cpus")]
    pub cpuset_cpus: Option<String>,

    /// MEMs in which to allow execution (0-3, 0,1)
    #[arg(long = "cpuset-mems")]
    pub cpuset_mems: Option<String>,

    /// Limit CPU CFS period
    #[arg(long = "cpu-period")]
    pub cpu_period: Option<u64>,

    /// Limit CPU CFS quota
    #[arg(long = "cpu-quota")]
    pub cpu_quota: Option<i64>,

    /// Set platform if server is multi-platform
    #[arg(long = "platform")]
    pub platform: Option<String>,

    /// Set isolation technology
    #[arg(long = "isolation")]
    pub isolation: Option<String>,

    /// Label to set on the image
    #[arg(long = "label")]
    pub labels: Vec<String>,

    /// Suppress the build output and print image ID on success
    #[arg(short = 'q', long = "quiet")]
    pub quiet: bool,

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
    Inspect(super::container::InspectArgs),
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
pub struct TagArgs {
    /// Source image
    pub source: String,

    /// Target image and tag
    pub target: String,
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
    /// Max number of search results
    #[arg(long = "limit", default_value_t = 25)]
    pub limit: usize,

    /// Search term
    pub term: String,
}
