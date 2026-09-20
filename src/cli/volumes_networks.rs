use clap::Args;

#[derive(Args, Debug)]
pub struct VolumeSubcommands {
    #[command(subcommand)]
    pub command: VolumeAction,
}

#[derive(clap::Subcommand, Debug)]
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

#[derive(clap::Subcommand, Debug)]
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
