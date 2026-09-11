use crate::health::HealthConfig;
use crate::oci::image::{ExecutionConfig, ImageConfig};
use crate::oci::runtime::Spec;
use crate::runtime::execute_bundle;
use crate::storage::{boxr_home, ImageRecord, ImageStore};
use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Instruction {
    From { image: String, as_stage: Option<String> },
    Run(String),
    Copy { from_stage: Option<String>, src: Vec<String>, dest: String },
    Add { src: Vec<String>, dest: String },
    Workdir(String),
    Env { key: String, value: String },
    Cmd(Vec<String>),
    Entrypoint(Vec<String>),
    Expose(u16),
    Label { key: String, value: String },
    Healthcheck(HealthConfig),
}

pub struct DockerIgnore {
    patterns: Vec<String>,
}

impl DockerIgnore {
    pub fn load_from_context(context_dir: &Path) -> Self {
        let ignore_file = context_dir.join(".dockerignore");
        if let Ok(content) = fs::read_to_string(&ignore_file) {
            let patterns: Vec<String> = content
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .collect();
            Self { patterns }
        } else {
            Self { patterns: Vec::new() }
        }
    }

    pub fn is_ignored(&self, rel_path: &Path) -> bool {
        let path_str = rel_path.to_string_lossy();
        let mut ignored = false;
        for p in &self.patterns {
            if let Some(exception) = p.strip_prefix('!') {
                if pattern_matches(exception, &path_str) {
                    ignored = false;
                }
            } else if pattern_matches(p, &path_str) {
                ignored = true;
            }
        }
        ignored
    }
}

fn pattern_matches(pattern: &str, path: &str) -> bool {
    let p = pattern.trim_end_matches('/');
    if let Some(suffix) = p.strip_prefix("*.") {
        path.ends_with(&format!(".{}", suffix))
    } else if let Some(suffix) = p.strip_prefix('*') {
        path.ends_with(suffix)
    } else {
        path == p || path.starts_with(&format!("{}/", p)) || path.ends_with(&format!("/{}", p))
    }
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
                let mut from_stage = None;
                let mut filtered_parts = Vec::new();

                for p in parts {
                    if let Some(stage) = p.strip_prefix("--from=") {
                        from_stage = Some(stage.to_string());
                    } else {
                        filtered_parts.push(p);
                    }
                }

                if filtered_parts.len() < 2 {
                    return Err(anyhow!("COPY requires at least one source and one destination"));
                }
                let dest = filtered_parts.last().unwrap().clone();
                let src = filtered_parts[..filtered_parts.len() - 1].to_vec();
                Ok(Instruction::Copy { from_stage, src, dest })
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
            "HEALTHCHECK" => {
                let mut cmd_str = rest;
                if let Some(cmd_idx) = rest.find("CMD") {
                    cmd_str = rest[cmd_idx + 3..].trim();
                }
                let test = parse_array_or_words(cmd_str);
                Ok(Instruction::Healthcheck(HealthConfig {
                    test,
                    interval_secs: 30,
                    timeout_secs: 30,
                    start_period_secs: 0,
                    retries: 3,
                }))
            }
            other => Err(anyhow!("Unsupported Dockerfile instruction: {}", other)),
        }
    }
}

pub struct BuildCache;

impl BuildCache {
    fn cache_dir() -> PathBuf {
        boxr_home().join("buildcache")
    }

    pub fn get(key: &str) -> Option<PathBuf> {
        let dir = Self::cache_dir().join(key).join("rootfs");
        if dir.exists() {
            Some(dir)
        } else {
            None
        }
    }

    pub fn put(key: &str, rootfs: &Path) -> Result<()> {
        let dir = Self::cache_dir().join(key).join("rootfs");
        fs::create_dir_all(&dir)?;
        copy_dir_all(rootfs, &dir)?;
        Ok(())
    }

    pub fn prune() -> Result<usize> {
        let cache = Self::cache_dir();
        if cache.exists() {
            let mut count = 0;
            for entry in fs::read_dir(&cache)? {
                let path = entry?.path();
                if path.is_dir() {
                    let _ = fs::remove_dir_all(&path);
                    count += 1;
                }
            }
            Ok(count)
        } else {
            Ok(0)
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct BuildStage {
    name: Option<String>,
    rootfs: PathBuf,
    config: ExecutionConfig,
}

pub struct BuildOptions {
    pub context_dir: PathBuf,
    pub dockerfile_path: PathBuf,
    pub tag: Option<String>,
    pub no_cache: bool,
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

        let dockerignore = DockerIgnore::load_from_context(&opts.context_dir);
        println!("Building image from {:?}", opts.dockerfile_path);

        let temp_dir = tempfile::tempdir()?;
        let mut stages: Vec<BuildStage> = Vec::new();

        let mut current_rootfs = temp_dir.path().join("stage-0");
        fs::create_dir_all(&current_rootfs)?;
        let mut current_config = ExecutionConfig::default();
        let mut current_stage_name: Option<String> = None;

        let mut step_count = 1;
        let total_steps = instructions.len();
        let mut cache_key = String::from("initial");

        for inst in &instructions {
            println!("Step {}/{}: {:?}", step_count, total_steps, inst);
            step_count += 1;

            match inst {
                Instruction::From { image, as_stage } => {
                    // If we already had a running stage, save it before starting new one
                    if !stages.is_empty() || current_rootfs.exists() {
                        stages.push(BuildStage {
                            name: current_stage_name.take(),
                            rootfs: current_rootfs.clone(),
                            config: current_config.clone(),
                        });
                    }

                    current_stage_name = as_stage.clone();
                    current_rootfs = temp_dir.path().join(format!("stage-{}", stages.len()));
                    fs::create_dir_all(&current_rootfs)?;

                    let base_record = match self.store.find(image) {
                        Some(rec) => rec,
                        None => {
                            println!("Pulling base image '{}'...", image);
                            crate::pull_image(image).await?
                        }
                    };

                    copy_dir_all(&PathBuf::from(&base_record.rootfs_path), &current_rootfs)?;
                    if let Some(base_cfg) = base_record.config.config {
                        current_config = base_cfg;
                    }
                    cache_key = format!("from_{}_{}", image, base_record.manifest_digest);
                }
                Instruction::Workdir(dir) => {
                    let dest = if dir.starts_with('/') {
                        current_rootfs.join(dir.trim_start_matches('/'))
                    } else {
                        let cur = current_config.working_dir.clone().unwrap_or_else(|| "/".to_string());
                        current_rootfs.join(cur.trim_start_matches('/')).join(dir)
                    };
                    fs::create_dir_all(&dest)?;
                    current_config.working_dir = Some(dir.clone());
                    cache_key = format!("{}_workdir_{}", cache_key, dir);
                }
                Instruction::Env { key, value } => {
                    let env_entry = format!("{}={}", key, value);
                    let mut current_env = current_config.env.take().unwrap_or_default();
                    current_env.retain(|e| e.split('=').next() != Some(key.as_str()));
                    current_env.push(env_entry);
                    current_config.env = Some(current_env);
                    cache_key = format!("{}_env_{}_{}", cache_key, key, value);
                }
                Instruction::Add { src, dest } => {
                    let target_dir = if dest.starts_with('/') {
                        current_rootfs.join(dest.trim_start_matches('/'))
                    } else {
                        let cur = current_config.working_dir.clone().unwrap_or_else(|| "/".to_string());
                        current_rootfs.join(cur.trim_start_matches('/')).join(dest)
                    };

                    for s in src {
                        let source_path = opts.context_dir.join(s);
                        if !source_path.exists() {
                            return Err(anyhow!("Source file not found: {:?}", source_path));
                        }
                        if let Ok(rel) = source_path.strip_prefix(&opts.context_dir) {
                            if dockerignore.is_ignored(rel) {
                                continue;
                            }
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
                    cache_key = format!("{}_add_{}_{:?}", cache_key, dest, src);
                }
                Instruction::Copy { from_stage, src, dest } => {
                    let target_dir = if dest.starts_with('/') {
                        current_rootfs.join(dest.trim_start_matches('/'))
                    } else {
                        let cur = current_config.working_dir.clone().unwrap_or_else(|| "/".to_string());
                        current_rootfs.join(cur.trim_start_matches('/')).join(dest)
                    };

                    let source_root: PathBuf = if let Some(from_s) = from_stage {
                        // Find matching stage by name or index
                        let found_stage = stages.iter().find(|s| {
                            s.name.as_deref() == Some(from_s.as_str())
                        }).or_else(|| {
                            if let Ok(idx) = from_s.parse::<usize>() {
                                stages.get(idx)
                            } else {
                                None
                            }
                        });

                        match found_stage {
                            Some(st) => st.rootfs.clone(),
                            None => return Err(anyhow!("Stage '{}' not found for COPY --from", from_s)),
                        }
                    } else {
                        opts.context_dir.clone()
                    };

                    for s in src {
                        let source_path = if from_stage.is_some() && s.starts_with('/') {
                            source_root.join(s.trim_start_matches('/'))
                        } else {
                            source_root.join(s)
                        };

                        if !source_path.exists() {
                            return Err(anyhow!("Source file not found: {:?}", source_path));
                        }

                        // Check .dockerignore for context copies
                        if from_stage.is_none() {
                            if let Ok(rel) = source_path.strip_prefix(&opts.context_dir) {
                                if dockerignore.is_ignored(rel) {
                                    continue;
                                }
                            }
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
                    cache_key = format!("{}_copy_{}_{:?}", cache_key, dest, src);
                }
                Instruction::Run(cmd) => {
                    let mut hasher = Sha256::new();
                    hasher.update(cache_key.as_bytes());
                    hasher.update(cmd.as_bytes());
                    let step_cache_key = hex::encode(hasher.finalize());

                    if !opts.no_cache {
                        if let Some(cached_rootfs) = BuildCache::get(&step_cache_key) {
                            println!(" ---> Using cache");
                            let _ = fs::remove_dir_all(&current_rootfs);
                            fs::create_dir_all(&current_rootfs)?;
                            copy_dir_all(&cached_rootfs, &current_rootfs)?;
                            cache_key = step_cache_key;
                            continue;
                        }
                    }

                    // Execute RUN step
                    let step_bundle = temp_dir.path().join(format!("bundle-step-{}", step_count));
                    fs::create_dir_all(&step_bundle)?;
                    let bundle_rootfs = step_bundle.join("rootfs");
                    copy_dir_all(&current_rootfs, &bundle_rootfs)?;

                    let run_args = vec!["/bin/sh".to_string(), "-c".to_string(), cmd.clone()];
                    let spec = Spec::new_default(Some(&current_config), Some(&run_args), None);
                    spec.save_to_bundle(&step_bundle)?;

                    let code = execute_bundle(&step_bundle, &spec, &[], &[], false)?;
                    if code != 0 {
                        return Err(anyhow!("The command '{}' returned a non-zero code: {}", cmd, code));
                    }

                    let _ = fs::remove_dir_all(&current_rootfs);
                    fs::create_dir_all(&current_rootfs)?;
                    copy_dir_all(&bundle_rootfs, &current_rootfs)?;

                    // Save to build cache
                    let _ = BuildCache::put(&step_cache_key, &current_rootfs);
                    cache_key = step_cache_key;
                }
                Instruction::Cmd(args) => {
                    current_config.cmd = Some(args.clone());
                }
                Instruction::Entrypoint(args) => {
                    current_config.entrypoint = Some(args.clone());
                }
                Instruction::Expose(_port) => {}
                Instruction::Label { key, value } => {
                    let mut labels = current_config.labels.take().unwrap_or_default();
                    labels.insert(key.clone(), value.clone());
                    current_config.labels = Some(labels);
                }
                Instruction::Healthcheck(_hc) => {}
            }
        }

        // Package final image
        let home = boxr_home();
        let random_id = hex::encode(crate::storage::container_store::rand_id());
        let image_id = format!("sha256:{}", random_id);
        let safe_id = image_id.replace(':', "_");

        let dest_image_dir = home.join("images").join(&safe_id);
        let dest_rootfs = dest_image_dir.join("rootfs");
        fs::create_dir_all(&dest_rootfs)?;
        copy_dir_all(&current_rootfs, &dest_rootfs)?;

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
                config: Some(current_config),
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

        assert_eq!(instructions[0], Instruction::From {
            image: "golang:1.22".to_string(),
            as_stage: Some("builder".to_string()),
        });

        assert_eq!(instructions[6], Instruction::Copy {
            from_stage: Some("builder".to_string()),
            src: vec!["/app/server".to_string()],
            dest: ".".to_string(),
        });
    }
}

