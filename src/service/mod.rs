//! # Service Management Subsystem ⚙️
//!
//! Manages running the Boxr daemon as a background system service with automatic
//! restart-on-boot and autostart on user login:
//! - **macOS**: `launchd` user agent (`~/Library/LaunchAgents/com.boxr.daemon.plist`).
//! - **Linux**: `systemd` user service (`~/.config/systemd/user/boxr.service`).

use crate::storage::boxr_home;
use anyhow::{Context, Result, anyhow};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// Locate current boxr binary on disk
fn find_boxr_executable() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if exe.exists() {
            return exe;
        }
    }

    let default_home_bin = boxr_home().join("bin").join("boxr");
    if default_home_bin.exists() {
        return default_home_bin;
    }

    let brew_bin = PathBuf::from("/opt/homebrew/bin/boxr");
    if brew_bin.exists() {
        return brew_bin;
    }

    PathBuf::from("boxr")
}

pub struct ServiceManager;

impl ServiceManager {
    #[cfg(target_os = "macos")]
    fn plist_path() -> Result<PathBuf> {
        let home = dirs_home()?;
        Ok(home.join("Library/LaunchAgents/com.boxr.daemon.plist"))
    }

    #[cfg(target_os = "linux")]
    fn service_path() -> Result<PathBuf> {
        let home = dirs_home()?;
        Ok(home.join(".config/systemd/user/boxr.service"))
    }

    /// Install autostart service definition
    pub fn install() -> Result<()> {
        let exe = find_boxr_executable();
        let home = boxr_home();
        fs::create_dir_all(&home)?;

        #[cfg(target_os = "macos")]
        {
            let plist_path = Self::plist_path()?;
            if let Some(parent) = plist_path.parent() {
                fs::create_dir_all(parent)?;
            }

            let log_file = home.join("daemon.log");
            let plist_content = format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.boxr.daemon</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
        <string>daemon</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>{}</string>
    <key>StandardErrorPath</key>
    <string>{}</string>
</dict>
</plist>
"#,
                exe.display(),
                log_file.display(),
                log_file.display()
            );

            fs::write(&plist_path, plist_content)?;
            println!(
                "✓ Installed launchd service agent to {}",
                plist_path.display()
            );
            println!("  Daemon binary: {}", exe.display());
            println!("  Autostart on login: Enabled");
            println!("\nTo start the service now, run:");
            println!("  boxr service start");
        }

        #[cfg(target_os = "linux")]
        {
            let s_path = Self::service_path()?;
            if let Some(parent) = s_path.parent() {
                fs::create_dir_all(parent)?;
            }

            let log_file = home.join("daemon.log");
            let service_content = format!(
                r#"[Unit]
Description=Boxr Container Engine Daemon
Documentation=https://github.com/kchaitanya863/kc-docker
After=network.target

[Service]
Type=simple
ExecStart={} daemon
Restart=always
RestartSec=2s
StandardOutput=append:{}
StandardError=append:{}

[Install]
WantedBy=default.target
"#,
                exe.display(),
                log_file.display(),
                log_file.display()
            );

            fs::write(&s_path, service_content)?;
            let _ = Command::new("systemctl")
                .args(["--user", "daemon-reload"])
                .status();
            let _ = Command::new("systemctl")
                .args(["--user", "enable", "boxr"])
                .status();
            println!(
                "✓ Installed and enabled systemd user service at {}",
                s_path.display()
            );
            println!("\nTo start the service now, run:");
            println!("  boxr service start");
        }

        Ok(())
    }

    /// Start the background service
    pub fn start() -> Result<()> {
        #[cfg(target_os = "macos")]
        {
            let plist_path = Self::plist_path()?;
            if !plist_path.exists() {
                Self::install()?;
            }

            let status = Command::new("launchctl")
                .args(["load", "-w", plist_path.to_str().unwrap()])
                .status()
                .context("Failed to run launchctl load")?;

            if !status.success() {
                // If already loaded, bootstrap or restart
                let _ = Command::new("launchctl")
                    .args(["start", "com.boxr.daemon"])
                    .status();
            }

            println!("✓ Started boxr daemon service (com.boxr.daemon)");
            println!(
                "  Listening on unix://{}",
                boxr_home().join("boxr.sock").display()
            );
        }

        #[cfg(target_os = "linux")]
        {
            let s_path = Self::service_path()?;
            if !s_path.exists() {
                Self::install()?;
            }

            let status = Command::new("systemctl")
                .args(["--user", "start", "boxr"])
                .status()
                .context("Failed to start systemd boxr service")?;

            if !status.success() {
                return Err(anyhow!("systemctl failed to start boxr service"));
            }

            println!("✓ Started boxr daemon service via systemd");
            println!(
                "  Listening on unix://{}",
                boxr_home().join("boxr.sock").display()
            );
        }

        Ok(())
    }

    /// Stop the background service
    pub fn stop() -> Result<()> {
        #[cfg(target_os = "macos")]
        {
            let plist_path = Self::plist_path()?;
            if plist_path.exists() {
                let _ = Command::new("launchctl")
                    .args(["unload", "-w", plist_path.to_str().unwrap()])
                    .status();
            }
            println!("✓ Stopped boxr daemon service");
        }

        #[cfg(target_os = "linux")]
        {
            let _ = Command::new("systemctl")
                .args(["--user", "stop", "boxr"])
                .status();
            println!("✓ Stopped boxr daemon service");
        }

        Ok(())
    }

    /// Display the live service status
    pub fn status() -> Result<()> {
        println!("{:<24} : Boxr Daemon Service", "Service Name");

        #[cfg(target_os = "macos")]
        {
            let plist_path = Self::plist_path()?;
            let is_installed = plist_path.exists();
            println!(
                "{:<24} : {}",
                "Configuration",
                if is_installed {
                    "Installed"
                } else {
                    "Not installed"
                }
            );
            if is_installed {
                println!("{:<24} : {}", "LaunchAgent Path", plist_path.display());
            }

            let output = Command::new("launchctl").args(["list"]).output();

            let is_running = if let Ok(out) = output {
                String::from_utf8_lossy(&out.stdout).contains("com.boxr.daemon")
            } else {
                false
            };

            println!(
                "{:<24} : {}",
                "Running State",
                if is_running {
                    "Active (Running)"
                } else {
                    "Inactive (Stopped)"
                }
            );
        }

        #[cfg(target_os = "linux")]
        {
            let s_path = Self::service_path()?;
            let is_installed = s_path.exists();
            println!(
                "{:<24} : {}",
                "Configuration",
                if is_installed {
                    "Installed"
                } else {
                    "Not installed"
                }
            );

            let output = Command::new("systemctl")
                .args(["--user", "is-active", "boxr"])
                .output();

            let is_active = if let Ok(out) = output {
                String::from_utf8_lossy(&out.stdout).trim() == "active"
            } else {
                false
            };

            println!(
                "{:<24} : {}",
                "Running State",
                if is_active {
                    "Active (Running)"
                } else {
                    "Inactive (Stopped)"
                }
            );
        }

        let sock = boxr_home().join("boxr.sock");
        println!("{:<24} : {}", "Socket File", sock.display());
        println!(
            "{:<24} : {}",
            "Socket Available",
            if sock.exists() { "Yes" } else { "No" }
        );
        println!(
            "{:<24} : {}",
            "Log File",
            boxr_home().join("daemon.log").display()
        );

        Ok(())
    }

    /// Uninstall service definition and remove autostart
    pub fn uninstall() -> Result<()> {
        Self::stop()?;

        #[cfg(target_os = "macos")]
        {
            let plist_path = Self::plist_path()?;
            if plist_path.exists() {
                let _ = fs::remove_file(&plist_path);
            }
            println!(
                "✓ Uninstalled launchd service plist: {}",
                plist_path.display()
            );
        }

        #[cfg(target_os = "linux")]
        {
            let s_path = Self::service_path()?;
            let _ = Command::new("systemctl")
                .args(["--user", "disable", "boxr"])
                .status();
            if s_path.exists() {
                let _ = fs::remove_file(&s_path);
            }
            let _ = Command::new("systemctl")
                .args(["--user", "daemon-reload"])
                .status();
            println!("✓ Uninstalled systemd service: {}", s_path.display());
        }

        Ok(())
    }
}

fn dirs_home() -> Result<PathBuf> {
    if let Ok(h) = std::env::var("HOME") {
        return Ok(PathBuf::from(h));
    }
    Err(anyhow!("Could not determine HOME directory"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_executable_resolution() {
        let exe = find_boxr_executable();
        assert!(!exe.as_os_str().is_empty());
    }

    #[test]
    fn test_service_status_query() {
        assert!(ServiceManager::status().is_ok());
    }
}
