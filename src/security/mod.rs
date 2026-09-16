#![allow(dead_code, unused_imports)]

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdMapping {
    pub container_id: u32,
    pub host_id: u32,
    pub size: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootlessUserConfig {
    pub enabled: bool,
    pub uid_mappings: Vec<IdMapping>,
    pub gid_mappings: Vec<IdMapping>,
}

impl Default for RootlessUserConfig {
    fn default() -> Self {
        #[cfg(unix)]
        let (uid, gid) = {
            let u = unsafe { libc::getuid() };
            let g = unsafe { libc::getgid() };
            (u, g)
        };
        #[cfg(not(unix))]
        let (uid, gid) = (1000, 1000);

        Self {
            enabled: uid != 0,
            uid_mappings: vec![IdMapping {
                container_id: 0,
                host_id: uid,
                size: 1,
            }],
            gid_mappings: vec![IdMapping {
                container_id: 0,
                host_id: gid,
                size: 1,
            }],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubordinateRange {
    pub start: u32,
    pub count: u32,
}

impl RootlessUserConfig {
    /// Parse /etc/subuid or /etc/subgid file content for a matching username or UID
    pub fn parse_subid_content(
        content: &str,
        user_id: u32,
        username: Option<&str>,
    ) -> Option<SubordinateRange> {
        let uid_str = user_id.to_string();
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 3 {
                let name_or_id = parts[0].trim();
                let matches_name = username.map(|u| u == name_or_id).unwrap_or(false);
                let matches_id = name_or_id == uid_str;
                if matches_name || matches_id {
                    if let (Ok(start), Ok(count)) = (
                        parts[1].trim().parse::<u32>(),
                        parts[2].trim().parse::<u32>(),
                    ) {
                        return Some(SubordinateRange { start, count });
                    }
                }
            }
        }
        None
    }

    /// Read subordinate range from /etc/subuid or /etc/subgid on Linux
    pub fn read_subordinate_range(is_gid: bool) -> Option<SubordinateRange> {
        #[cfg(unix)]
        let (id, user_name) = {
            let u = if is_gid {
                unsafe { libc::getgid() }
            } else {
                unsafe { libc::getuid() }
            };
            let name = std::env::var("USER").ok();
            (u, name)
        };
        #[cfg(not(unix))]
        let (id, user_name) = (1000, None);

        let file_path = if is_gid { "/etc/subgid" } else { "/etc/subuid" };
        if let Ok(content) = fs::read_to_string(file_path) {
            Self::parse_subid_content(&content, id, user_name.as_deref())
        } else {
            None
        }
    }

    /// Check if newuidmap and newgidmap helper binaries are installed on the system
    pub fn has_newidmap_binaries() -> bool {
        let has_uidmap = Path::new("/usr/bin/newuidmap").exists()
            || Path::new("/bin/newuidmap").exists()
            || std::process::Command::new("which")
                .arg("newuidmap")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
        let has_gidmap = Path::new("/usr/bin/newgidmap").exists()
            || Path::new("/bin/newgidmap").exists()
            || std::process::Command::new("which")
                .arg("newgidmap")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
        has_uidmap && has_gidmap
    }

    /// Configure UID and GID mapping for a target child PID in a new user namespace
    #[cfg(target_os = "linux")]
    pub fn setup_child_mappings(pid: i32) -> Result<()> {
        let (host_uid, host_gid) = unsafe { (libc::getuid(), libc::getgid()) };

        let sub_uid = Self::read_subordinate_range(false);
        let sub_gid = Self::read_subordinate_range(true);

        // Attempt newuidmap/newgidmap if both binaries and subuid/subgid ranges are available
        if Self::has_newidmap_binaries() && sub_uid.is_some() && sub_gid.is_some() {
            let u_range = sub_uid.unwrap();
            let g_range = sub_gid.unwrap();

            // newuidmap <pid> 0 <host_uid> 1 1 <sub_uid_start> <sub_uid_count>
            let uid_status = std::process::Command::new("newuidmap")
                .args([
                    &pid.to_string(),
                    "0",
                    &host_uid.to_string(),
                    "1",
                    "1",
                    &u_range.start.to_string(),
                    &u_range.count.to_string(),
                ])
                .status();

            // newgidmap <pid> 0 <host_gid> 1 1 <sub_gid_start> <sub_gid_count>
            let gid_status = std::process::Command::new("newgidmap")
                .args([
                    &pid.to_string(),
                    "0",
                    &host_gid.to_string(),
                    "1",
                    "1",
                    &g_range.start.to_string(),
                    &g_range.count.to_string(),
                ])
                .status();

            if let (Ok(u_st), Ok(g_st)) = (uid_status, gid_status) {
                if u_st.success() && g_st.success() {
                    return Ok(());
                }
            }
        }

        // Fallback: write single UID/GID map directly to /proc/<pid>/uid_map and /proc/<pid>/gid_map
        let cfg = RootlessUserConfig {
            enabled: true,
            uid_mappings: vec![IdMapping {
                container_id: 0,
                host_id: host_uid,
                size: 1,
            }],
            gid_mappings: vec![IdMapping {
                container_id: 0,
                host_id: host_gid,
                size: 1,
            }],
        };
        cfg.write_proc_mappings(pid)?;
        Ok(())
    }

    /// Configure UID and GID mapping for a target PID in Linux /proc/<pid>
    #[cfg(target_os = "linux")]
    pub fn write_proc_mappings(&self, pid: i32) -> Result<()> {
        let pid_str = pid.to_string();
        let proc_dir = Path::new("/proc").join(&pid_str);

        // In Linux unprivileged user namespaces, writing to gid_map requires first disabling setgroups
        let setgroups_path = proc_dir.join("setgroups");
        if setgroups_path.exists() {
            let _ = fs::write(&setgroups_path, "deny");
        }

        // Write uid_map: "<container_uid> <host_uid> <count>\n"
        let mut uid_content = String::new();
        for m in &self.uid_mappings {
            uid_content.push_str(&format!("{} {} {}\n", m.container_id, m.host_id, m.size));
        }
        fs::write(proc_dir.join("uid_map"), uid_content)
            .context("Failed to write uid_map in /proc")?;

        // Write gid_map: "<container_gid> <host_gid> <count>\n"
        let mut gid_content = String::new();
        for m in &self.gid_mappings {
            gid_content.push_str(&format!("{} {} {}\n", m.container_id, m.host_id, m.size));
        }
        fs::write(proc_dir.join("gid_map"), gid_content)
            .context("Failed to write gid_map in /proc")?;

        Ok(())
    }
}

/// Standard safe Linux Capabilities conforming to Docker/Podman default capability whitelist.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityProfile {
    pub bounding: Vec<String>,
    pub effective: Vec<String>,
    pub permitted: Vec<String>,
}

impl Default for CapabilityProfile {
    fn default() -> Self {
        let caps = vec![
            "CAP_CHOWN".to_string(),
            "CAP_DAC_OVERRIDE".to_string(),
            "CAP_FOWNER".to_string(),
            "CAP_FSETID".to_string(),
            "CAP_KILL".to_string(),
            "CAP_SETGID".to_string(),
            "CAP_SETUID".to_string(),
            "CAP_SETPCAP".to_string(),
            "CAP_NET_BIND_SERVICE".to_string(),
            "CAP_NET_RAW".to_string(),
            "CAP_SYS_CHROOT".to_string(),
            "CAP_MKNOD".to_string(),
            "CAP_AUDIT_WRITE".to_string(),
            "CAP_SETFCAP".to_string(),
        ];
        Self {
            bounding: caps.clone(),
            effective: caps.clone(),
            permitted: caps,
        }
    }
}

/// Default OCI Seccomp specification blocking dangerous system calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeccompRule {
    pub default_action: String,
    pub architectures: Vec<String>,
    pub syscalls: Vec<SeccompSyscall>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeccompSyscall {
    pub names: Vec<String>,
    pub action: String,
}

impl SeccompRule {
    pub fn default_filter() -> Self {
        Self {
            default_action: "SCMP_ACT_ALLOW".to_string(),
            architectures: vec![
                "SCMP_ARCH_X86_64".to_string(),
                "SCMP_ARCH_AARCH64".to_string(),
            ],
            syscalls: vec![SeccompSyscall {
                names: vec![
                    "acct".to_string(),
                    "add_key".to_string(),
                    "bpf".to_string(),
                    "clock_settime".to_string(),
                    "init_module".to_string(),
                    "finit_module".to_string(),
                    "delete_module".to_string(),
                    "kexec_load".to_string(),
                    "kexec_file_load".to_string(),
                    "keyctl".to_string(),
                    "lookup_dcookie".to_string(),
                    "perf_event_open".to_string(),
                    "pivot_root".to_string(),
                    "ptrace".to_string(),
                    "reboot".to_string(),
                    "request_key".to_string(),
                    "set_mempolicy".to_string(),
                    "settimeofday".to_string(),
                    "stime".to_string(),
                    "swapoff".to_string(),
                    "swapon".to_string(),
                    "sysfs".to_string(),
                    "sys_settimeofday".to_string(),
                    "umount2".to_string(),
                    "unshare".to_string(),
                    "userfaultfd".to_string(),
                    "vmsplice".to_string(),
                ],
                action: "SCMP_ACT_ERRNO".to_string(),
            }],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rootless_user_config_defaults() {
        let config = RootlessUserConfig::default();
        assert!(!config.uid_mappings.is_empty());
        assert!(!config.gid_mappings.is_empty());
        assert_eq!(config.uid_mappings[0].container_id, 0);
        assert_eq!(config.gid_mappings[0].container_id, 0);
    }

    #[test]
    fn test_parse_subid_content() {
        let subuid_content = "\
# /etc/subuid comment
root:100000:65536
vagrant:165536:65536
1000:231072:65536
";
        // Lookup by username
        let range1 = RootlessUserConfig::parse_subid_content(subuid_content, 1001, Some("vagrant"));
        assert_eq!(
            range1,
            Some(SubordinateRange {
                start: 165536,
                count: 65536
            })
        );

        // Lookup by uid
        let range2 = RootlessUserConfig::parse_subid_content(subuid_content, 1000, None);
        assert_eq!(
            range2,
            Some(SubordinateRange {
                start: 231072,
                count: 65536
            })
        );

        // Non-existent user
        let range3 = RootlessUserConfig::parse_subid_content(subuid_content, 9999, Some("nonexistent"));
        assert_eq!(range3, None);
    }

    #[test]
    fn test_seccomp_default_rules() {
        let seccomp = SeccompRule::default_filter();
        assert_eq!(seccomp.default_action, "SCMP_ACT_ALLOW");
        assert!(!seccomp.syscalls.is_empty());
        let blocked = &seccomp.syscalls[0];
        assert_eq!(blocked.action, "SCMP_ACT_ERRNO");
        assert!(blocked.names.contains(&"reboot".to_string()));
        assert!(blocked.names.contains(&"swapon".to_string()));
    }
}
