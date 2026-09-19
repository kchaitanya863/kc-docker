use boxr::builder::{DockerIgnore, DockerfileParser, Instruction};
use boxr::compose::{ComposeFile, ComposeProject};
use std::path::{Path, PathBuf};
use tempfile::tempdir;

#[test]
fn test_qa_dockerfile_line_continuations_and_comments() {
    let content = r#"
# Comment 1
# Comment 2

FROM \
    alpine:3.19 AS \
    builder

WORKDIR \
    /workspace/app

ENV \
    APP_PORT=8080 \
    APP_ENV=production

RUN echo "line 1" && \
    echo "line 2" && \
    echo "line 3"

EXPOSE 8080

CMD ["/bin/sh", "-c", "echo ready"]
"#;

    let instructions = DockerfileParser::parse_str(content).unwrap();
    assert_eq!(instructions.len(), 7);

    match &instructions[0] {
        Instruction::From { image, as_stage } => {
            assert_eq!(image, "alpine:3.19");
            assert_eq!(as_stage.as_deref(), Some("builder"));
        }
        _ => panic!("Expected FROM instruction"),
    }

    match &instructions[1] {
        Instruction::Workdir(dir) => assert_eq!(dir, "/workspace/app"),
        _ => panic!("Expected WORKDIR instruction"),
    }
}

#[test]
fn test_qa_dockerfile_negative_cases() {
    // 1. Unknown instruction
    let bad_inst = "FLY_TO_MARS yes";
    assert!(DockerfileParser::parse_str(bad_inst).is_err());

    // 2. Empty instruction
    let empty_from = "FROM";
    assert!(DockerfileParser::parse_str(empty_from).is_err());

    // 3. Invalid port in EXPOSE
    let bad_port = "FROM alpine\nEXPOSE invalid_port";
    assert!(DockerfileParser::parse_str(bad_port).is_err());

    // 4. COPY without destination
    let bad_copy = "FROM alpine\nCOPY file1.txt";
    assert!(DockerfileParser::parse_str(bad_copy).is_err());
}

#[test]
fn test_qa_dockerignore_wildcard_and_negation_rules() {
    let temp = tempdir().unwrap();
    let dockerignore_content = r#"
*.tmp
temp/
docs/*.md
!docs/README.md
vendor/*
!vendor/manifest.json
"#;

    std::fs::write(temp.path().join(".dockerignore"), dockerignore_content).unwrap();
    let ignore = DockerIgnore::load_from_context(temp.path());

    // *.tmp rule
    assert!(ignore.is_ignored(Path::new("build.tmp")));
    assert!(ignore.is_ignored(Path::new("sub/cache.tmp")));
    assert!(!ignore.is_ignored(Path::new("build.txt")));

    // temp/ rule
    assert!(ignore.is_ignored(Path::new("temp/data.json")));

    // docs/ negation rule
    assert!(ignore.is_ignored(Path::new("docs/manual.md")));
    assert!(!ignore.is_ignored(Path::new("docs/README.md")));

    // vendor/ negation rule
    assert!(ignore.is_ignored(Path::new("vendor/bundle.js")));
    assert!(!ignore.is_ignored(Path::new("vendor/manifest.json")));
}

#[test]
fn test_qa_compose_cycle_detection() {
    // 2-service cycle
    let yaml_cycle_2 = r#"
services:
  service_a:
    image: alpine
    depends_on:
      - service_b
  service_b:
    image: alpine
    depends_on:
      - service_a
"#;
    let compose2: ComposeFile = serde_yaml::from_str(yaml_cycle_2).unwrap();
    let proj2 = ComposeProject {
        name: "cycle2".to_string(),
        compose_file_path: PathBuf::from("docker-compose.yml"),
        compose: compose2,
    };
    assert!(
        proj2.dependency_order().is_err(),
        "Expected cyclic dependency error for 2-node cycle"
    );

    // 3-service cycle: a -> b -> c -> a
    let yaml_cycle_3 = r#"
services:
  a:
    image: alpine
    depends_on: [b]
  b:
    image: alpine
    depends_on: [c]
  c:
    image: alpine
    depends_on: [a]
"#;
    let compose3: ComposeFile = serde_yaml::from_str(yaml_cycle_3).unwrap();
    let proj3 = ComposeProject {
        name: "cycle3".to_string(),
        compose_file_path: PathBuf::from("docker-compose.yml"),
        compose: compose3,
    };
    assert!(
        proj3.dependency_order().is_err(),
        "Expected cyclic dependency error for 3-node cycle"
    );
}

#[test]
fn test_qa_compose_undefined_dependency() {
    let yaml_missing = r#"
services:
  web:
    image: nginx
    depends_on:
      - non_existent_database
"#;
    let compose: ComposeFile = serde_yaml::from_str(yaml_missing).unwrap();
    let proj = ComposeProject {
        name: "missing".to_string(),
        compose_file_path: PathBuf::from("docker-compose.yml"),
        compose,
    };
    assert!(
        proj.dependency_order().is_err(),
        "Expected error for undefined service dependency"
    );
}
