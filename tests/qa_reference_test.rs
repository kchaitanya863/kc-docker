use boxr::oci::reference::ImageReference;

#[test]
fn test_qa_reference_positive_cases() {
    let test_cases = vec![
        ("alpine", "registry-1.docker.io", "library/alpine", "latest", None),
        ("ubuntu:22.04", "registry-1.docker.io", "library/ubuntu", "22.04", None),
        ("library/nginx:1.25", "registry-1.docker.io", "library/nginx", "1.25", None),
        ("myuser/myrepo:dev", "registry-1.docker.io", "myuser/myrepo", "dev", None),
        ("ghcr.io/homebrew/core/rust:1.75", "ghcr.io", "homebrew/core/rust", "1.75", None),
        ("quay.io/coreos/etcd:v3.5", "quay.io", "coreos/etcd", "v3.5", None),
        ("localhost:5000/my-image:v1", "localhost:5000", "my-image", "v1", None),
        ("registry.internal.corp:8443/team/app:v2.0", "registry.internal.corp:8443", "team/app", "v2.0", None),
        ("alpine@sha256:7144f7e13da1d9", "registry-1.docker.io", "library/alpine", "latest", Some("sha256:7144f7e13da1d9")),
    ];

    for (input, reg, repo, tag, digest) in test_cases {
        let r = ImageReference::parse(input).expect(&format!("Failed to parse valid reference: {}", input));
        assert_eq!(r.registry, reg, "Registry mismatch for {}", input);
        assert_eq!(r.repository, repo, "Repository mismatch for {}", input);
        assert_eq!(r.tag, tag, "Tag mismatch for {}", input);
        assert_eq!(r.digest.as_deref(), digest, "Digest mismatch for {}", input);
    }
}

#[test]
fn test_qa_reference_negative_and_breaking_cases() {
    let invalid_inputs = vec![
        "",                      // Empty
        "   ",                   // Whitespace only
        "\t\n",                  // Escape chars only
    ];

    for input in invalid_inputs {
        assert!(ImageReference::parse(input).is_err(), "Expected error for invalid reference: {:?}", input);
    }
}

#[test]
fn test_qa_reference_display_formatting() {
    let r1 = ImageReference::parse("alpine").unwrap();
    assert_eq!(r1.display_name(), "alpine:latest");

    let r2 = ImageReference::parse("ghcr.io/org/repo:1.0").unwrap();
    assert_eq!(r2.display_name(), "ghcr.io/org/repo:1.0");

    let r3 = ImageReference::parse("myuser/app:v3").unwrap();
    assert_eq!(r3.display_name(), "myuser/app:v3");
}
