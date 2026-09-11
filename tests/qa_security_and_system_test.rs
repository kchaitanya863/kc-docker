use boxr::auth::{custom_base64_decode, custom_base64_encode};
use boxr::cgroups::ResourceLimits;
use boxr::health::parse_restart_policy;
use boxr::runtime::kill::ContainerKiller;
use boxr::security::SeccompRule;

#[test]
fn test_qa_base64_roundtrip_all_lengths_and_fuzz() {
    // Empty
    assert_eq!(custom_base64_encode(""), "");
    assert_eq!(custom_base64_decode("").unwrap(), b"");

    // Padding cases
    // 1 byte: 2 padding chars
    assert_eq!(custom_base64_encode("M"), "TQ==");
    assert_eq!(custom_base64_decode("TQ==").unwrap(), b"M");

    // 2 bytes: 1 padding char
    assert_eq!(custom_base64_encode("Ma"), "TWE=");
    assert_eq!(custom_base64_decode("TWE=").unwrap(), b"Ma");

    // 3 bytes: 0 padding chars
    assert_eq!(custom_base64_encode("Man"), "TWFu");
    assert_eq!(custom_base64_decode("TWFu").unwrap(), b"Man");

    // Arbitrary ASCII string
    let test_str = "user:p@ssw0rd!#$*()_+~|}{[]:;?,.<>";
    let encoded = custom_base64_encode(test_str);
    let decoded = custom_base64_decode(&encoded).unwrap();
    assert_eq!(String::from_utf8(decoded).unwrap(), test_str);
}

#[test]
fn test_qa_signal_parsing_matrix() {
    let signals = vec![
        ("1", 1),
        ("HUP", 1),
        ("SIGHUP", 1),
        ("2", 2),
        ("INT", 2),
        ("SIGINT", 2),
        ("9", 9),
        ("KILL", 9),
        ("SIGKILL", 9),
        ("10", 10),
        ("USR1", 10),
        ("SIGUSR1", 10),
        ("12", 12),
        ("USR2", 12),
        ("SIGUSR2", 12),
        ("15", 15),
        ("TERM", 15),
        ("SIGTERM", 15),
        ("18", 18),
        ("CONT", 18),
        ("SIGCONT", 18),
        ("19", 19),
        ("STOP", 19),
        ("SIGSTOP", 19),
    ];

    for (input, expected) in signals {
        assert_eq!(ContainerKiller::parse_signal(input).unwrap(), expected);
    }

    // Invalid signals
    assert!(ContainerKiller::parse_signal("SIGFAKE").is_err());
    assert!(ContainerKiller::parse_signal("NOT_A_SIGNAL").is_err());
}

#[test]
fn test_qa_resource_limits_and_restart_policies() {
    // Memory units
    assert_eq!(ResourceLimits::parse_memory("100b").unwrap(), 100);
    assert_eq!(ResourceLimits::parse_memory("10k").unwrap(), 10 * 1024);
    assert_eq!(ResourceLimits::parse_memory("10kb").unwrap(), 10 * 1024);
    assert_eq!(
        ResourceLimits::parse_memory("256m").unwrap(),
        256 * 1024 * 1024
    );
    assert_eq!(
        ResourceLimits::parse_memory("256mb").unwrap(),
        256 * 1024 * 1024
    );
    assert_eq!(
        ResourceLimits::parse_memory("4g").unwrap(),
        4 * 1024 * 1024 * 1024
    );
    assert_eq!(
        ResourceLimits::parse_memory("4gb").unwrap(),
        4 * 1024 * 1024 * 1024
    );

    // Invalid memory
    assert!(ResourceLimits::parse_memory("not_a_number").is_err());
    assert!(ResourceLimits::parse_memory("100xyz").is_err());

    // CPU limits
    let (quota, period) = ResourceLimits::parse_cpus("2.5").unwrap();
    assert_eq!(period, 100_000);
    assert_eq!(quota, 250_000);
    assert!(ResourceLimits::parse_cpus("invalid_cpu").is_err());

    // Restart policies
    assert!(parse_restart_policy("no").is_ok());
    assert!(parse_restart_policy("always").is_ok());
    assert!(parse_restart_policy("unless-stopped").is_ok());
    assert!(parse_restart_policy("on-failure").is_ok());
    assert!(parse_restart_policy("on-failure:5").is_ok());

    // Invalid restart policies
    assert!(parse_restart_policy("random_policy").is_err());
    assert!(parse_restart_policy("on-failure:not_a_number").is_err());
}

#[test]
fn test_qa_seccomp_profile_blocks_dangerous_syscalls() {
    let filter = SeccompRule::default_filter();
    assert_eq!(filter.default_action, "SCMP_ACT_ALLOW");

    let blocked_syscalls = &filter.syscalls[0];
    assert_eq!(blocked_syscalls.action, "SCMP_ACT_ERRNO");

    let must_block = [
        "reboot",
        "swapon",
        "swapoff",
        "kexec_load",
        "init_module",
        "finit_module",
        "delete_module",
        "ptrace",
        "bpf",
    ];

    for s in must_block {
        assert!(
            blocked_syscalls.names.iter().any(|name| name == s),
            "Seccomp rule must block critical syscall: {}",
            s
        );
    }
}
