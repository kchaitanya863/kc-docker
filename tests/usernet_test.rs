use boxr::network::PortMapping;
use boxr::network::usernet::{
    ArpPacket, EthernetHeader, Ipv4Header, UdpHeader, UserNetEngine, compute_checksum,
    CONTAINER_MAC, DEFAULT_CONTAINER_IP, DEFAULT_DNS_IP, DEFAULT_GATEWAY_IP,
    ETHERTYPE_ARP, ETHERTYPE_IPV4, IP_PROTO_ICMP, VIRTUAL_GATEWAY_MAC,
};
use std::net::Ipv4Addr;

#[test]
fn test_ethernet_header_roundtrip() {
    let mut raw = Vec::new();
    let header = EthernetHeader {
        dst_mac: [0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff],
        src_mac: [0x11, 0x22, 0x33, 0x44, 0x55, 0x66],
        ethertype: ETHERTYPE_IPV4,
    };
    header.write_to(&mut raw);
    assert_eq!(raw.len(), 14);

    let (parsed, rest) = EthernetHeader::parse(&raw).unwrap();
    assert_eq!(parsed, header);
    assert!(rest.is_empty());
}

#[test]
fn test_usernet_engine_arp_gateway_and_dns() {
    let engine = UserNetEngine::new(&[]);

    // 1. ARP Request for Gateway
    let mut gw_frame = Vec::new();
    let eth_gw = EthernetHeader {
        dst_mac: [0xff; 6],
        src_mac: CONTAINER_MAC,
        ethertype: ETHERTYPE_ARP,
    };
    eth_gw.write_to(&mut gw_frame);
    let arp_gw = ArpPacket {
        htype: 1,
        ptype: 0x0800,
        hlen: 6,
        plen: 4,
        opcode: 1,
        sender_mac: CONTAINER_MAC,
        sender_ip: DEFAULT_CONTAINER_IP,
        target_mac: [0u8; 6],
        target_ip: DEFAULT_GATEWAY_IP,
    };
    arp_gw.write_request(&mut gw_frame);

    let reply_gw = engine.handle_incoming_frame(&gw_frame).unwrap();
    let (r_eth, r_arp_buf) = EthernetHeader::parse(&reply_gw).unwrap();
    assert_eq!(r_eth.dst_mac, CONTAINER_MAC);
    assert_eq!(r_eth.src_mac, VIRTUAL_GATEWAY_MAC);
    let r_arp = ArpPacket::parse(r_arp_buf).unwrap();
    assert_eq!(r_arp.opcode, 2); // Reply
    assert_eq!(r_arp.sender_ip, DEFAULT_GATEWAY_IP);
    assert_eq!(r_arp.target_ip, DEFAULT_CONTAINER_IP);

    // 2. ARP Request for DNS
    let mut dns_frame = Vec::new();
    eth_gw.write_to(&mut dns_frame);
    let arp_dns = ArpPacket {
        htype: 1,
        ptype: 0x0800,
        hlen: 6,
        plen: 4,
        opcode: 1,
        sender_mac: CONTAINER_MAC,
        sender_ip: DEFAULT_CONTAINER_IP,
        target_mac: [0u8; 6],
        target_ip: DEFAULT_DNS_IP,
    };
    arp_dns.write_request(&mut dns_frame);

    let reply_dns = engine.handle_incoming_frame(&dns_frame).unwrap();
    let (_, r_dns_buf) = EthernetHeader::parse(&reply_dns).unwrap();
    let r_arp_dns = ArpPacket::parse(r_dns_buf).unwrap();
    assert_eq!(r_arp_dns.opcode, 2);
    assert_eq!(r_arp_dns.sender_ip, DEFAULT_DNS_IP);

    // 3. ARP Request for Unknown IP -> None
    let mut unknown_frame = Vec::new();
    eth_gw.write_to(&mut unknown_frame);
    let arp_unknown = ArpPacket {
        htype: 1,
        ptype: 0x0800,
        hlen: 6,
        plen: 4,
        opcode: 1,
        sender_mac: CONTAINER_MAC,
        sender_ip: DEFAULT_CONTAINER_IP,
        target_mac: [0u8; 6],
        target_ip: Ipv4Addr::new(192, 168, 99, 99),
    };
    arp_unknown.write_request(&mut unknown_frame);
    assert!(engine.handle_incoming_frame(&unknown_frame).is_none());
}

#[test]
fn test_usernet_engine_icmp_ping_gateway() {
    let engine = UserNetEngine::new(&[]);

    let mut icmp_payload = vec![8, 0, 0, 0, 0xaa, 0xbb, 0x00, 0x05, 0x11, 0x22, 0x33, 0x44];
    let csum = compute_checksum(&icmp_payload);
    icmp_payload[2..4].copy_from_slice(&csum.to_be_bytes());

    let ip = Ipv4Header {
        ihl: 5,
        tos: 0,
        total_length: 20 + icmp_payload.len() as u16,
        id: 5432,
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

    let reply = engine.handle_incoming_frame(&frame).unwrap();
    let (r_eth, r_ip_buf) = EthernetHeader::parse(&reply).unwrap();
    assert_eq!(r_eth.dst_mac, CONTAINER_MAC);
    assert_eq!(r_eth.src_mac, VIRTUAL_GATEWAY_MAC);

    let (r_ip, r_icmp) = Ipv4Header::parse(r_ip_buf).unwrap();
    assert_eq!(r_ip.src_ip, DEFAULT_GATEWAY_IP);
    assert_eq!(r_ip.dst_ip, DEFAULT_CONTAINER_IP);
    assert_eq!(r_icmp[0], 0); // Echo reply
    assert_eq!(r_icmp[1], 0);
    assert_eq!(r_icmp[4..8], [0xaa, 0xbb, 0x00, 0x05]); // Identifier & sequence
}

#[test]
fn test_udp_header_serialization() {
    let udp = UdpHeader {
        src_port: 54321,
        dst_port: 53,
        length: 32,
        checksum: 0,
    };
    let mut raw = Vec::new();
    udp.write_to(&mut raw);
    raw.extend_from_slice(b"sample dns payload bytes");

    let (parsed, payload) = UdpHeader::parse(&raw).unwrap();
    assert_eq!(parsed, udp);
    assert_eq!(payload, b"sample dns payload bytes");
}

#[test]
fn test_usernet_port_mappings_stored() {
    let ports = vec![
        PortMapping::parse("8080:80").unwrap(),
        PortMapping::parse("127.0.0.1:3000:3000/tcp").unwrap(),
    ];
    let engine = UserNetEngine::new(&ports);
    assert_eq!(engine.ports.len(), 2);
    assert_eq!(engine.ports[0].host_port, 8080);
    assert_eq!(engine.ports[0].container_port, 80);
    assert_eq!(engine.ports[1].container_port, 3000);
}
