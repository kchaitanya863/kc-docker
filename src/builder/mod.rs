use crate::health::HealthConfig;
use crate::oci::image::{ExecutionConfig, ImageConfig};
use crate::oci::runtime::Spec;
use crate::runtime::execute_bundle;
use crate::storage::{ImageRecord, ImageStore, boxr_home};
use anyhow::{Context, Result, anyhow};
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Instruction {
    From {
        image: String,
        as_stage: Option<String>,
    },
    Run(String),
    Copy {
        from_stage: Option<String>,
        src: Vec<String>,
        dest: String,
    },
    Add {
        src: Vec<String>,
        dest: String,
    },
    Workdir(String),
    Env {
        key: String,
        value: String,
    },
    Cmd(Vec<String>),
    Entrypoint(Vec<String>),
    Expose(u16),
    Label {
        key: String,
        value: String,
    },
    Healthcheck(HealthConfig),
    Arg {
        name: String,
        default: Option<String>,
    },
    User(String),
    Volume(Vec<String>),
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
            Self {
                patterns: Vec::new(),
            }
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
    if p.contains('*') {
        let parts: Vec<&str> = p.split('*').collect();
        if parts.len() == 2 {
            let prefix = parts[0];
            let suffix = parts[1];
            return path.starts_with(prefix)
                && path.ends_with(suffix)
                && path.len() >= prefix.len() + suffix.len();
        }
    }
    path == p || path.starts_with(&format!("{}/", p)) || path.ends_with(&format!("/{}", p))
}

pub fn matches_wildcard(pattern: &str, text: &str) -> bool {
    let p_chars: Vec<char> = pattern.chars().collect();
    let t_chars: Vec<char> = text.chars().collect();
    let mut p = 0;
    let mut t = 0;
    let mut star = None;
    let mut match_idx = 0;

    while t < t_chars.len() {
        if p < p_chars.len() && (p_chars[p] == '?' || p_chars[p] == t_chars[t]) {
            p += 1;
            t += 1;
        } else if p < p_chars.len() && p_chars[p] == '*' {
            star = Some(p);
            match_idx = t;
            p += 1;
        } else if let Some(s) = star {
            p = s + 1;
            match_idx += 1;
            t = match_idx;
        } else {
            return false;
        }
    }
    while p < p_chars.len() && p_chars[p] == '*' {
        p += 1;
    }
    p == p_chars.len()
}

pub fn parse_key_value_pairs(rest: &str) -> Vec<(String, String)> {
    if !rest.contains('=') {
        if let Some((k, v)) = rest.split_once(char::is_whitespace) {
            let key = k.trim().to_string();
            let val = v.trim().trim_matches('"').trim_matches('\'').to_string();
            return vec![(key, val)];
        }
        return Vec::new();
    }

    let mut pairs = Vec::new();
    let chars: Vec<char> = rest.chars().collect();
    let mut i = 0;
    let n = chars.len();

    while i < n {
        while i < n && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= n {
            break;
        }

        let mut key = String::new();
        while i < n && chars[i] != '=' && !chars[i].is_whitespace() {
            key.push(chars[i]);
            i += 1;
        }

        while i < n && chars[i].is_whitespace() {
            i += 1;
        }
        if i < n && chars[i] == '=' {
            i += 1;
        }

        while i < n && chars[i].is_whitespace() {
            i += 1;
        }

        let mut val = String::new();
        if i < n && (chars[i] == '"' || chars[i] == '\'') {
            let quote = chars[i];
            i += 1;
            while i < n && chars[i] != quote {
                if chars[i] == '\\' && i + 1 < n {
                    i += 1;
                }
                val.push(chars[i]);
                i += 1;
            }
            if i < n && chars[i] == quote {
                i += 1;
            }
        } else {
            while i < n && !chars[i].is_whitespace() {
                val.push(chars[i]);
                i += 1;
            }
        }

        if !key.is_empty() {
            pairs.push((key, val));
        }
    }
    pairs
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
                let insts = Self::parse_line(full_line)?;
                instructions.extend(insts);
            }
            current_line.clear();
        }

        Ok(instructions)
    }

    fn parse_line(line: &str) -> Result<Vec<Instruction>> {
        let trimmed = line.trim();
        let (keyword, rest) = match trimmed.split_once(char::is_whitespace) {
            Some((kw, r)) => (kw, r.trim()),
            None => (trimmed, ""),
        };

        let keyword_upper = keyword.to_uppercase();

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
                Ok(vec![Instruction::From { image, as_stage }])
            }
            "RUN" => Ok(vec![Instruction::Run(rest.to_string())]),
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
                    return Err(anyhow!(
                        "COPY requires at least one source and one destination"
                    ));
                }
                let dest = filtered_parts.last().unwrap().clone();
                let src = filtered_parts[..filtered_parts.len() - 1].to_vec();
                Ok(vec![Instruction::Copy {
                    from_stage,
                    src,
                    dest,
                }])
            }
            "ADD" => {
                let parts = parse_words(rest);
                if parts.len() < 2 {
                    return Err(anyhow!(
                        "ADD requires at least one source and one destination"
                    ));
                }
                let dest = parts.last().unwrap().clone();
                let src = parts[..parts.len() - 1].to_vec();
                Ok(vec![Instruction::Add { src, dest }])
            }
            "WORKDIR" => Ok(vec![Instruction::Workdir(rest.to_string())]),
            "ENV" => {
                let pairs = parse_key_value_pairs(rest);
                if pairs.is_empty() {
                    return Err(anyhow!("Invalid ENV format: '{}'", rest));
                }
                Ok(pairs
                    .into_iter()
                    .map(|(key, value)| Instruction::Env { key, value })
                    .collect())
            }
            "CMD" => {
                let args = parse_array_or_words(rest);
                Ok(vec![Instruction::Cmd(args)])
            }
            "ENTRYPOINT" => {
                let args = parse_array_or_words(rest);
                Ok(vec![Instruction::Entrypoint(args)])
            }
            "EXPOSE" => {
                let port: u16 = rest
                    .split('/')
                    .next()
                    .unwrap_or(rest)
                    .trim()
                    .parse()
                    .with_context(|| format!("Invalid port in EXPOSE: {}", rest))?;
                Ok(vec![Instruction::Expose(port)])
            }
            "LABEL" => {
                let pairs = parse_key_value_pairs(rest);
                if pairs.is_empty() {
                    return Err(anyhow!("Invalid LABEL format: '{}'", rest));
                }
                Ok(pairs
                    .into_iter()
                    .map(|(key, value)| Instruction::Label { key, value })
                    .collect())
            }
            "HEALTHCHECK" => {
                let mut cmd_str = rest;
                if let Some(cmd_idx) = rest.find("CMD") {
                    cmd_str = rest[cmd_idx + 3..].trim();
                }
                let test = parse_array_or_words(cmd_str);
                Ok(vec![Instruction::Healthcheck(HealthConfig {
                    test,
                    interval_secs: 30,
                    timeout_secs: 30,
                    start_period_secs: 0,
                    retries: 3,
                })])
            }
            "ARG" => {
                if let Some((k, v)) = rest.split_once('=') {
                    Ok(vec![Instruction::Arg {
                        name: k.trim().to_string(),
                        default: Some(v.trim().trim_matches('"').to_string()),
                    }])
                } else {
                    Ok(vec![Instruction::Arg {
                        name: rest.trim().to_string(),
                        default: None,
                    }])
                }
            }
            "USER" => Ok(vec![Instruction::User(rest.to_string())]),
            "VOLUME" => {
                let vols = parse_array_or_words(rest);
                Ok(vec![Instruction::Volume(vols)])
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
        if dir.exists() { Some(dir) } else { None }
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
    pub build_args: std::collections::HashMap<String, String>,
    pub target: Option<String>,
    pub add_host: Vec<String>,
    pub memory: Option<String>,
    pub shm_size: Option<String>,
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

        let first_non_arg = instructions
            .iter()
            .find(|i| !matches!(i, Instruction::Arg { .. }));
        match first_non_arg {
            Some(Instruction::From { .. }) => {}
            _ => return Err(anyhow!("Dockerfile must begin with FROM instruction")),
        }

        if let Some(target_stage) = &opts.target {
            let target_exists = instructions.iter().any(|inst| {
                if let Instruction::From { as_stage, .. } = inst {
                    as_stage.as_deref() == Some(target_stage.as_str())
                } else {
                    false
                }
            });
            if !target_exists {
                return Err(anyhow!("target stage {} could not be found", target_stage));
            }
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
                    // If target was specified and previous stage matches, stop building
                    if let Some(target_stage) = &opts.target {
                        if current_stage_name.as_deref() == Some(target_stage.as_str()) {
                            break;
                        }
                    }

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
                        let cur = current_config
                            .working_dir
                            .clone()
                            .unwrap_or_else(|| "/".to_string());
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
                    if dest.contains("..") {
                        return Err(anyhow!(
                            "Path traversal rejected in ADD destination: '{}'",
                            dest
                        ));
                    }
                    let target_dir = if dest.starts_with('/') {
                        current_rootfs.join(dest.trim_start_matches('/'))
                    } else {
                        let cur = current_config
                            .working_dir
                            .clone()
                            .unwrap_or_else(|| "/".to_string());
                        current_rootfs.join(cur.trim_start_matches('/')).join(dest)
                    };
                    if let Ok(canon_root) = current_rootfs.canonicalize() {
                        let mut check = target_dir.clone();
                        while let Some(parent) = check.parent() {
                            if parent.exists() {
                                if let Ok(canon_p) = parent.canonicalize() {
                                    if !canon_p.starts_with(&canon_root) {
                                        return Err(anyhow!(
                                            "ADD destination escapes container rootfs: '{}'",
                                            dest
                                        ));
                                    }
                                }
                                break;
                            }
                            check = parent.to_path_buf();
                        }
                    }

                    let mut resolved_sources: Vec<PathBuf> = Vec::new();
                    let mut url_sources: Vec<String> = Vec::new();
                    for s in src {
                        if s.starts_with("http://") || s.starts_with("https://") {
                            url_sources.push(s.clone());
                            continue;
                        }

                        if s.contains("..") || s.starts_with('/') {
                            return Err(anyhow!("Path traversal rejected in ADD source: '{}'", s));
                        }

                        if s.contains('*') || s.contains('?') {
                            let (dir_part, pattern) = if let Some(last_slash) = s.rfind('/') {
                                (&s[..last_slash], &s[last_slash + 1..])
                            } else {
                                ("", s.as_str())
                            };
                            let search_dir = if dir_part.is_empty() {
                                opts.context_dir.clone()
                            } else {
                                opts.context_dir.join(dir_part.trim_start_matches('/'))
                            };

                            if !search_dir.exists() {
                                return Err(anyhow!("Source file not found: {:?}", search_dir));
                            }

                            let mut matches_count = 0;
                            if let Ok(entries) = fs::read_dir(&search_dir) {
                                let mut sorted_entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
                                sorted_entries.sort_by_key(|e| e.file_name());
                                for entry in sorted_entries {
                                    let fname = entry.file_name().to_string_lossy().to_string();
                                    if matches_wildcard(pattern, &fname) {
                                        resolved_sources.push(entry.path());
                                        matches_count += 1;
                                    }
                                }
                            }
                            if matches_count == 0 {
                                return Err(anyhow!("No files matching pattern: '{}'", s));
                            }
                        } else {
                            let source_path = opts.context_dir.join(s);
                            if let Ok(canon_ctx) = opts.context_dir.canonicalize() {
                                if let Ok(canon_src) = source_path.canonicalize() {
                                    if !canon_src.starts_with(&canon_ctx) {
                                        return Err(anyhow!(
                                            "ADD source escapes build context: '{}'",
                                            s
                                        ));
                                    }
                                }
                            }
                            if !source_path.exists() {
                                return Err(anyhow!("Source file not found: {:?}", source_path));
                            }
                            resolved_sources.push(source_path);
                        }
                    }

                    if (resolved_sources.len() + url_sources.len()) > 1 && !dest.ends_with('/') && !target_dir.is_dir() {
                        return Err(anyhow!(
                            "When adding multiple files, destination must end with /: '{}'",
                            dest
                        ));
                    }

                    for s in url_sources {
                        let resp = reqwest::get(&s).await.context("Failed to fetch ADD URL")?;
                        let bytes = resp.bytes().await.context("Failed to read ADD URL body")?;
                        if let Some(parent) = target_dir.parent() {
                            fs::create_dir_all(parent)?;
                        }
                        let dest_file = if target_dir.is_dir() || dest.ends_with('/') {
                            fs::create_dir_all(&target_dir)?;
                            let url_file = s.rsplit('/').next().unwrap_or("download");
                            target_dir.join(url_file)
                        } else {
                            target_dir.clone()
                        };
                        fs::write(&dest_file, &bytes)?;
                    }

                    for source_path in resolved_sources {
                        if let Ok(rel) = source_path.strip_prefix(&opts.context_dir) {
                            if dockerignore.is_ignored(rel) {
                                continue;
                            }
                        }

                        let s_str = source_path.to_string_lossy().to_string();
                        let is_tar = s_str.ends_with(".tar")
                            || s_str.ends_with(".tar.gz")
                            || s_str.ends_with(".tgz")
                            || s_str.ends_with(".tar.bz2")
                            || s_str.ends_with(".tar.xz");

                        if is_tar && source_path.is_file() {
                            fs::create_dir_all(&target_dir)?;
                            let f = fs::File::open(&source_path)?;
                            if s_str.ends_with(".tar.gz") || s_str.ends_with(".tgz") {
                                let gz = flate2::read::GzDecoder::new(f);
                                let mut archive = tar::Archive::new(gz);
                                crate::oci::image::unpack_archive_safely(&mut archive, &target_dir)?;
                            } else {
                                let mut archive = tar::Archive::new(f);
                                crate::oci::image::unpack_archive_safely(&mut archive, &target_dir)?;
                            }
                        } else if source_path.is_dir() {
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
                Instruction::Copy {
                    from_stage,
                    src,
                    dest,
                } => {
                    if dest.contains("..") {
                        return Err(anyhow!(
                            "Path traversal rejected in COPY destination: '{}'",
                            dest
                        ));
                    }
                    let target_dir = if dest.starts_with('/') {
                        current_rootfs.join(dest.trim_start_matches('/'))
                    } else {
                        let cur = current_config
                            .working_dir
                            .clone()
                            .unwrap_or_else(|| "/".to_string());
                        current_rootfs.join(cur.trim_start_matches('/')).join(dest)
                    };
                    if let Ok(canon_root) = current_rootfs.canonicalize() {
                        let mut check = target_dir.clone();
                        while let Some(parent) = check.parent() {
                            if parent.exists() {
                                if let Ok(canon_p) = parent.canonicalize() {
                                    if !canon_p.starts_with(&canon_root) {
                                        return Err(anyhow!(
                                            "COPY destination escapes container rootfs: '{}'",
                                            dest
                                        ));
                                    }
                                }
                                break;
                            }
                            check = parent.to_path_buf();
                        }
                    }

                    let source_root: PathBuf = if let Some(from_s) = from_stage {
                        // Find matching stage by name or index, or fallback to local image in store
                        let found_stage = stages
                            .iter()
                            .find(|s| s.name.as_deref() == Some(from_s.as_str()))
                            .or_else(|| {
                                if let Ok(idx) = from_s.parse::<usize>() {
                                    stages.get(idx)
                                } else {
                                    None
                                }
                            });

                        if let Some(st) = found_stage {
                            st.rootfs.clone()
                        } else if let Some(img_rec) = self.store.find(from_s) {
                            PathBuf::from(&img_rec.rootfs_path)
                        } else {
                            return Err(anyhow!(
                                "Stage or image '{}' not found for COPY --from",
                                from_s
                            ));
                        }
                    } else {
                        opts.context_dir.clone()
                    };

                    let mut resolved_sources: Vec<PathBuf> = Vec::new();
                    for s in src {
                        if s.contains("..") {
                            return Err(anyhow!("Path traversal rejected in COPY source: '{}'", s));
                        }
                        if from_stage.is_none() && s.starts_with('/') {
                            return Err(anyhow!("COPY source cannot be absolute: '{}'", s));
                        }

                        if s.contains('*') || s.contains('?') {
                            let (dir_part, pattern) = if let Some(last_slash) = s.rfind('/') {
                                (&s[..last_slash], &s[last_slash + 1..])
                            } else {
                                ("", s.as_str())
                            };
                            let search_dir = if from_stage.is_some() && dir_part.starts_with('/') {
                                source_root.join(dir_part.trim_start_matches('/'))
                            } else if dir_part.is_empty() {
                                source_root.clone()
                            } else {
                                source_root.join(dir_part)
                            };

                            if !search_dir.exists() {
                                return Err(anyhow!("Source file not found: {:?}", search_dir));
                            }

                            let mut matches_count = 0;
                            if let Ok(entries) = fs::read_dir(&search_dir) {
                                let mut sorted_entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
                                sorted_entries.sort_by_key(|e| e.file_name());
                                for entry in sorted_entries {
                                    let fname = entry.file_name().to_string_lossy().to_string();
                                    if matches_wildcard(pattern, &fname) {
                                        resolved_sources.push(entry.path());
                                        matches_count += 1;
                                    }
                                }
                            }
                            if matches_count == 0 {
                                return Err(anyhow!("No files matching pattern: '{}'", s));
                            }
                        } else {
                            let source_path = if from_stage.is_some() && s.starts_with('/') {
                                source_root.join(s.trim_start_matches('/'))
                            } else {
                                source_root.join(s)
                            };

                            if let Ok(canon_src_root) = source_root.canonicalize() {
                                if let Ok(canon_src) = source_path.canonicalize() {
                                    if !canon_src.starts_with(&canon_src_root) {
                                        return Err(anyhow!(
                                            "COPY source escapes context directory: '{}'",
                                            s
                                        ));
                                    }
                                }
                            }

                            if !source_path.exists() {
                                return Err(anyhow!("Source file not found: {:?}", source_path));
                            }
                            resolved_sources.push(source_path);
                        }
                    }

                    if resolved_sources.len() > 1 && !dest.ends_with('/') && !target_dir.is_dir() {
                        return Err(anyhow!(
                            "When copying multiple files, destination must end with /: '{}'",
                            dest
                        ));
                    }

                    for source_path in resolved_sources {
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
                        return Err(anyhow!(
                            "The command '{}' returned a non-zero code: {}",
                            cmd,
                            code
                        ));
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
                Instruction::Expose(port) => {
                    let mut exposed = current_config.exposed_ports.take().unwrap_or_default();
                    exposed.insert(format!("{}/tcp", port), serde_json::json!({}));
                    current_config.exposed_ports = Some(exposed);
                    cache_key = format!("{}_expose_{}", cache_key, port);
                }
                Instruction::Label { key, value } => {
                    let mut labels = current_config.labels.take().unwrap_or_default();
                    labels.insert(key.clone(), value.clone());
                    current_config.labels = Some(labels);
                }
                Instruction::Healthcheck(hc) => {
                    current_config.healthcheck = Some(hc.clone());
                    cache_key = format!("{}_hc_{:?}", cache_key, hc.test);
                }
                Instruction::Arg { name, default } => {
                    let val = opts.build_args.get(name).cloned().or(default.clone());
                    let val_str = val.clone().unwrap_or_default();
                    if let Some(v) = val {
                        let env_entry = format!("{}={}", name, v);
                        if let Some(envs) = &mut current_config.env {
                            envs.retain(|e| !e.starts_with(&format!("{}=", name)));
                            envs.push(env_entry);
                        } else {
                            current_config.env = Some(vec![env_entry]);
                        }
                    }
                    cache_key = format!("{}_arg_{}_{}", cache_key, name, val_str);
                }
                Instruction::User(user) => {
                    current_config.user = Some(user.clone());
                    cache_key = format!("{}_user_{}", cache_key, user);
                }
                Instruction::Volume(vols) => {
                    let mut vol_map = HashMap::new();
                    for v in vols {
                        vol_map.insert(v.clone(), serde_json::json!({}));
                    }
                    current_config.volumes = Some(vol_map);
                    cache_key = format!("{}_vol_{:?}", cache_key, vols);
                }
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

        let full_tag = opts
            .tag
            .unwrap_or_else(|| format!("boxr-build:{}", &random_id[..8]));
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
                history: Vec::new(),
            },
        };

        self.store.add(record.clone())?;
        println!(
            "Successfully built image {} ({}:{})",
            &record.id, record.reference, record.tag
        );
        Ok(record)
    }
}

fn parse_words(s: &str) -> Vec<String> {
    s.split_whitespace()
        .map(|w| w.trim_matches('"').to_string())
        .collect()
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
