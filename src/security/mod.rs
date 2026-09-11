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

impl RootlessUserConfig {
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
