use clap::{Args, Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "boxr",
    author = "Boxr Contributors",
    version,
    about = "A fast, lightweight OCI container engine and runtime written in Rust",
    long_about = "boxr is an Open Container Initiative (OCI) compliant container engine and runtime written in Rust.\nIt pulls image manifests and layers via the OCI Distribution Spec, unpacks root filesystems with whiteout support per the OCI Image Spec, generates OCI Runtime Spec bundles, and executes containers."
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

    /// List local images
    Images,

    /// List containers
    Ps(PsArgs),

    /// Remove one or more containers
    Rm(RmArgs),

    /// Remove one or more images
    Rmi(RmiArgs),

    /// Generate a standard OCI runtime specification (config.json)
    Spec(SpecArgs),
}

#[derive(Args, Debug)]
pub struct PullArgs {
    /// The container image reference (e.g., 'hello-world', 'alpine:3.19', 'ghcr.io/org/repo:tag')
    pub image: String,
}

#[derive(Args, Debug)]
pub struct RunArgs {
    /// Allocate a pseudo-TTY and keep stdin open
    #[arg(short = 'i', long = "interactive")]
    pub interactive: bool,

    /// Automatically remove the container when it exits
    #[arg(long = "rm")]
    pub rm: bool,

    /// Assign a name to the container
    #[arg(long = "name")]
    pub name: Option<String>,

    /// Set environment variables (-e KEY=VALUE)
    #[arg(short = 'e', long = "env")]
    pub env: Vec<String>,

    /// The container image to run
    pub image: String,

    /// Command and arguments to run inside the container (overrides image CMD)
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub command: Vec<String>,
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
