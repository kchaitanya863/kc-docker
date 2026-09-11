#![allow(dead_code)]

use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Default)]
pub struct ResourceLimits {
    pub memory_max_bytes: Option<i64>,
    pub memory_swap_max_bytes: Option<i64>,
    pub cpu_quota_us: Option<i64>,
    pub cpu_period_us: Option<u64>,
    pub cpu_shares: Option<u64>,
    pub pids_max: Option<i64>,
}

impl ResourceLimits {
    pub fn parse_memory(input: &str) -> Result<i64> {
        let input = input.trim().to_lowercase();
        let (num_str, multiplier) = if let Some(n) = input.strip_suffix("g") {
            (n, 1024 * 1024 * 1024)
        } else if let Some(n) = input.strip_suffix("gb") {
            (n, 1024 * 1024 * 1024)
        } else if let Some(n) = input.strip_suffix("m") {
            (n, 1024 * 1024)
        } else if let Some(n) = input.strip_suffix("mb") {
            (n, 1024 * 1024)
        } else if let Some(n) = input.strip_suffix("k") {
            (n, 1024)
        } else if let Some(n) = input.strip_suffix("kb") {
            (n, 1024)
        } else if let Some(n) = input.strip_suffix("b") {
            (n, 1)
        } else {
            (input.as_str(), 1)
        };

        let val: i64 = num_str.parse().context("Invalid memory limit string")?;
        Ok(val * multiplier)
    }

    pub fn parse_cpus(input: &str) -> Result<(i64, u64)> {
        let cpus: f64 = input.parse().context("Invalid cpus limit string")?;
        let period: u64 = 100_000; // 100ms default period
        let quota = (cpus * period as f64) as i64;
        Ok((quota, period))
    }
}

pub struct CgroupV2Manager {
    cgroup_path: PathBuf,
}

impl CgroupV2Manager {
    /// Locate or initialize the rootless / system cgroups v2 hierarchy for a container
    pub fn new(container_id: &str) -> Result<Self> {
        let base_cgroup = Self::detect_cgroup_root();
        let container_cgroup = base_cgroup.join("boxr").join(container_id);

        if base_cgroup.exists() {
            let _ = fs::create_dir_all(&container_cgroup);
        }

        Ok(Self {
            cgroup_path: container_cgroup,
        })
    }

    fn detect_cgroup_root() -> PathBuf {
        #[cfg(target_os = "linux")]
        {
            // Check user slice first (for rootless execution)
            let uid = unsafe { libc::getuid() };
            if uid != 0 {
                let user_slice = PathBuf::from(format!(
                    "/sys/fs/cgroup/user.slice/user-{}.slice/user@{}.service",
                    uid, uid
                ));
                if user_slice.exists() {
                    return user_slice;
                }
            }

            let sys_cgroup = PathBuf::from("/sys/fs/cgroup");
            if sys_cgroup.exists() {
                return sys_cgroup;
            }
        }

        // Mock/Fallback location for testing or platforms without cgroups
        PathBuf::from("/tmp/boxr-cgroup")
    }

    /// Apply resource constraints to the container cgroup
    pub fn apply_limits(&self, limits: &ResourceLimits) -> Result<()> {
        if !self.cgroup_path.exists() {
            return Ok(());
        }

        // Apply memory limits
        if let Some(mem) = limits.memory_max_bytes {
            let _ = fs::write(self.cgroup_path.join("memory.max"), mem.to_string());
        }

        // Apply CPU quota
        if let (Some(quota), Some(period)) = (limits.cpu_quota_us, limits.cpu_period_us) {
            let val = format!("{} {}", quota, period);
            let _ = fs::write(self.cgroup_path.join("cpu.max"), val);
        }

        // Apply PID limits
        if let Some(pids) = limits.pids_max {
            let val = if pids > 0 {
                pids.to_string()
            } else {
                "max".to_string()
            };
            let _ = fs::write(self.cgroup_path.join("pids.max"), val);
        }

        Ok(())
    }

    /// Attach a process ID to this cgroup
    pub fn add_process(&self, pid: i32) -> Result<()> {
        if !self.cgroup_path.exists() {
            return Ok(());
        }
        fs::write(self.cgroup_path.join("cgroup.procs"), pid.to_string())
            .context("Failed to attach pid to cgroup.procs")?;
        Ok(())
    }

    /// Freeze all processes in the cgroup (pause)
    pub fn freeze(&self) -> Result<()> {
        if self.cgroup_path.exists() {
            let freeze_file = self.cgroup_path.join("cgroup.freeze");
            if freeze_file.exists() {
                fs::write(freeze_file, "1")?;
            }
        }
        Ok(())
    }

    /// Thaw all processes in the cgroup (unpause)
    pub fn unfreeze(&self) -> Result<()> {
        if self.cgroup_path.exists() {
            let freeze_file = self.cgroup_path.join("cgroup.freeze");
            if freeze_file.exists() {
                fs::write(freeze_file, "0")?;
            }
        }
        Ok(())
    }

    /// Clean up the container cgroup hierarchy
    pub fn cleanup(&self) -> Result<()> {
        if self.cgroup_path.exists() {
            // In cgroups v2, cgroup.kill terminates all remaining procs
            let kill_file = self.cgroup_path.join("cgroup.kill");
            if kill_file.exists() {
                let _ = fs::write(kill_file, "1");
            }
            let _ = fs::remove_dir(&self.cgroup_path);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_memory_units() {
        assert_eq!(
            ResourceLimits::parse_memory("512m").unwrap(),
            512 * 1024 * 1024
        );
        assert_eq!(
            ResourceLimits::parse_memory("2g").unwrap(),
            2 * 1024 * 1024 * 1024
        );
        assert_eq!(ResourceLimits::parse_memory("1024k").unwrap(), 1024 * 1024);
        assert_eq!(ResourceLimits::parse_memory("1000").unwrap(), 1000);
    }

    #[test]
    fn test_parse_cpus() {
        let (quota, period) = ResourceLimits::parse_cpus("1.5").unwrap();
        assert_eq!(period, 100_000);
        assert_eq!(quota, 150_000);

        let (quota, period) = ResourceLimits::parse_cpus("0.5").unwrap();
        assert_eq!(period, 100_000);
        assert_eq!(quota, 50_000);
    }
}
