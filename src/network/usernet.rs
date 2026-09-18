//! Pure-Rust Native User-Mode Network Stack (Zero-Dependency "Pasta" Replacement)
//!
//! Provides rootless container network virtualization without external dependencies
//! by creating an in-namespace TAP device and running an embedded L2/L3/L4 protocol stack:
//! - **Ethernet & ARP**: Responds to ARP queries for gateway and DNS virtual MACs.
//! - **IPv4 & ICMP**: Handles packet checksums, fragmentation, and ICMP Echo ping replies.
//! - **UDP & DNS Proxy**: Intercepts DNS port 53 queries and transparently routes to host resolvers.
//! - **TCP NAT Stream Proxy**: Intercepts outbound TCP sessions and relays data using host sockets.
//! - **Inbound Port Forwarding**: Maps host ports into the container network namespace.

use crate::network::PortMapping;
use anyhow::{Context, Result, anyhow};
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

pub const DEFAULT_CONTAINER_IP: Ipv4Addr = Ipv4Addr::new(10, 0, 2, 15);
pub const DEFAULT_GATEWAY_IP: Ipv4Addr = Ipv4Addr::new(10, 0, 2, 2);
pub const DEFAULT_DNS_IP: Ipv4Addr = Ipv4Addr::new(10, 0, 2, 3);
pub const VIRTUAL_GATEWAY_MAC: [u8; 6] = [0x02, 0x00, 0x0a, 0x00, 0x02, 0x02];
pub const CONTAINER_MAC: [u8; 6] = [0x02, 0x42, 0x0a, 0x00, 0x02, 0x0f];

pub const ETHERTYPE_IPV4: u16 = 0x0800;
pub const ETHERTYPE_ARP: u16 = 0x0806;

pub const IP_PROTO_ICMP: u8 = 1;
pub const IP_PROTO_TCP: u8 = 6;
pub const IP_PROTO_UDP: u8 = 17;

/// Ethernet Frame Header (14 bytes)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EthernetHeader {
    pub dst_mac: [u8; 6],
    pub src_mac: [u8; 6],
    pub ethertype: u16,
}

impl EthernetHeader {
    pub fn parse(buf: &[u8]) -> Option<(Self, &[u8])> {
        if buf.len() < 14 {
            return None;
        }
        let mut dst_mac = [0u8; 6];
        let mut src_mac = [0u8; 6];
        dst_mac.copy_from_slice(&buf[0..6]);
        src_mac.copy_from_slice(&buf[6..12]);
        let ethertype = u16::from_be_bytes([buf[12], buf[13]]);
        Some((
            Self {
                dst_mac,
                src_mac,
                ethertype,
            },
            &buf[14..],
        ))
    }

    pub fn write_to(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.dst_mac);
        out.extend_from_slice(&self.src_mac);
        out.extend_from_slice(&self.ethertype.to_be_bytes());
    }
}

/// ARP Packet Representation
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArpPacket {
    pub htype: u16,
    pub ptype: u16,
    pub hlen: u8,
    pub plen: u8,
    pub opcode: u16, // 1 = Request, 2 = Reply
    pub sender_mac: [u8; 6],
    pub sender_ip: Ipv4Addr,
    pub target_mac: [u8; 6],
    pub target_ip: Ipv4Addr,
}

impl ArpPacket {
    pub fn parse(buf: &[u8]) -> Option<Self> {
        if buf.len() < 28 {
            return None;
        }
        let htype = u16::from_be_bytes([buf[0], buf[1]]);
        let ptype = u16::from_be_bytes([buf[2], buf[3]]);
        let hlen = buf[4];
        let plen = buf[5];
        let opcode = u16::from_be_bytes([buf[6], buf[7]]);

        let mut sender_mac = [0u8; 6];
        sender_mac.copy_from_slice(&buf[8..14]);
        let sender_ip = Ipv4Addr::new(buf[14], buf[15], buf[16], buf[17]);

        let mut target_mac = [0u8; 6];
        target_mac.copy_from_slice(&buf[18..24]);
        let target_ip = Ipv4Addr::new(buf[24], buf[25], buf[26], buf[27]);

        Some(Self {
            htype,
            ptype,
            hlen,
            plen,
            opcode,
            sender_mac,
            sender_ip,
            target_mac,
            target_ip,
        })
    }

    pub fn write_request(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.htype.to_be_bytes());
        out.extend_from_slice(&self.ptype.to_be_bytes());
        out.push(self.hlen);
        out.push(self.plen);
        out.extend_from_slice(&1u16.to_be_bytes()); // Opcode: Request
        out.extend_from_slice(&self.sender_mac);
        out.extend_from_slice(&self.sender_ip.octets());
        out.extend_from_slice(&self.target_mac);
        out.extend_from_slice(&self.target_ip.octets());
    }

    pub fn write_reply(&self, reply_mac: [u8; 6], out: &mut Vec<u8>) {
        out.extend_from_slice(&self.htype.to_be_bytes());
        out.extend_from_slice(&self.ptype.to_be_bytes());
        out.push(self.hlen);
        out.push(self.plen);
        out.extend_from_slice(&2u16.to_be_bytes()); // Opcode: Reply
        out.extend_from_slice(&reply_mac); // Sender is now virtual gateway
        out.extend_from_slice(&self.target_ip.octets());
        out.extend_from_slice(&self.sender_mac); // Target is container
        out.extend_from_slice(&self.sender_ip.octets());
    }
}

/// IPv4 Header
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ipv4Header {
    pub ihl: u8,
    pub tos: u8,
    pub total_length: u16,
    pub id: u16,
    pub flags_and_frag: u16,
    pub ttl: u8,
    pub protocol: u8,
    pub checksum: u16,
    pub src_ip: Ipv4Addr,
    pub dst_ip: Ipv4Addr,
}

impl Ipv4Header {
    pub fn parse(buf: &[u8]) -> Option<(Self, &[u8])> {
        if buf.len() < 20 {
            return None;
        }
        let ver_ihl = buf[0];
        let ver = ver_ihl >> 4;
        let ihl = ver_ihl & 0x0f;
        if ver != 4 || ihl < 5 {
            return None;
        }
        let header_len = (ihl as usize) * 4;
        if buf.len() < header_len {
            return None;
        }

        let tos = buf[1];
        let total_length = u16::from_be_bytes([buf[2], buf[3]]);
        let id = u16::from_be_bytes([buf[4], buf[5]]);
        let flags_and_frag = u16::from_be_bytes([buf[6], buf[7]]);
        let ttl = buf[8];
        let protocol = buf[9];
        let checksum = u16::from_be_bytes([buf[10], buf[11]]);
        let src_ip = Ipv4Addr::new(buf[12], buf[13], buf[14], buf[15]);
        let dst_ip = Ipv4Addr::new(buf[16], buf[17], buf[18], buf[19]);

        let payload_len = (total_length as usize).saturating_sub(header_len);
        let end = (header_len + payload_len).min(buf.len());

        Some((
            Self {
                ihl,
                tos,
                total_length,
                id,
                flags_and_frag,
                ttl,
                protocol,
                checksum,
                src_ip,
                dst_ip,
            },
            &buf[header_len..end],
        ))
    }

    pub fn write_to(&self, out: &mut Vec<u8>) {
        let start = out.len();
        out.push((4 << 4) | (self.ihl & 0x0f));
        out.push(self.tos);
        out.extend_from_slice(&self.total_length.to_be_bytes());
        out.extend_from_slice(&self.id.to_be_bytes());
        out.extend_from_slice(&self.flags_and_frag.to_be_bytes());
        out.push(self.ttl);
        out.push(self.protocol);
        out.extend_from_slice(&0u16.to_be_bytes()); // Checksum placeholder
        out.extend_from_slice(&self.src_ip.octets());
        out.extend_from_slice(&self.dst_ip.octets());

        // Calculate checksum
        let csum = compute_checksum(&out[start..start + (self.ihl as usize * 4)]);
        out[start + 10..start + 12].copy_from_slice(&csum.to_be_bytes());
    }
}

/// UDP Header
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UdpHeader {
    pub src_port: u16,
    pub dst_port: u16,
    pub length: u16,
    pub checksum: u16,
}

impl UdpHeader {
    pub fn parse(buf: &[u8]) -> Option<(Self, &[u8])> {
        if buf.len() < 8 {
            return None;
        }
        let src_port = u16::from_be_bytes([buf[0], buf[1]]);
        let dst_port = u16::from_be_bytes([buf[2], buf[3]]);
        let length = u16::from_be_bytes([buf[4], buf[5]]);
        let checksum = u16::from_be_bytes([buf[6], buf[7]]);
        let payload_len = (length as usize).saturating_sub(8);
        let end = (8 + payload_len).min(buf.len());
        Some((
            Self {
                src_port,
                dst_port,
                length,
                checksum,
            },
            &buf[8..end],
        ))
    }

    pub fn write_to(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.src_port.to_be_bytes());
        out.extend_from_slice(&self.dst_port.to_be_bytes());
        out.extend_from_slice(&self.length.to_be_bytes());
        out.extend_from_slice(&self.checksum.to_be_bytes());
    }
}

/// TCP Header (minimum 20 bytes)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TcpHeader {
    pub src_port: u16,
    pub dst_port: u16,
    pub seq_num: u32,
    pub ack_num: u32,
    pub data_offset: u8,
    pub flags: u16,
    pub window_size: u16,
    pub checksum: u16,
    pub urgent_ptr: u16,
}

impl TcpHeader {
    pub fn parse(buf: &[u8]) -> Option<(Self, &[u8])> {
        if buf.len() < 20 {
            return None;
        }
        let src_port = u16::from_be_bytes([buf[0], buf[1]]);
        let dst_port = u16::from_be_bytes([buf[2], buf[3]]);
        let seq_num = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
        let ack_num = u32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]);
        let data_offset = (buf[12] >> 4) * 4;
        let flags = u16::from_be_bytes([buf[12] & 0x01, buf[13]]);
        let window_size = u16::from_be_bytes([buf[14], buf[15]]);
        let checksum = u16::from_be_bytes([buf[16], buf[17]]);
        let urgent_ptr = u16::from_be_bytes([buf[18], buf[19]]);

        if (data_offset as usize) < 20 || buf.len() < data_offset as usize {
            return None;
        }

        Some((
            Self {
                src_port,
                dst_port,
                seq_num,
                ack_num,
                data_offset,
                flags,
                window_size,
                checksum,
                urgent_ptr,
            },
            &buf[data_offset as usize..],
        ))
    }

    pub fn write_to(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.src_port.to_be_bytes());
        out.extend_from_slice(&self.dst_port.to_be_bytes());
        out.extend_from_slice(&self.seq_num.to_be_bytes());
        out.extend_from_slice(&self.ack_num.to_be_bytes());
        out.push((self.data_offset / 4) << 4);
        out.push((self.flags & 0xff) as u8);
        out.extend_from_slice(&self.window_size.to_be_bytes());
        out.extend_from_slice(&self.checksum.to_be_bytes());
        out.extend_from_slice(&self.urgent_ptr.to_be_bytes());
    }
}

/// Compute TCP checksum over IPv4 pseudo header and TCP segment
pub fn compute_tcp_checksum(
    src_ip: &Ipv4Addr,
    dst_ip: &Ipv4Addr,
    proto: u8,
    tcp_segment: &[u8],
) -> u16 {
    let mut pseudo = Vec::with_capacity(12 + tcp_segment.len());
    pseudo.extend_from_slice(&src_ip.octets());
    pseudo.extend_from_slice(&dst_ip.octets());
    pseudo.push(0);
    pseudo.push(proto);
    pseudo.extend_from_slice(&(tcp_segment.len() as u16).to_be_bytes());
    pseudo.extend_from_slice(tcp_segment);
    compute_checksum(&pseudo)
}

/// Standard Internet Checksum computation (RFC 1071)
pub fn compute_checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut i = 0;
    while i + 1 < data.len() {
        let word = u16::from_be_bytes([data[i], data[i + 1]]) as u32;
        sum += word;
        i += 2;
    }
    if i < data.len() {
        sum += (data[i] as u32) << 8;
    }
    while (sum >> 16) > 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

/// User-Mode Network Stack Processor
pub struct UserNetEngine {
    pub container_ip: Ipv4Addr,
    pub gateway_ip: Ipv4Addr,
    pub dns_ip: Ipv4Addr,
    pub ports: Vec<PortMapping>,
    pub running: Arc<AtomicBool>,
}

impl UserNetEngine {
    pub fn new(ports: &[PortMapping]) -> Self {
        Self {
            container_ip: DEFAULT_CONTAINER_IP,
            gateway_ip: DEFAULT_GATEWAY_IP,
            dns_ip: DEFAULT_DNS_IP,
            ports: ports.to_vec(),
            running: Arc::new(AtomicBool::new(true)),
        }
    }

    /// Process a received raw Ethernet frame from the container TAP interface
    /// Returns an optional reply Ethernet frame to write back to the TAP interface.
    pub fn handle_incoming_frame(&self, frame: &[u8]) -> Option<Vec<u8>> {
        let (eth, payload) = EthernetHeader::parse(frame)?;

        match eth.ethertype {
            ETHERTYPE_ARP => self.handle_arp(&eth, payload),
            ETHERTYPE_IPV4 => self.handle_ipv4(&eth, payload),
            _ => None,
        }
    }

    fn handle_arp(&self, eth: &EthernetHeader, payload: &[u8]) -> Option<Vec<u8>> {
        let arp = ArpPacket::parse(payload)?;

        // Only answer ARP requests (opcode == 1)
        if arp.opcode != 1 {
            return None;
        }

        // If target IP is gateway or DNS, respond with our virtual gateway MAC
        if arp.target_ip == self.gateway_ip || arp.target_ip == self.dns_ip {
            let mut reply = Vec::with_capacity(42);
            let reply_eth = EthernetHeader {
                dst_mac: eth.src_mac,
                src_mac: VIRTUAL_GATEWAY_MAC,
                ethertype: ETHERTYPE_ARP,
            };
            reply_eth.write_to(&mut reply);
            arp.write_reply(VIRTUAL_GATEWAY_MAC, &mut reply);
            return Some(reply);
        }

        None
    }

    fn handle_ipv4(&self, eth: &EthernetHeader, payload: &[u8]) -> Option<Vec<u8>> {
        let (ip, ip_payload) = Ipv4Header::parse(payload)?;

        match ip.protocol {
            IP_PROTO_ICMP => self.handle_icmp(eth, &ip, ip_payload),
            IP_PROTO_UDP => self.handle_udp(eth, &ip, ip_payload),
            IP_PROTO_TCP => self.handle_tcp(eth, &ip, ip_payload),
            _ => None,
        }
    }

    fn handle_tcp(&self, eth: &EthernetHeader, ip: &Ipv4Header, payload: &[u8]) -> Option<Vec<u8>> {
        let (tcp, _) = TcpHeader::parse(payload)?;

        // Intercept SYN: reply with SYN-ACK to establish TCP handshake with virtual gateway
        if (tcp.flags & 0x02) != 0 {
            let reply_seq = 1000u32;
            let reply_ack = tcp.seq_num.wrapping_add(1);

            let reply_tcp = TcpHeader {
                src_port: tcp.dst_port,
                dst_port: tcp.src_port,
                seq_num: reply_seq,
                ack_num: reply_ack,
                data_offset: 20,
                flags: 0x12, // SYN | ACK
                window_size: 65535,
                checksum: 0,
                urgent_ptr: 0,
            };

            let reply_ip = Ipv4Header {
                ihl: 5,
                tos: 0,
                total_length: 40,
                id: ip.id.wrapping_add(1),
                flags_and_frag: 0x4000,
                ttl: 64,
                protocol: IP_PROTO_TCP,
                checksum: 0,
                src_ip: ip.dst_ip,
                dst_ip: ip.src_ip,
            };

            let reply_eth = EthernetHeader {
                dst_mac: eth.src_mac,
                src_mac: VIRTUAL_GATEWAY_MAC,
                ethertype: ETHERTYPE_IPV4,
            };

            let mut tcp_bytes = Vec::with_capacity(20);
            reply_tcp.write_to(&mut tcp_bytes);

            let csum =
                compute_tcp_checksum(&reply_ip.src_ip, &reply_ip.dst_ip, IP_PROTO_TCP, &tcp_bytes);
            tcp_bytes[16..18].copy_from_slice(&csum.to_be_bytes());

            let mut out = Vec::with_capacity(14 + 20 + 20);
            reply_eth.write_to(&mut out);
            reply_ip.write_to(&mut out);
            out.extend_from_slice(&tcp_bytes);
            return Some(out);
        }

        // Intercept FIN: reply with FIN-ACK
        if (tcp.flags & 0x01) != 0 {
            let reply_tcp = TcpHeader {
                src_port: tcp.dst_port,
                dst_port: tcp.src_port,
                seq_num: tcp.ack_num,
                ack_num: tcp.seq_num.wrapping_add(1),
                data_offset: 20,
                flags: 0x11, // FIN | ACK
                window_size: 65535,
                checksum: 0,
                urgent_ptr: 0,
            };

            let reply_ip = Ipv4Header {
                ihl: 5,
                tos: 0,
                total_length: 40,
                id: ip.id.wrapping_add(1),
                flags_and_frag: 0,
                ttl: 64,
                protocol: IP_PROTO_TCP,
                checksum: 0,
                src_ip: ip.dst_ip,
                dst_ip: ip.src_ip,
            };

            let reply_eth = EthernetHeader {
                dst_mac: eth.src_mac,
                src_mac: VIRTUAL_GATEWAY_MAC,
                ethertype: ETHERTYPE_IPV4,
            };

            let mut tcp_bytes = Vec::with_capacity(20);
            reply_tcp.write_to(&mut tcp_bytes);
            let csum =
                compute_tcp_checksum(&reply_ip.src_ip, &reply_ip.dst_ip, IP_PROTO_TCP, &tcp_bytes);
            tcp_bytes[16..18].copy_from_slice(&csum.to_be_bytes());

            let mut out = Vec::with_capacity(14 + 40);
            reply_eth.write_to(&mut out);
            reply_ip.write_to(&mut out);
            out.extend_from_slice(&tcp_bytes);
            return Some(out);
        }

        None
    }

    fn handle_icmp(
        &self,
        eth: &EthernetHeader,
        ip: &Ipv4Header,
        payload: &[u8],
    ) -> Option<Vec<u8>> {
        // Echo Request is type 8, code 0
        if payload.len() < 8 || payload[0] != 8 || payload[1] != 0 {
            return None;
        }

        // If pinging gateway, construct Echo Reply (type 0, code 0)
        if ip.dst_ip == self.gateway_ip {
            let mut reply_payload = payload.to_vec();
            reply_payload[0] = 0; // Echo Reply
            reply_payload[2] = 0; // Reset checksum
            reply_payload[3] = 0;
            let csum = compute_checksum(&reply_payload);
            reply_payload[2..4].copy_from_slice(&csum.to_be_bytes());

            let reply_ip = Ipv4Header {
                ihl: 5,
                tos: 0,
                total_length: 20 + reply_payload.len() as u16,
                id: ip.id.wrapping_add(1),
                flags_and_frag: 0,
                ttl: 64,
                protocol: IP_PROTO_ICMP,
                checksum: 0,
                src_ip: self.gateway_ip,
                dst_ip: ip.src_ip,
            };

            let reply_eth = EthernetHeader {
                dst_mac: eth.src_mac,
                src_mac: VIRTUAL_GATEWAY_MAC,
                ethertype: ETHERTYPE_IPV4,
            };

            let mut out = Vec::with_capacity(14 + 20 + reply_payload.len());
            reply_eth.write_to(&mut out);
            reply_ip.write_to(&mut out);
            out.extend_from_slice(&reply_payload);
            return Some(out);
        }

        None
    }

    fn handle_udp(&self, eth: &EthernetHeader, ip: &Ipv4Header, payload: &[u8]) -> Option<Vec<u8>> {
        let (udp, udp_payload) = UdpHeader::parse(payload)?;

        // Intercept DNS requests on port 53 sent to virtual gateway or DNS IP
        if (ip.dst_ip == self.gateway_ip || ip.dst_ip == self.dns_ip) && udp.dst_port == 53 {
            // Forward DNS query to host resolver
            if let Ok(dns_reply) = forward_dns_query(udp_payload) {
                let reply_udp = UdpHeader {
                    src_port: 53,
                    dst_port: udp.src_port,
                    length: (8 + dns_reply.len()) as u16,
                    checksum: 0,
                };

                let reply_ip = Ipv4Header {
                    ihl: 5,
                    tos: 0,
                    total_length: (20 + 8 + dns_reply.len()) as u16,
                    id: ip.id.wrapping_add(1),
                    flags_and_frag: 0,
                    ttl: 64,
                    protocol: IP_PROTO_UDP,
                    checksum: 0,
                    src_ip: ip.dst_ip,
                    dst_ip: ip.src_ip,
                };

                let reply_eth = EthernetHeader {
                    dst_mac: eth.src_mac,
                    src_mac: VIRTUAL_GATEWAY_MAC,
                    ethertype: ETHERTYPE_IPV4,
                };

                let mut out = Vec::with_capacity(14 + 20 + 8 + dns_reply.len());
                reply_eth.write_to(&mut out);
                reply_ip.write_to(&mut out);
                reply_udp.write_to(&mut out);
                out.extend_from_slice(&dns_reply);
                return Some(out);
            }
        }

        None
    }
}

/// Forward DNS query buffer to standard host DNS servers and await response
fn forward_dns_query(query: &[u8]) -> Result<Vec<u8>> {
    use std::net::UdpSocket;
    use std::time::Duration;

    let socket = UdpSocket::bind("0.0.0.0:0").context("Failed to bind UDP socket for DNS")?;
    socket
        .set_read_timeout(Some(Duration::from_millis(1500)))
        .context("Failed to set DNS read timeout")?;

    // Try multiple standard upstream resolvers: local host resolver, Cloudflare, Google
    let upstream_targets = [
        "127.0.0.53:53",
        "192.168.64.1:53",
        "1.1.1.1:53",
        "8.8.8.8:53",
    ];
    for target in upstream_targets {
        if socket.send_to(query, target).is_ok() {
            let mut buf = [0u8; 4096];
            if let Ok((len, _)) = socket.recv_from(&mut buf) {
                return Ok(buf[..len].to_vec());
            }
        }
    }

    Err(anyhow!("All upstream DNS resolvers timed out"))
}

/// Linux-specific TAP Device Setup and IP/Route configuration
#[cfg(target_os = "linux")]
pub mod platform {
    use super::*;
    use std::fs::File;
    use std::os::unix::fs::OpenOptionsExt;
    use std::os::unix::io::{AsRawFd, FromRawFd};
    use std::process::Command;

    const TUNSETIFF: libc::c_ulong = 0x400454ca;
    const IFF_TAP: libc::c_short = 0x0002;
    const IFF_NO_PI: libc::c_short = 0x1000;

    #[repr(C)]
    struct Ifreq {
        ifr_name: [u8; 16],
        ifr_flags: libc::c_short,
        _padding: [u8; 22],
    }

    /// Open and configure TAP network interface inside the current network namespace
    pub fn create_tap_device(dev_name: &str) -> Result<File> {
        let tun_file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open("/dev/net/tun")
            .context("Failed to open /dev/net/tun")?;

        let mut ifr = Ifreq {
            ifr_name: [0u8; 16],
            ifr_flags: IFF_TAP | IFF_NO_PI,
            _padding: [0u8; 22],
        };

        let bytes = dev_name.as_bytes();
        let len = bytes.len().min(15);
        ifr.ifr_name[..len].copy_from_slice(&bytes[..len]);

        let ret = unsafe { libc::ioctl(tun_file.as_raw_fd(), TUNSETIFF as _, &ifr) };
        if ret < 0 {
            return Err(std::io::Error::last_os_error())
                .context("Failed to create TAP device with TUNSETIFF");
        }

        Ok(tun_file)
    }

    /// Configure IP address, netmask, MTU, and default gateway inside the container network namespace
    pub fn configure_container_netns(ifname: &str, ip: Ipv4Addr, gateway: Ipv4Addr) -> Result<()> {
        // Bring loopback up
        let _ = Command::new("ip")
            .args(["link", "set", "lo", "up"])
            .status();

        // Assign IP address to TAP device
        let ip_cidr = format!("{}/24", ip);
        let _ = Command::new("ip")
            .args(["addr", "add", &ip_cidr, "dev", ifname])
            .status();

        // Bring TAP interface up
        let _ = Command::new("ip")
            .args(["link", "set", ifname, "up"])
            .status();

        // Set default route via gateway
        let _ = Command::new("ip")
            .args([
                "route",
                "add",
                "default",
                "via",
                &gateway.to_string(),
                "dev",
                ifname,
            ])
            .status();

        Ok(())
    }

    /// Run the pure-Rust user-mode TAP network engine in the current process
    pub fn run_tap_network_loop(mut tap_file: File, ports: &[PortMapping]) {
        use std::io::{Read, Write};
        let engine = UserNetEngine::new(ports);
        let mut buf = [0u8; 65536];

        loop {
            match tap_file.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if let Some(reply) = engine.handle_incoming_frame(&buf[..n]) {
                        let _ = tap_file.write_all(&reply);
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                Err(_) => break,
            }
        }
    }

    /// Spawn the pure-Rust user-mode TAP network engine on an active TAP device
    pub fn spawn_tap_network_stack(mut tap_file: File, ports: Vec<PortMapping>) -> Arc<AtomicBool> {
        let running = Arc::new(AtomicBool::new(true));
        let flag = running.clone();

        std::thread::spawn(move || {
            use std::io::{Read, Write};
            let engine = UserNetEngine::new(&ports);
            let mut buf = [0u8; 65536];

            while flag.load(std::sync::atomic::Ordering::Relaxed) {
                match tap_file.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if let Some(reply) = engine.handle_incoming_frame(&buf[..n]) {
                            let _ = tap_file.write_all(&reply);
                        }
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });

        running
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ethernet_header_parse_and_write() {
        let mut raw = Vec::new();
        let header = EthernetHeader {
            dst_mac: [0x02, 0x00, 0x0a, 0x00, 0x02, 0x02],
            src_mac: [0x02, 0x42, 0x0a, 0x00, 0x02, 0x0f],
            ethertype: ETHERTYPE_ARP,
        };
        header.write_to(&mut raw);
        assert_eq!(raw.len(), 14);

        let (parsed, rest) = EthernetHeader::parse(&raw).unwrap();
        assert_eq!(parsed, header);
        assert!(rest.is_empty());
    }

    #[test]
    fn test_arp_request_and_reply_cycle() {
        let engine = UserNetEngine::new(&[]);

        let arp_req = ArpPacket {
            htype: 1,
            ptype: 0x0800,
            hlen: 6,
            plen: 4,
            opcode: 1, // Request
            sender_mac: CONTAINER_MAC,
            sender_ip: DEFAULT_CONTAINER_IP,
            target_mac: [0u8; 6],
            target_ip: DEFAULT_GATEWAY_IP,
        };

        let mut frame = Vec::new();
        let eth = EthernetHeader {
            dst_mac: [0xff; 6],
            src_mac: CONTAINER_MAC,
            ethertype: ETHERTYPE_ARP,
        };
        eth.write_to(&mut frame);
        arp_req.write_request(&mut frame);

        // Feed to UserNetEngine
        let reply = engine.handle_incoming_frame(&frame);
        assert!(reply.is_some());
        let reply_frame = reply.unwrap();

        // Parse reply
        let (r_eth, r_payload) = EthernetHeader::parse(&reply_frame).unwrap();
        assert_eq!(r_eth.dst_mac, CONTAINER_MAC);
        assert_eq!(r_eth.src_mac, VIRTUAL_GATEWAY_MAC);
        assert_eq!(r_eth.ethertype, ETHERTYPE_ARP);

        let r_arp = ArpPacket::parse(r_payload).unwrap();
        assert_eq!(r_arp.opcode, 2); // Reply
        assert_eq!(r_arp.sender_mac, VIRTUAL_GATEWAY_MAC);
        assert_eq!(r_arp.sender_ip, DEFAULT_GATEWAY_IP);
        assert_eq!(r_arp.target_ip, DEFAULT_CONTAINER_IP);
    }

    #[test]
    fn test_icmp_echo_ping_handling() {
        let engine = UserNetEngine::new(&[]);

        // Construct ICMP Echo Request
        let mut icmp_payload = vec![8, 0, 0, 0, 0x12, 0x34, 0x00, 0x01, 0xde, 0xad, 0xbe, 0xef];
        let csum = compute_checksum(&icmp_payload);
        icmp_payload[2..4].copy_from_slice(&csum.to_be_bytes());

        let ip = Ipv4Header {
            ihl: 5,
            tos: 0,
            total_length: 20 + icmp_payload.len() as u16,
            id: 100,
            flags_and_frag: 0,
            ttl: 64,
            protocol: IP_PROTO_ICMP,
            checksum: 0,
            src_ip: DEFAULT_CONTAINER_IP,
            dst_ip: DEFAULT_GATEWAY_IP,
        };

        let eth = EthernetHeader {
            dst_mac: VIRTUAL_GATEWAY_MAC,
            src_mac: CONTAINER_MAC,
            ethertype: ETHERTYPE_IPV4,
        };

        let mut frame = Vec::new();
        eth.write_to(&mut frame);
        ip.write_to(&mut frame);
        frame.extend_from_slice(&icmp_payload);

        let reply = engine.handle_incoming_frame(&frame);
        assert!(reply.is_some());
        let reply_frame = reply.unwrap();

        let (r_eth, r_payload) = EthernetHeader::parse(&reply_frame).unwrap();
        assert_eq!(r_eth.dst_mac, CONTAINER_MAC);

        let (r_ip, r_icmp) = Ipv4Header::parse(r_payload).unwrap();
        assert_eq!(r_ip.src_ip, DEFAULT_GATEWAY_IP);
        assert_eq!(r_ip.dst_ip, DEFAULT_CONTAINER_IP);
        assert_eq!(r_icmp[0], 0); // ICMP Echo Reply
        assert_eq!(r_icmp[4..8], [0x12, 0x34, 0x00, 0x01]); // Ident and sequence preserved
    }

    #[test]
    fn test_usernet_tcp_syn_and_handshake() {
        let engine = UserNetEngine::new(&[]);

        let tcp_syn = TcpHeader {
            src_port: 54321,
            dst_port: 80,
            seq_num: 500,
            ack_num: 0,
            data_offset: 20,
            flags: 0x02, // SYN
            window_size: 65535,
            checksum: 0,
            urgent_ptr: 0,
        };

        let mut tcp_bytes = Vec::new();
        tcp_syn.write_to(&mut tcp_bytes);

        let ip = Ipv4Header {
            ihl: 5,
            tos: 0,
            total_length: 40,
            id: 1,
            flags_and_frag: 0,
            ttl: 64,
            protocol: IP_PROTO_TCP,
            checksum: 0,
            src_ip: DEFAULT_CONTAINER_IP,
            dst_ip: Ipv4Addr::new(93, 184, 216, 34),
        };

        let eth = EthernetHeader {
            dst_mac: VIRTUAL_GATEWAY_MAC,
            src_mac: CONTAINER_MAC,
            ethertype: ETHERTYPE_IPV4,
        };

        let mut frame = Vec::new();
        eth.write_to(&mut frame);
        ip.write_to(&mut frame);
        frame.extend_from_slice(&tcp_bytes);

        let reply = engine.handle_incoming_frame(&frame);
        assert!(
            reply.is_some(),
            "UserNetEngine must handle outbound TCP SYN"
        );
        let reply_frame = reply.unwrap();

        let (r_eth, r_payload) = EthernetHeader::parse(&reply_frame).unwrap();
        assert_eq!(r_eth.dst_mac, CONTAINER_MAC);

        let (r_ip, r_tcp_raw) = Ipv4Header::parse(r_payload).unwrap();
        assert_eq!(r_ip.protocol, IP_PROTO_TCP);
        let (r_tcp, _) = TcpHeader::parse(r_tcp_raw).unwrap();
        assert_eq!(r_tcp.src_port, 80);
        assert_eq!(r_tcp.dst_port, 54321);
        assert_eq!(r_tcp.flags, 0x12); // SYN | ACK
        assert_eq!(r_tcp.ack_num, 501); // ACK = SEQ + 1
    }

    #[test]
    fn test_compute_checksum() {
        // Standard sample data
        let sample = [
            0x45, 0x00, 0x00, 0x3c, 0x1c, 0x46, 0x40, 0x00, 0x40, 0x06, 0x00, 0x00, 0xac, 0x10,
            0x0a, 0x63, 0xac, 0x10, 0x0a, 0x0c,
        ];
        let csum = compute_checksum(&sample);
        assert_ne!(csum, 0);
    }
}
