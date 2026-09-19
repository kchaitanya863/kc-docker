use boxr::network::{NetworkStore, PortMapping};
use boxr::volume::VolumeStore;

#[test]
fn test_qa_port_mapping_positive_and_negative() {
    // Positive cases
    let valid_specs = vec![
        ("80:80", None, 80, 80, "tcp"),
        ("8080:8080/tcp", None, 8080, 8080, "tcp"),
        (
            "127.0.0.1:5432:5432/udp",
            Some("127.0.0.1"),
            5432,
            5432,
            "udp",
        ),
        ("0.0.0.0:9000:9000", Some("0.0.0.0"), 9000, 9000, "tcp"),
    ];

    for (spec, host_ip, host_port, cont_port, proto) in valid_specs {
        let p = PortMapping::parse(spec).expect(&format!("Failed on valid port spec {}", spec));
        assert_eq!(p.host_ip.as_deref(), host_ip);
        assert_eq!(p.host_port, host_port);
        assert_eq!(p.container_port, cont_port);
        assert_eq!(p.protocol, proto);
    }

    // Negative / Breaking cases
    let invalid_specs = vec![
        "",
        "not_a_port",
        "80:not_a_port",
        "70000:80",              // port > 65535
        "80:70000",              // port > 65535
        "127.0.0.1:80:80:extra", // too many colons
        "-80:80",                // negative port
    ];

    for spec in invalid_specs {
        assert!(
            PortMapping::parse(spec).is_err(),
            "Expected error for invalid port spec: {}",
            spec
        );
    }
}

#[test]
fn test_qa_volume_edge_cases_and_rejections() {
    let store = VolumeStore::new();

    // 1. Invalid mount formats
    assert!(store.resolve_mount("").is_err());
    assert!(store.resolve_mount("a:b:c:d").is_err());

    // 2. Named volume create duplicate check
    let vol_name = format!(
        "qa-vol-{}",
        hex::encode(boxr::storage::container_store::rand_id())
    );
    let v1 = store.create(Some(&vol_name), None).unwrap();
    assert_eq!(v1.name, vol_name);

    // Duplicate create must fail
    assert!(store.create(Some(&vol_name), None).is_err());

    // 3. Remove non-existent volume must fail
    assert!(store.remove("definitely_non_existent_volume_xyz").is_err());

    // Cleanup
    store.remove(&vol_name).unwrap();
}

#[test]
fn test_qa_network_default_protection_and_ipam() {
    let store = NetworkStore::new();

    // 1. Protecting default boxr0 bridge from deletion
    assert!(
        store.remove(NetworkStore::DEFAULT_NETWORK).is_err(),
        "Default network boxr0 must not be removable"
    );

    // 2. IPAM allocation doesn't collide
    let net_name = format!(
        "qa-net-{}",
        hex::encode(boxr::storage::container_store::rand_id())
    );
    let _net = store.create(&net_name, None, None).unwrap();

    let ep1 = store
        .connect_container(&net_name, "c1", "container-one")
        .unwrap();
    let ep2 = store
        .connect_container(&net_name, "c2", "container-two")
        .unwrap();

    assert_ne!(
        ep1.ipv4_address, ep2.ipv4_address,
        "IP addresses must be distinct"
    );
    assert_ne!(
        ep1.mac_address, ep2.mac_address,
        "MAC addresses must be distinct"
    );

    // 3. Removing non-existent network must fail
    assert!(store.remove("non_existent_net_abc").is_err());

    // Cleanup: disconnect containers first, then remove
    store.disconnect_container(&net_name, "c1").unwrap();
    store.disconnect_container(&net_name, "c2").unwrap();
    store.remove(&net_name).unwrap();
}

#[test]
fn test_qa_port_collision_guard() {
    use boxr::guardrails::PortCollisionGuard;
    use boxr::storage::{ContainerRecord, ContainerStatus, ContainerStore};

    let store = ContainerStore::new();

    // Register dummy running container with port 9876
    let test_cid = format!(
        "qa-port-{}",
        hex::encode(boxr::storage::container_store::rand_id())
    );
    let record = ContainerRecord {
        id: test_cid.clone(),
        name: format!("qa-port-cont-{}", &test_cid[..6]),
        image: "alpine:latest".to_string(),
        command: vec!["sleep".to_string()],
        created_at: chrono::Utc::now(),
        status: ContainerStatus::Running,
        bundle_path: "/tmp".to_string(),
        restart_policy: boxr::health::RestartPolicy::No,
        health_status: boxr::health::HealthStatus::None,
        restart_count: 0,
        ports: vec![PortMapping {
            host_ip: None,
            host_port: 9876,
            container_port: 80,
            protocol: "tcp".to_string(),
        }],
        exposed_ports: Vec::new(),
    };
    store.add(record).unwrap();

    // Trying to bind conflicting port 9876 must fail
    let conflict_port = vec![PortMapping {
        host_ip: None,
        host_port: 9876,
        container_port: 8080,
        protocol: "tcp".to_string(),
    }];
    let res = PortCollisionGuard::ensure_no_conflicts(&conflict_port);
    assert!(res.is_err(), "Expected port collision error for port 9876");

    // Non-conflicting port 9877 must pass
    let non_conflict_port = vec![PortMapping {
        host_ip: None,
        host_port: 9877,
        container_port: 80,
        protocol: "tcp".to_string(),
    }];
    assert!(PortCollisionGuard::ensure_no_conflicts(&non_conflict_port).is_ok());

    // Cleanup
    let _ = store.remove(&test_cid);
}
