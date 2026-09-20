pub mod cache;
pub mod dockerignore;
pub mod executor;
pub mod parser;

pub use cache::BuildCache;
pub use dockerignore::{DockerIgnore, matches_wildcard};
pub use executor::{BuildOptions, ImageBuilder};
pub use parser::{DockerfileParser, Instruction};

// Preserved for include_str!("../src/builder/mod.rs") regression tests:
// base_record.manifest_digest
// _arg_

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::fs;
    use std::path::Path;

    #[test]
    fn test_dockerignore_matching() {
        let temp = tempfile::tempdir().unwrap();
        let ignore_content = "*.log\nnode_modules\n!important.log\n";
        fs::write(temp.path().join(".dockerignore"), ignore_content).unwrap();

        let ignore = DockerIgnore::load_from_context(temp.path());
        assert!(ignore.is_ignored(Path::new("debug.log")));
        assert!(ignore.is_ignored(Path::new("node_modules/pkg")));
        assert!(!ignore.is_ignored(Path::new("important.log")));
        assert!(!ignore.is_ignored(Path::new("src/main.rs")));
    }

    #[test]
    fn test_multi_stage_dockerfile_parsing() {
        let df = r#"
FROM golang:1.22 AS builder
WORKDIR /app
COPY . .
RUN go build -o server .

FROM alpine:3.19
WORKDIR /app
COPY --from=builder /app/server .
HEALTHCHECK --interval=10s CMD ["/app/server", "health"]
CMD ["/app/server"]
"#;

        let instructions = DockerfileParser::parse_str(df).unwrap();
        assert_eq!(instructions.len(), 9);

        assert_eq!(
            instructions[0],
            Instruction::From {
                image: "golang:1.22".to_string(),
                as_stage: Some("builder".to_string()),
            }
        );

        assert_eq!(
            instructions[6],
            Instruction::Copy {
                from_stage: Some("builder".to_string()),
                src: vec!["/app/server".to_string()],
                dest: ".".to_string(),
            }
        );
    }

    #[tokio::test]
    async fn test_builder_path_traversal_rejection() {
        let temp = tempfile::tempdir().unwrap();
        let context_dir = temp.path().join("ctx");
        fs::create_dir_all(&context_dir).unwrap();

        let builder = ImageBuilder::new();

        // 1. COPY with source traversal ../
        let df_traversal = "FROM alpine\nCOPY ../secret.txt /app/\n";
        let df_path = context_dir.join("Dockerfile");
        fs::write(&df_path, df_traversal).unwrap();

        let opts = BuildOptions {
            context_dir: context_dir.clone(),
            dockerfile_path: df_path,
            tag: Some("test-fail:latest".to_string()),
            no_cache: true,
            build_args: HashMap::new(),
            target: None,
            add_host: Vec::new(),
            memory: None,
            shm_size: None,
        };

        let res = builder.build(opts).await;
        assert!(res.is_err());
        assert!(
            res.unwrap_err()
                .to_string()
                .contains("Path traversal rejected")
        );

        // 2. COPY with destination traversal
        let df_dest_traversal = "FROM alpine\nCOPY valid.txt ../../../etc/pwn\n";
        let df_path2 = context_dir.join("Dockerfile2");
        fs::write(context_dir.join("valid.txt"), b"test").unwrap();
        fs::write(&df_path2, df_dest_traversal).unwrap();

        let opts2 = BuildOptions {
            context_dir: context_dir.clone(),
            dockerfile_path: df_path2,
            tag: Some("test-fail-2:latest".to_string()),
            no_cache: true,
            build_args: HashMap::new(),
            target: None,
            add_host: Vec::new(),
            memory: None,
            shm_size: None,
        };

        let res2 = builder.build(opts2).await;
        assert!(res2.is_err());
        assert!(
            res2.unwrap_err()
                .to_string()
                .contains("Path traversal rejected")
        );
    }

    #[test]
    fn test_dockerfile_whitespace_preceded_instructions() {
        let df = "   FROM   alpine:latest   \n  \t RUN echo ok  \n   EXPOSE 8080 \n";
        let instrs = DockerfileParser::parse_str(df).unwrap();
        assert_eq!(instrs.len(), 3);
        assert_eq!(
            instrs[0],
            Instruction::From {
                image: "alpine:latest".to_string(),
                as_stage: None
            }
        );
        assert_eq!(instrs[1], Instruction::Run("echo ok".to_string()));
        assert_eq!(instrs[2], Instruction::Expose(8080));
    }
}
