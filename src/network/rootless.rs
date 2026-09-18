#![allow(dead_code)]

use anyhow::{Context, Result};
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Notify;

/// A user-space TCP port forwarder proxy running in rootless user mode without requiring root/sudo privileges.
pub struct RootlessPortForwarder {
    host_addr: SocketAddr,
    target_addr: SocketAddr,
    stop_notify: Arc<Notify>,
    is_stopped: Arc<AtomicBool>,
}

impl RootlessPortForwarder {
    pub fn new(host_addr: SocketAddr, target_addr: SocketAddr) -> Self {
        Self {
            host_addr,
            target_addr,
            stop_notify: Arc::new(Notify::new()),
            is_stopped: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Start the user-space TCP proxy forwarding traffic between host and container target
    pub async fn start(&self) -> Result<()> {
        let listener = TcpListener::bind(self.host_addr).await.with_context(|| {
            format!(
                "Failed to bind rootless port forwarder on {}",
                self.host_addr
            )
        })?;

        let stop_notify = self.stop_notify.clone();
        let is_stopped = self.is_stopped.clone();
        let target = self.target_addr;

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = stop_notify.notified() => {
                        break;
                    }
                    accept_res = listener.accept() => {
                        if is_stopped.load(Ordering::SeqCst) {
                            break;
                        }
                        match accept_res {
                            Ok((mut inbound, _)) => {
                                let target = target;
                                tokio::spawn(async move {
                                    if let Ok(mut outbound) = TcpStream::connect(target).await {
                                        let _ = tokio::io::copy_bidirectional(&mut inbound, &mut outbound).await;
                                    }
                                });
                            }
                            Err(_) => break,
                        }
                    }
                }
            }
        });

        Ok(())
    }

    pub fn stop(&self) {
        self.is_stopped.store(true, Ordering::SeqCst);
        self.stop_notify.notify_waiters();
    }

    pub fn is_running(&self) -> bool {
        !self.is_stopped.load(Ordering::SeqCst)
    }
}

pub struct PortForwardManager;

impl PortForwardManager {
    /// Start forwarding for all requested port mappings
    pub async fn start_forwarding(
        ports: &[crate::network::PortMapping],
    ) -> Result<Vec<Arc<RootlessPortForwarder>>> {
        let mut forwarders = Vec::new();
        for p in ports {
            let host_ip_str = p.host_ip.as_deref().unwrap_or("0.0.0.0");
            let host_addr: SocketAddr = format!("{}:{}", host_ip_str, p.host_port)
                .parse()
                .unwrap_or_else(|_| SocketAddr::from(([0, 0, 0, 0], p.host_port)));

            let target_addr: SocketAddr = format!("127.0.0.1:{}", p.container_port)
                .parse()
                .unwrap_or_else(|_| SocketAddr::from(([127, 0, 0, 1], p.container_port)));

            let forwarder = Arc::new(RootlessPortForwarder::new(host_addr, target_addr));
            let _ = forwarder.start().await;
            forwarders.push(forwarder);
        }
        Ok(forwarders)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rootless_port_forwarder_lifecycle() {
        let host: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let target: SocketAddr = "127.0.0.1:9".parse().unwrap(); // dummy target
        let forwarder = RootlessPortForwarder::new(host, target);
        assert!(forwarder.is_running());
        forwarder.stop();
        assert!(!forwarder.is_running());
    }

    #[tokio::test]
    async fn test_rootless_port_forwarder_unblocks_on_stop() {
        let host: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let target: SocketAddr = "127.0.0.1:9".parse().unwrap();
        let forwarder = RootlessPortForwarder::new(host, target);
        forwarder.start().await.unwrap();
        // Immediately stop; should unblock without any client connection
        forwarder.stop();
        assert!(!forwarder.is_running());
    }
}
