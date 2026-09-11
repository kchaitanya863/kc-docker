#![allow(dead_code)]

use anyhow::{Context, Result};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};

/// A user-space TCP port forwarder proxy running in rootless user mode without requiring root/sudo privileges.
pub struct RootlessPortForwarder {
    host_addr: SocketAddr,
    target_addr: SocketAddr,
    running: Arc<AtomicBool>,
}

impl RootlessPortForwarder {
    pub fn new(host_addr: SocketAddr, target_addr: SocketAddr) -> Self {
        Self {
            host_addr,
            target_addr,
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Start the user-space TCP proxy forwarding traffic between host and container target
    pub async fn start(&self) -> Result<()> {
        let listener = TcpListener::bind(self.host_addr)
            .await
            .with_context(|| format!("Failed to bind rootless port forwarder on {}", self.host_addr))?;

        self.running.store(true, Ordering::SeqCst);
        let running_flag = self.running.clone();
        let target = self.target_addr;

        tokio::spawn(async move {
            while running_flag.load(Ordering::SeqCst) {
                if let Ok((mut inbound, _)) = listener.accept().await {
                    tokio::spawn(async move {
                        if let Ok(mut outbound) = TcpStream::connect(target).await {
                            let _ = tokio::io::copy_bidirectional(&mut inbound, &mut outbound).await;
                        }
                    });
                }
            }
        });

        Ok(())
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
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
        forwarder.stop();
        assert!(!forwarder.running.load(Ordering::SeqCst));
    }
}
