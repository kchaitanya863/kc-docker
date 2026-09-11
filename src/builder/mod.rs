use crate::oci::image::{ExecutionConfig, ImageConfig};
use crate::oci::runtime::Spec;
use crate::runtime::execute_bundle;
use crate::storage::{boxr_home, ImageRecord, ImageStore};
use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Instruction {
    From { image: String, as_stage: Option<String> },
    Run(String),
    Copy { src: Vec<String>, dest: String },
    Add { src: Vec<String>, dest: String },
    Workdir(String),
    Env { key: String, value: String },
    Cmd(Vec<String>),
    Entrypoint(Vec<String>),
    Expose(u16),
    Label { key: String, value: String },
}

pub struct DockerfileParser;

impl DockerfileParser {
    pub fn parse_file(path: &Path) -> Result<Vec<Instruction>> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read Dockerfile at {:?}", path))?;
        Self::parse_str(&content)
    }

    pub fn parse_str(content: &str) -> Result<Vec<Instruction>> {
        let mut instructions = Vec::new();
        let mut current_line = String::new();

        for raw_line in content.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some(stripped) = line.strip_suffix('\\') {
                current_line.push_str(stripped.trim());
                current_line.push(' ');
                continue;
            } else {
                current_line.push_str(line);
            }

            let full_line = current_line.trim();
            if !full_line.is_empty() {
                let inst = Self::parse_line(full_line)?;
                instructions.push(inst);
            }
            current_line.clear();
        }

        Ok(instructions)
    }

    fn parse_line(line: &str) -> Result<Instruction> {
        let (keyword, rest) = line.split_once(char::is_whitespace)
            .ok_or_else(|| anyhow!("Invalid Dockerfile instruction: '{}'", line))?;

        let keyword_upper = keyword.to_uppercase();
        let rest = rest.trim();

        match keyword_upper.as_str() {
            "FROM" => {
                let parts: Vec<&str> = rest.split_whitespace().collect();
                if parts.is_empty() {
                    return Err(anyhow!("FROM instruction requires an image"));
                }
                let image = parts[0].to_string();
                let as_stage = if parts.len() >= 3 && parts[1].eq_ignore_ascii_case("AS") {
                    Some(parts[2].to_string())
                } else {
                    None
                };
                Ok(Instruction::From { image, as_stage })
            }
            "RUN" => Ok(Instruction::Run(rest.to_string())),
            "COPY" => {
                let parts = parse_words(rest);
                if parts.len() < 2 {
                    return Err(anyhow!("COPY requires at least one source and one destination"));
                }
                let dest = parts.last().unwrap().clone();
                let src = parts[..parts.len() - 1].to_vec();
                Ok(Instruction::Copy { src, dest })
            }
            "ADD" => {
                let parts = parse_words(rest);
                if parts.len() < 2 {
                    return Err(anyhow!("ADD requires at least one source and one destination"));
                }
                let dest = parts.last().unwrap().clone();
                let src = parts[..parts.len() - 1].to_vec();
                Ok(Instruction::Add { src, dest })
            }
            "WORKDIR" => Ok(Instruction::Workdir(rest.to_string())),
            "ENV" => {
                if let Some((k, v)) = rest.split_once('=') {
                    Ok(Instruction::Env {
                        key: k.trim().to_string(),
                        value: v.trim().trim_matches('"').to_string(),
                    })
                } else if let Some((k, v)) = rest.split_once(char::is_whitespace) {
                    Ok(Instruction::Env {
                        key: k.trim().to_string(),
                        value: v.trim().trim_matches('"').to_string(),
                    })
                } else {
                    Err(anyhow!("Invalid ENV format: '{}'", rest))
                }
            }
            "CMD" => {
                let args = parse_array_or_words(rest);
                Ok(Instruction::Cmd(args))
            }
            "ENTRYPOINT" => {
                let args = parse_array_or_words(rest);
                Ok(Instruction::Entrypoint(args))
            }
            "EXPOSE" => {
                let port: u16 = rest.split('/').next().unwrap_or(rest).trim().parse()
                    .with_context(|| format!("Invalid port in EXPOSE: {}", rest))?;
                Ok(Instruction::Expose(port))
            }
            "LABEL" => {
                if let Some((k, v)) = rest.split_once('=') {
                    Ok(Instruction::Label {
                        key: k.trim().to_string(),
                        value: v.trim().trim_matches('"').to_string(),
                    })
                } else {
                    Err(anyhow!("Invalid LABEL format: '{}'", rest))
                }
            }
            other => Err(anyhow!("Unsupported Dockerfile instruction: {}", other)),
        }
    }
}

pub struct BuildOptions {
    pub context_dir: PathBuf,
    pub dockerfile_path: PathBuf,
    pub tag: Option<String>,
}

pub struct ImageBuilder {
    store: ImageStore,
}

impl ImageBuilder {
    pub fn new() -> Self {
        Self {
            store: ImageStore::new(),
        }
    }

    pub async fn build(&self, opts: BuildOptions) -> Result<ImageRecord> {
        let instructions = DockerfileParser::parse_file(&opts.dockerfile_path)?;
        if instructions.is_empty() {
            return Err(anyhow!("Empty Dockerfile"));
        }

        println!("Building image from {:?}", opts.dockerfile_path);

        let temp_dir = tempfile::tempdir()?;
        let build_rootfs = temp_dir.path().join("rootfs");
        fs::create_dir_all(&build_rootfs)?;

        let mut config = ExecutionConfig::default();
        let mut step_count = 1;
        let total_steps = instructions.len();

        for inst in &instructions {
            println!("Step {}/{}: {:?}", step_count, total_steps, inst);
            step_count += 1;

            match inst {
                Instruction::From { image, .. } => {
                    // Pull base image if not present
                    let base_record = match self.store.find(image) {
                        Some(rec) => rec,
                        None => {
                            println!("Pulling base image '{}'...", image);
                            crate::pull_image(image).await?
                        }
                    };

                    // Copy base rootfs to build rootfs
                    copy_dir_all(&PathBuf::from(&base_record.rootfs_path), &build_rootfs)?;
                    if let Some(base_cfg) = base_record.config.config {
                        config = base_cfg;
                    }
                }
                Instruction::Workdir(dir) => {
                    let dest = if dir.starts_with('/') {
                        build_rootfs.join(dir.trim_start_matches('/'))
                    } else {
                        let cur = config.working_dir.clone().unwrap_or_else(|| "/".to_string());
                        build_rootfs.join(cur.trim_start_matches('/')).join(dir)
                    };
                    fs::create_dir_all(&dest)?;
                    config.working_dir = Some(dir.clone());
                }
                Instruction::Env { key, value } => {
                    let env_entry = format!("{}={}", key, value);
                    let mut current_env = config.env.take().unwrap_or_default();
                    current_env.retain(|e| e.split('=').next() != Some(key.as_str()));
                    current_env.push(env_entry);
                    config.env = Some(current_env);
                }
                Instruction::Copy { src, dest } | Instruction::Add { src, dest } => {
                    let target_dir = if dest.starts_with('/') {
                        build_rootfs.join(dest.trim_start_matches('/'))
                    } else {
                        let cur = config.working_dir.clone().unwrap_or_else(|| "/".to_string());
                        build_rootfs.join(cur.trim_start_matches('/')).join(dest)
                    };

                    for s in src {
                        let source_path = opts.context_dir.join(s);
                        if !source_path.exists() {
                            return Err(anyhow!("Source file not found in build context: {:?}", source_path));
                        }
                        if source_path.is_dir() {
                            copy_dir_all(&source_path, &target_dir)?;
                        } else {
                            if let Some(parent) = target_dir.parent() {
                                fs::create_dir_all(parent)?;
                            }
                            if target_dir.is_dir() || dest.ends_with('/') {
                                fs::create_dir_all(&target_dir)?;
                                let file_name = source_path.file_name().unwrap();
                                fs::copy(&source_path, target_dir.join(file_name))?;
                            } else {
                                fs::copy(&source_path, &target_dir)?;
                            }
                        }
                    }
                }
                Instruction::Run(cmd) => {
                    // Create minimal bundle to execute RUN command inside build rootfs
                    let step_bundle = temp_dir.path().join(format!("bundle-step-{}", step_count));
                    fs::create_dir_all(&step_bundle)?;
                    // Link build_rootfs as bundle rootfs
                    let bundle_rootfs = step_bundle.join("rootfs");
                    copy_dir_all(&build_rootfs, &bundle_rootfs)?;

                    let run_args = vec!["/bin/sh".to_string(), "-c".to_string(), cmd.clone()];
                    let spec = Spec::new_default(Some(&config), Some(&run_args), None);
                    spec.save_to_bundle(&step_bundle)?;

                    let code = execute_bundle(&step_bundle, &spec, &[], &[], false)?;
                    if code != 0 {
                        return Err(anyhow!("The command '{}' returned a non-zero code: {}", cmd, code));
                    }

                    // Propagate modified rootfs back
                    let _ = fs::remove_dir_all(&build_rootfs);
                    fs::create_dir_all(&build_rootfs)?;
                    copy_dir_all(&bundle_rootfs, &build_rootfs)?;
                }
                Instruction::Cmd(args) => {
                    config.cmd = Some(args.clone());
                }
                Instruction::Entrypoint(args) => {
                    config.entrypoint = Some(args.clone());
                }
                Instruction::Expose(_port) => {}
                Instruction::Label { key, value } => {
                    let mut labels = config.labels.take().unwrap_or_default();
                    labels.insert(key.clone(), value.clone());
                    config.labels = Some(labels);
                }
            }
        }

        // Package into local image store
        let home = boxr_home();
        let random_id = hex::encode(crate::storage::container_store::rand_id());
        let image_id = format!("sha256:{}", random_id);
        let safe_id = image_id.replace(':', "_");

        let dest_image_dir = home.join("images").join(&safe_id);
        let dest_rootfs = dest_image_dir.join("rootfs");
        fs::create_dir_all(&dest_rootfs)?;
        copy_dir_all(&build_rootfs, &dest_rootfs)?;

        let full_tag = opts.tag.unwrap_or_else(|| format!("boxr-build:{}", &random_id[..8]));
        let (repo, tag) = if let Some((r, t)) = full_tag.split_once(':') {
            (r.to_string(), t.to_string())
        } else {
            (full_tag.clone(), "latest".to_string())
        };

        let record = ImageRecord {
            id: random_id[..12].to_string(),
            reference: repo,
            tag,
            manifest_digest: image_id.clone(),
            config_digest: image_id.clone(),
            size_bytes: 1024 * 1024,
            created_at: Utc::now(),
            rootfs_path: dest_rootfs.to_string_lossy().to_string(),
            config: ImageConfig {
                architecture: std::env::consts::ARCH.to_string(),
                os: "linux".to_string(),
                config: Some(config),
                rootfs: None,
            },
        };

        self.store.add(record.clone())?;
        println!("Successfully built image {} ({}:{})", &record.id, record.reference, record.tag);
        Ok(record)
    }
}

fn parse_words(s: &str) -> Vec<String> {
    s.split_whitespace().map(|w| w.trim_matches('"').to_string()).collect()
}

fn parse_array_or_words(s: &str) -> Vec<String> {
    if s.starts_with('[') && s.ends_with(']') {
        if let Ok(vec) = serde_json::from_str::<Vec<String>>(s) {
            return vec;
        }
    }
    parse_words(s)
}

fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());

        if ty.is_dir() {
            copy_dir_all(&from, &to)?;
        } else if ty.is_symlink() {
            #[cfg(unix)]
            {
                if let Ok(target) = fs::read_link(&from) {
                    let _ = std::os::unix::fs::symlink(target, &to);
                }
            }
        } else {
            let _ = fs::copy(&from, &to);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dockerfile_parser() {
        let dockerfile = r#"
# Sample Dockerfile
FROM alpine:3.19
WORKDIR /app
ENV PORT=8080
COPY . /app
RUN echo "building..."
EXPOSE 8080
CMD ["/bin/sh", "-c", "echo hello"]
"#;

        let instructions = DockerfileParser::parse_str(dockerfile).unwrap();
        assert_eq!(instructions.len(), 7);

        assert_eq!(instructions[0], Instruction::From { image: "alpine:3.19".to_string(), as_stage: None });
        assert_eq!(instructions[1], Instruction::Workdir("/app".to_string()));
        assert_eq!(instructions[2], Instruction::Env { key: "PORT".to_string(), value: "8080".to_string() });
        assert_eq!(instructions[3], Instruction::Copy { src: vec![".".to_string()], dest: "/app".to_string() });
        assert_eq!(instructions[4], Instruction::Run("echo \"building...\"".to_string()));
        assert_eq!(instructions[5], Instruction::Expose(8080));
        assert_eq!(instructions[6], Instruction::Cmd(vec!["/bin/sh".to_string(), "-c".to_string(), "echo hello".to_string()]));
    }
}
