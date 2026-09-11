use crate::storage::boxr_home;
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::net::Ipv4Addr;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortMapping {
    pub host_ip: Option<String>,
    pub host_port: u16,
    pub container_port: u16,
    pub protocol: String, // "tcp" or "udp"
}

impl PortMapping {
    /// Parse port string: e.g. "8080:80", "127.0.0.1:3000:3000/tcp"
    pub fn parse(spec: &str) -> Result<Self> {
        let (proto, port_spec) = if let Some((p, pr)) = spec.split_once('/') {
            (pr.to_lowercase(), p)
        } else {
            ("tcp".to_string(), spec)
        };

        let parts: Vec<&str> = port_spec.split(':').collect();
        match parts.len() {
            1 => {
                // container port only (dynamic host port)
                let c_port: u16 = parts[0].parse()?;
                Ok(Self {
                    host_ip: None,
                    host_port: c_port,
                    container_port: c_port,
                    protocol: proto,
                })
            }
            2 => {
                // host_port:container_port
                let h_port: u16 = parts[0].parse()?;
                let c_port: u16 = parts[1].parse()?;
                Ok(Self {
                    host_ip: None,
                    host_port: h_port,
                    container_port: c_port,
                    protocol: proto,
                })
            }
            3 => {
                // host_ip:host_port:container_port
                let ip = parts[0].to_string();
                let h_port: u16 = parts[1].parse()?;
                let c_port: u16 = parts[2].parse()?;
                Ok(Self {
                    host_ip: Some(ip),
                    host_port: h_port,
                    container_port: c_port,
                    protocol: proto,
                })
            }
            _ => Err(anyhow!("Invalid port specification: {}", spec)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkEndpoint {
    pub container_id: String,
    pub container_name: String,
    pub ipv4_address: String,
    pub mac_address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkRecord {
    pub id: String,
    pub name: String,
    pub driver: String,
    pub subnet: String,
    pub gateway: String,
    pub internal: bool,
    pub created_at: DateTime<Utc>,
    pub containers: HashMap<String, NetworkEndpoint>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct NetworkStoreData {
    networks: Vec<NetworkRecord>,
}

pub struct NetworkStore {
    index_file: PathBuf,
}

impl NetworkStore {
    pub const DEFAULT_NETWORK: &'static str = "boxr0";

    pub fn new() -> Self {
        let home = boxr_home();
        let store = Self {
            index_file: home.join("networks.json"),
        };
        store.ensure_default_network();
        store
    }

    fn load(&self) -> NetworkStoreData {
        if let Ok(content) = fs::read_to_string(&self.index_file) {
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            NetworkStoreData::default()
        }
    }

    fn save(&self, data: &NetworkStoreData) -> Result<()> {
        let content = serde_json::to_string_pretty(data)?;
        fs::write(&self.index_file, content)?;
        Ok(())
    }

    fn ensure_default_network(&self) {
        let mut data = self.load();
        if !data.networks.iter().any(|n| n.name == Self::DEFAULT_NETWORK) {
            let default_net = NetworkRecord {
                id: "boxr00000000".to_string(),
                name: Self::DEFAULT_NETWORK.to_string(),
                driver: "bridge".to_string(),
                subnet: "172.28.0.0/16".to_string(),
                gateway: "172.28.0.1".to_string(),
                internal: false,
                created_at: Utc::now(),
                containers: HashMap::new(),
            };
            data.networks.push(default_net);
            let _ = self.save(&data);
        }
    }

    pub fn list(&self) -> Vec<NetworkRecord> {
        self.load().networks
    }

    pub fn find(&self, query: &str) -> Option<NetworkRecord> {
        let data = self.load();
        data.networks.into_iter().find(|n| n.id.starts_with(query) || n.name == query)
    }

    pub fn create(&self, name: &str, subnet: Option<&str>, gateway: Option<&str>) -> Result<NetworkRecord> {
        let mut data = self.load();
        if data.networks.iter().any(|n| n.name == name) {
            return Err(anyhow!("Network '{}' already exists", name));
        }

        let network_count = data.networks.len();
        let default_subnet = format!("172.{}.0.0/16", 29 + network_count);
        let default_gw = format!("172.{}.0.1", 29 + network_count);

        let chosen_subnet = subnet.unwrap_or(&default_subnet).to_string();
        let chosen_gw = gateway.unwrap_or(&default_gw).to_string();

        let id = hex::encode(crate::storage::container_store::rand_id());

        let record = NetworkRecord {
            id,
            name: name.to_string(),
            driver: "bridge".to_string(),
            subnet: chosen_subnet,
            gateway: chosen_gw,
            internal: false,
            created_at: Utc::now(),
            containers: HashMap::new(),
        };

        data.networks.push(record.clone());
        self.save(&data)?;
        Ok(record)
    }

    pub fn remove(&self, query: &str) -> Result<NetworkRecord> {
        let mut data = self.load();
        if query == Self::DEFAULT_NETWORK {
            return Err(anyhow!("Cannot remove the default bridge network"));
        }

        if let Some(pos) = data.networks.iter().position(|n| n.id.starts_with(query) || n.name == query) {
            let removed = data.networks.remove(pos);
            self.save(&data)?;
            Ok(removed)
        } else {
            Err(anyhow!("Network '{}' not found", query))
        }
    }

    /// Allocate next available IP and attach container to network
    pub fn connect_container(&self, network_name: &str, container_id: &str, container_name: &str) -> Result<NetworkEndpoint> {
        let mut data = self.load();
        let net = data.networks.iter_mut().find(|n| n.name == network_name || n.id.starts_with(network_name))
            .ok_or_else(|| anyhow!("Network '{}' not found", network_name))?;

        if let Some(ep) = net.containers.get(container_id) {
            return Ok(ep.clone());
        }

        // Allocate next IP
        let ip = allocate_ip_in_subnet(&net.subnet, &net.gateway, &net.containers)?;
        let mac = format!("02:42:{:02x}:{:02x}:{:02x}:{:02x}",
            ip.octets()[0], ip.octets()[1], ip.octets()[2], ip.octets()[3]);

        let endpoint = NetworkEndpoint {
            container_id: container_id.to_string(),
            container_name: container_name.to_string(),
            ipv4_address: ip.to_string(),
            mac_address: mac,
        };

        net.containers.insert(container_id.to_string(), endpoint.clone());
        self.save(&data)?;
        Ok(endpoint)
    }

    /// Disconnect container from network
    pub fn disconnect_container(&self, network_name: &str, container_id: &str) -> Result<()> {
        let mut data = self.load();
        let net = data.networks.iter_mut().find(|n| n.name == network_name || n.id.starts_with(network_name))
            .ok_or_else(|| anyhow!("Network '{}' not found", network_name))?;

        net.containers.remove(container_id);
        self.save(&data)?;
        Ok(())
    }

    /// Generate an /etc/hosts content for a container, mapping all other containers in this network
    #[allow(dead_code)]
    pub fn generate_hosts_file(&self, network_name: &str, _current_container_id: &str) -> Result<String> {
        let mut lines = vec![
            "127.0.0.1\tlocalhost".to_string(),
            "::1\tlocalhost ip6-localhost ip6-loopback".to_string(),
        ];

        let data = self.load();
        if let Some(net) = data.networks.iter().find(|n| n.name == network_name || n.id.starts_with(network_name)) {
            for (cid, ep) in &net.containers {
                lines.push(format!("{}\t{}\t{}", ep.ipv4_address, ep.container_name, &cid[..12.min(cid.len())]));
            }
        }

        Ok(lines.join("\n") + "\n")
    }
}

fn allocate_ip_in_subnet(subnet_str: &str, gateway_str: &str, existing: &HashMap<String, NetworkEndpoint>) -> Result<Ipv4Addr> {
    let (ip_part, _mask) = subnet_str.split_once('/')
        .ok_or_else(|| anyhow!("Invalid CIDR subnet {}", subnet_str))?;

    let base_ip: Ipv4Addr = ip_part.parse()?;
    let gateway: Ipv4Addr = gateway_str.parse()?;

    let used_ips: Vec<Ipv4Addr> = existing.values()
        .filter_map(|e| e.ipv4_address.parse().ok())
        .collect();

    let octets = base_ip.octets();
    // Scan host range starting at .2 up to .254
    for host in 2..254 {
        let candidate = Ipv4Addr::new(octets[0], octets[1], octets[2], host);
        if candidate != gateway && !used_ips.contains(&candidate) {
            return Ok(candidate);
        }
    }

    Err(anyhow!("No available IP addresses in subnet {}", subnet_str))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_port_mapping_parse() {
        let p1 = PortMapping::parse("8080:80").unwrap();
        assert_eq!(p1.host_port, 8080);
        assert_eq!(p1.container_port, 80);
        assert_eq!(p1.protocol, "tcp");

        let p2 = PortMapping::parse("127.0.0.1:5432:5432/udp").unwrap();
        assert_eq!(p2.host_ip.as_deref(), Some("127.0.0.1"));
        assert_eq!(p2.host_port, 5432);
        assert_eq!(p2.container_port, 5432);
        assert_eq!(p2.protocol, "udp");
    }

    #[test]
    fn test_network_lifecycle_and_ipam() {
        let temp = tempdir().unwrap();
        let store = NetworkStore {
            index_file: temp.path().join("networks.json"),
        };
        store.ensure_default_network();

        // Check default network
        let default_net = store.find("boxr0").unwrap();
        assert_eq!(default_net.name, "boxr0");

        // Create custom network
        let custom = store.create("custom-net", None, None).unwrap();
        assert_eq!(custom.name, "custom-net");

        // Connect container
        let ep = store.connect_container("custom-net", "c123456", "web-server").unwrap();
        assert_eq!(ep.container_name, "web-server");
        assert!(ep.ipv4_address.starts_with("172."));

        // Generate hosts
        let hosts = store.generate_hosts_file("custom-net", "c123456").unwrap();
        assert!(hosts.contains("web-server"));

        // Disconnect
        store.disconnect_container("custom-net", "c123456").unwrap();
        let updated = store.find("custom-net").unwrap();
        assert!(updated.containers.is_empty());

        // Remove
        store.remove("custom-net").unwrap();
        assert!(store.find("custom-net").is_none());
    }
}
