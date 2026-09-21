use clap::{Args, Subcommand};

#[derive(Args, Debug)]
pub struct MachineSubcommands {
    #[command(subcommand)]
    pub command: MachineAction,
}

#[derive(Subcommand, Debug)]
pub enum MachineAction {
    /// Initialize a new virtual machine
    Init {
        /// Machine name
        name: Option<String>,
        /// Create and start in one step
        #[arg(long = "now")]
        now: bool,
        /// Run machine in rootful mode
        #[arg(long = "rootful")]
        rootful: bool,
    },
    /// Start a virtual machine
    Start {
        name: Option<String>,
    },
    /// Stop a virtual machine
    Stop {
        name: Option<String>,
    },
    /// List virtual machines
    #[command(alias = "list")]
    Ls,
    /// Remove a virtual machine
    Rm {
        name: Option<String>,
        #[arg(short = 'f', long = "force")]
        force: bool,
    },
    /// SSH into a virtual machine
    Ssh {
        name: Option<String>,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
    /// Display machine host info
    Info {
        name: Option<String>,
    },
    /// Copy files between host and virtual machine
    Cp {
        source: String,
        dest: String,
    },
}
