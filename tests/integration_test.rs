#[test]
fn test_dockerfile_parsing_and_lexing() {
    let dockerfile = r#"
FROM alpine:3.19 AS builder
WORKDIR /workspace
ENV CGO_ENABLED=0 GOOS=linux
COPY go.mod go.sum ./
RUN go mod download
COPY . .
RUN go build -o myapp .

FROM alpine:3.19
WORKDIR /root/
COPY --from=builder /workspace/myapp .
EXPOSE 8080
ENTRYPOINT ["./myapp"]
CMD ["--config", "config.yaml"]
"#;

    // Line continuation test
    let multi_line = r#"
FROM alpine:latest
RUN apk update && \
    apk add --no-cache curl \
    bash git
"#;

    assert!(dockerfile.contains("FROM"));
    assert!(multi_line.contains("apk add"));
}

#[test]
fn test_compose_dependency_resolution() {
    let yaml = r#"
version: '3.8'
services:
  frontend:
    image: nginx
    depends_on:
      - backend
  backend:
    image: my-backend
    depends_on:
      - redis
      - db
  redis:
    image: redis:alpine
  db:
    image: postgres:15
"#;

    let parsed: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
    let services = parsed.get("services").unwrap().as_mapping().unwrap();
    assert_eq!(services.len(), 4);
}

#[test]
fn test_port_spec_parsing() {
    let specs = vec![
        ("8080:80", 8080, 80, "tcp"),
        ("127.0.0.1:3000:3000/udp", 3000, 3000, "udp"),
    ];

    for (spec, expected_host, expected_container, expected_proto) in specs {
        let (proto, port_spec) = if let Some((p, pr)) = spec.split_once('/') {
            (pr, p)
        } else {
            ("tcp", spec)
        };
        let parts: Vec<&str> = port_spec.split(':').collect();
        let (host_port, container_port) = match parts.len() {
            2 => (parts[0].parse::<u16>().unwrap(), parts[1].parse::<u16>().unwrap()),
            3 => (parts[1].parse::<u16>().unwrap(), parts[2].parse::<u16>().unwrap()),
            _ => panic!("unexpected port format"),
        };
        assert_eq!(host_port, expected_host);
        assert_eq!(container_port, expected_container);
        assert_eq!(proto, expected_proto);
    }
}
