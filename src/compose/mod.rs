use crate::builder::{BuildOptions, ImageBuilder};
use crate::cli::RunArgs;
use crate::network::NetworkStore;
use crate::storage::{ContainerRecord, ContainerStore};
use crate::volume::VolumeStore;
use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposeFile {
    #[serde(default)]
    pub version: Option<String>,
    pub services: HashMap<String, ServiceConfig>,
    #[serde(default)]
    pub volumes: HashMap<String, Option<ComposeVolumeConfig>>,
    #[serde(default)]
    pub networks: HashMap<String, Option<ComposeNetworkConfig>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ComposeVolumeConfig {
    pub driver: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ComposeNetworkConfig {
    pub driver: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServiceConfig {
    pub image: Option<String>,
    pub build: Option<String>,
    pub container_name: Option<String>,
    pub env_file: Option<CommandOrList>,
    pub command: Option<CommandOrList>,
    pub entrypoint: Option<CommandOrList>,
    pub environment: Option<EnvironmentConfig>,
    pub ports: Option<Vec<String>>,
    pub volumes: Option<Vec<String>>,
    pub depends_on: Option<Vec<String>>,
    pub networks: Option<Vec<String>>,
    pub restart: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CommandOrList {
    String(String),
    List(Vec<String>),
}

impl CommandOrList {
    pub fn to_vec(&self) -> Vec<String> {
        match self {
            CommandOrList::String(s) => s.split_whitespace().map(|w| w.to_string()).collect(),
            CommandOrList::List(l) => l.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EnvironmentConfig {
    List(Vec<String>),
    Map(HashMap<String, String>),
}

impl EnvironmentConfig {
    pub fn to_vec(&self) -> Vec<String> {
        match self {
            EnvironmentConfig::List(l) => l.clone(),
            EnvironmentConfig::Map(m) => m.iter().map(|(k, v)| format!("{}={}", k, v)).collect(),
        }
    }
}

pub struct ComposeProject {
    pub name: String,
    pub compose_file_path: PathBuf,
    pub compose: ComposeFile,
}

impl ComposeProject {
    pub fn from_str(content: &str, project_name: &str) -> Result<Self> {
        let compose: ComposeFile =
            serde_yaml::from_str(content).context("Failed to parse YAML compose file")?;

        Ok(Self {
            name: project_name.to_string(),
            compose_file_path: PathBuf::from("docker-compose.yml"),
            compose,
        })
    }

    pub fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read compose file at {:?}", path))?;

        let compose: ComposeFile =
            serde_yaml::from_str(&content).context("Failed to parse YAML compose file")?;

        let project_name = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("compose")
            .to_string();

        Ok(Self {
            name: project_name,
            compose_file_path: path.to_path_buf(),
            compose,
        })
    }

    /// Compute topological ordering of services based on depends_on
    pub fn dependency_order(&self) -> Result<Vec<String>> {
        let mut order = Vec::new();
        let mut visited = HashSet::new();
        let mut visiting = HashSet::new();

        for service in self.compose.services.keys() {
            if !visited.contains(service) {
                self.visit_service(service, &mut visited, &mut visiting, &mut order)?;
            }
        }

        Ok(order)
    }

    fn visit_service(
        &self,
        service: &str,
        visited: &mut HashSet<String>,
        visiting: &mut HashSet<String>,
        order: &mut Vec<String>,
    ) -> Result<()> {
        if visiting.contains(service) {
            return Err(anyhow!(
                "Cyclic dependency detected involving service '{}'",
                service
            ));
        }

        if !visited.contains(service) {
            visiting.insert(service.to_string());

            if let Some(cfg) = self.compose.services.get(service) {
                if let Some(deps) = &cfg.depends_on {
                    for dep in deps {
                        if !self.compose.services.contains_key(dep) {
                            return Err(anyhow!(
                                "Service '{}' depends on undefined service '{}'",
                                service,
                                dep
                            ));
                        }
                        self.visit_service(dep, visited, visiting, order)?;
                    }
                }
            }

            visiting.remove(service);
            visited.insert(service.to_string());
            order.push(service.to_string());
        }

        Ok(())
    }

    pub async fn up(&self, detach: bool, build: bool) -> Result<()> {
        let order = self.dependency_order()?;
        println!(
            "Starting compose project '{}' (service order: {:?})",
            self.name, order
        );

        // Ensure default project network
        let net_store = NetworkStore::new();
        let project_net_name = format!("{}_default", self.name);
        if net_store.find(&project_net_name).is_none() {
            let _ = net_store.create(&project_net_name, None, None);
        }

        // Ensure project volumes
        let vol_store = VolumeStore::new();
        for vol_name in self.compose.volumes.keys() {
            let scoped_vol = format!("{}_{}", self.name, vol_name);
            if vol_store.find(&scoped_vol).is_none() {
                let _ = vol_store.create(Some(&scoped_vol), None);
            }
        }

        // Launch services
        let root_dir = self.compose_file_path.parent().unwrap_or(Path::new("."));

        for svc_name in order {
            let svc = self.compose.services.get(&svc_name).unwrap();
            let container_name = svc
                .container_name
                .clone()
                .unwrap_or_else(|| format!("{}_{}_1", self.name, svc_name));

            // Determine image
            let image_name = if let Some(build_path_str) = &svc.build {
                if build || svc.image.is_none() {
                    let build_path = root_dir.join(build_path_str);
                    let builder = ImageBuilder::new();
                    let built_tag = format!("{}_{}:latest", self.name, svc_name);
                    let record = builder
                        .build(BuildOptions {
                            context_dir: build_path.clone(),
                            dockerfile_path: build_path.join("Dockerfile"),
                            tag: Some(built_tag.clone()),
                            no_cache: false,
                            build_args: std::collections::HashMap::new(),
                            target: None,
                            add_host: Vec::new(),
                            memory: None,
                            shm_size: None,
                            quiet: false,
                        })
                        .await?;
                    record.reference
                } else {
                    svc.image.clone().unwrap()
                }
            } else if let Some(img) = &svc.image {
                img.clone()
            } else {
                return Err(anyhow!(
                    "Service '{}' must specify either image or build",
                    svc_name
                ));
            };

            let mut env_vec = Vec::new();

            // 1. Base values from env_file
            if let Some(ef) = &svc.env_file {
                for path_str in ef.to_vec() {
                    let p = root_dir.join(&path_str);
                    if let Ok(content) = fs::read_to_string(&p) {
                        for line in content.lines() {
                            let trimmed = line.trim();
                            if !trimmed.is_empty() && !trimmed.starts_with('#') {
                                env_vec.push(trimmed.to_string());
                            }
                        }
                    }
                }
            }

            // 2. Explicit inline environment declarations override env_file values
            if let Some(env_entries) = &svc.environment {
                for e in env_entries.to_vec() {
                    if let Some((k, _)) = e.split_once('=') {
                        env_vec.retain(|existing| {
                            existing
                                .split_once('=')
                                .map(|(ek, _)| ek != k)
                                .unwrap_or(true)
                        });
                    }
                    env_vec.push(e);
                }
            }

            let cmd_vec = svc.command.as_ref().map(|c| c.to_vec()).unwrap_or_default();
            let port_vec = svc.ports.clone().unwrap_or_default();
            let mut vol_vec = Vec::new();

            if let Some(vols) = &svc.volumes {
                for v in vols {
                    // Scope named volumes to project name
                    if let Some((src, dest)) = v.split_once(':') {
                        if !src.starts_with('/') && !src.starts_with('.') && !src.starts_with('~') {
                            vol_vec.push(format!("{}_{}:{}", self.name, src, dest));
                        } else {
                            vol_vec.push(v.clone());
                        }
                    } else {
                        vol_vec.push(v.clone());
                    }
                }
            }

            println!(
                "Creating and starting service '{}' ({})",
                svc_name, container_name
            );

            // Attach to network
            let _ = net_store.connect_container(&project_net_name, &container_name, &svc_name);

            let run_args = RunArgs {
                interactive: false,
                tty: false,
                detach,
                rm: false,
                name: Some(container_name),
                env: env_vec,
                ports: port_vec,
                volumes: vol_vec,
                memory: None,
                labels: Vec::new(),
                dns: Vec::new(),
                cidfile: None,
                cpus: None,
                pids_limit: None,
                rootless: true,
                restart: svc.restart.clone().unwrap_or_else(|| "no".to_string()),
                health_cmd: None,
                platform: None,
                privileged: false,
                network: "bridge".to_string(),
                disable_content_trust: false,
                gpus: None,
                entrypoint: None,
                env_file: None,
                user: None,
                hostname: None,
                add_host: Vec::new(),
                shm_size: None,
                cap_add: Vec::new(),
                cap_drop: Vec::new(),
                read_only: false,
                init: false,
                tmpfs: Vec::new(),
                devices: Vec::new(),
                security_opt: Vec::new(),
                cpu_shares: None,
                cpuset_cpus: None,
                memory_swap: None,
                memory_reservation: None,
                dns_search: Vec::new(),
                dns_option: Vec::new(),
                expose: Vec::new(),
                sysctl: Vec::new(),
                stop_timeout: None,
                stop_signal: None,
                annotations: Vec::new(),
                ulimits: Vec::new(),
                ipc: None,
                pid: None,
                uts: None,
                userns: None,
                cgroupns: None,
                cgroup_parent: None,
                isolation: None,
                cpu_count: None,
                cpu_percent: None,
                io_maxbandwidth: None,
                io_maxiops: None,
                publish_all: false,
                ip: None,
                ip6: None,
                mac_address: None,
                link: Vec::new(),
                network_alias: Vec::new(),
                mount: Vec::new(),
                health_interval: None,
                health_timeout: None,
                health_retries: None,
                health_start_period: None,
                health_start_interval: None,
                no_healthcheck: false,
                attach: Vec::new(),
                pull: None,
                quiet: false,
                log_driver: None,
                log_opt: Vec::new(),
                oom_kill_disable: false,
                oom_score_adj: None,
                group_add: Vec::new(),
                label_file: None,
                umask: None,
                domainname: None,
                detach_keys: None,
                blkio_weight: None,
                blkio_weight_device: Vec::new(),
                cpu_period: None,
                cpu_quota: None,
                cpu_rt_period: None,
                cpu_rt_runtime: None,
                cpuset_mems: None,
                device_cgroup_rule: Vec::new(),
                device_read_bps: Vec::new(),
                device_read_iops: Vec::new(),
                device_write_bps: Vec::new(),
                device_write_iops: Vec::new(),
                link_local_ip: Vec::new(),
                memory_swappiness: None,
                runtime: None,
                sig_proxy: true,
                storage_opt: Vec::new(),
                use_api_socket: false,
                volume_driver: None,
                volumes_from: Vec::new(),
                workdir: None,
                pod: None,
                image: image_name,
                command: cmd_vec,
            };

            match crate::run_container(run_args).await {
                Ok(code) if code != 0 => {
                    return Err(anyhow!(
                        "Service '{}' in compose project '{}' exited with error code {}",
                        svc_name,
                        self.name,
                        code
                    ));
                }
                Err(err) => {
                    return Err(anyhow!(
                        "Failed to start service '{}' in compose project '{}': {:?}",
                        svc_name,
                        self.name,
                        err
                    ));
                }
                _ => {}
            }
        }

        println!("Project '{}' started successfully.", self.name);
        Ok(())
    }

    pub fn down(&self, remove_volumes: bool) -> Result<()> {
        println!("Stopping compose project '{}'...", self.name);
        let store = ContainerStore::new();
        let prefix = format!("{}_", self.name);

        let mut custom_names = HashSet::new();
        for (svc_name, svc) in &self.compose.services {
            if let Some(cname) = &svc.container_name {
                custom_names.insert(cname.clone());
            }
            for num in 1..=20 {
                custom_names.insert(format!("{}_{}_{}", self.name, svc_name, num));
            }
        }

        for c in store.list() {
            let matches_project = custom_names.contains(&c.name) || {
                if let Some(rest) = c.name.strip_prefix(&prefix) {
                    if let Some((svc, _num)) = rest.rsplit_once('_') {
                        self.compose.services.contains_key(svc)
                    } else {
                        false
                    }
                } else {
                    false
                }
            };

            if matches_project {
                println!("Stopping container {}", c.name);
                let _ = crate::stop_container(&c.id, None);
                println!("Removing container {}", c.name);
                let _ = crate::remove_container(&c.id, true);
            }
        }

        if remove_volumes {
            let vol_store = VolumeStore::new();
            for vol_name in self.compose.volumes.keys() {
                let scoped_vol = format!("{}_{}", self.name, vol_name);
                let _ = vol_store.remove(&scoped_vol);
            }
        }

        let net_store = NetworkStore::new();
        let project_net_name = format!("{}_default", self.name);
        let _ = net_store.remove(&project_net_name);

        println!("Project '{}' stopped and removed.", self.name);
        Ok(())
    }

    pub async fn build(&self, no_cache: bool, quiet: bool) -> Result<()> {
        let root_dir = self.compose_file_path.parent().unwrap_or(Path::new("."));
        for (svc_name, svc) in &self.compose.services {
            if let Some(build_ctx) = &svc.build {
                let ctx_path = root_dir.join(build_ctx);
                let dockerfile = ctx_path.join("Dockerfile");
                if dockerfile.exists() {
                    let builder = ImageBuilder::new();
                    let tag = svc
                        .image
                        .clone()
                        .unwrap_or_else(|| format!("{}_{}:latest", self.name, svc_name));
                    let _ = builder
                        .build(BuildOptions {
                            context_dir: ctx_path,
                            dockerfile_path: dockerfile,
                            tag: Some(tag),
                            no_cache,
                            build_args: HashMap::new(),
                            target: None,
                            add_host: Vec::new(),
                            memory: None,
                            shm_size: None,
                            quiet,
                        })
                        .await?;
                }
            }
        }
        Ok(())
    }

    pub fn ps(&self) -> Result<Vec<ContainerRecord>> {
        let store = ContainerStore::new();
        let prefix = format!("{}_", self.name);

        let mut custom_names = HashSet::new();
        for svc in self.compose.services.values() {
            if let Some(cname) = &svc.container_name {
                custom_names.insert(cname.clone());
            }
        }

        let containers: Vec<ContainerRecord> = store
            .list()
            .into_iter()
            .filter(|c| c.name.starts_with(&prefix) || custom_names.contains(&c.name))
            .collect();
        Ok(containers)
    }

    pub async fn pull_images(&self) -> Result<()> {
        for (svc_name, svc) in &self.compose.services {
            if let Some(img) = &svc.image {
                println!("Pulling service {} image {}...", svc_name, img);
                let _ = crate::pull_image(img).await?;
            }
        }
        Ok(())
    }

    pub async fn push_images(&self) -> Result<()> {
        for (svc_name, svc) in &self.compose.services {
            if let Some(img) = &svc.image {
                println!("Pushing service {} image {}...", svc_name, img);
                let _ = crate::push_image(img).await?;
            }
        }
        Ok(())
    }

    pub async fn create_containers(&self) -> Result<()> {
        let order = self.dependency_order()?;
        for svc_name in order {
            let svc = self.compose.services.get(&svc_name).unwrap();
            let container_name = svc
                .container_name
                .clone()
                .unwrap_or_else(|| format!("{}_{}_1", self.name, svc_name));
            if let Some(img) = &svc.image {
                let run_args = self.build_service_run_args(&svc_name, svc, &container_name, img, false)?;
                let _ = crate::create_only_container(run_args).await?;
            }
        }
        Ok(())
    }

    pub async fn run_one_off(
        &self,
        service: &str,
        command: Vec<String>,
        rm: bool,
    ) -> Result<()> {
        let svc = self
            .compose
            .services
            .get(service)
            .ok_or_else(|| anyhow!("Service '{}' not found", service))?;
        let container_name = format!("{}_{}_run_{}", self.name, service, hex::encode(crate::storage::container_store::rand_id()));
        let image_name = svc
            .image
            .clone()
            .ok_or_else(|| anyhow!("Service '{}' has no image", service))?;
        let mut run_args = self.build_service_run_args(service, svc, &container_name, &image_name, true)?;
        run_args.rm = rm;
        run_args.detach = false;
        run_args.interactive = true;
        run_args.tty = true;
        if !command.is_empty() {
            run_args.command = command;
        }
        let _ = crate::run_container(run_args).await?;
        Ok(())
    }

    fn build_service_run_args(
        &self,
        _svc_name: &str,
        svc: &ServiceConfig,
        container_name: &str,
        image_name: &str,
        detach: bool,
    ) -> Result<RunArgs> {
        let root_dir = self.compose_file_path.parent().unwrap_or(Path::new("."));
        let mut env_vec = Vec::new();
        if let Some(ef) = &svc.env_file {
            for path_str in ef.to_vec() {
                let p = root_dir.join(&path_str);
                if let Ok(content) = fs::read_to_string(&p) {
                    for line in content.lines() {
                        let trimmed = line.trim();
                        if !trimmed.is_empty() && !trimmed.starts_with('#') {
                            env_vec.push(trimmed.to_string());
                        }
                    }
                }
            }
        }
        if let Some(env_entries) = &svc.environment {
            env_vec.extend(env_entries.to_vec());
        }
        let cmd_vec = svc.command.as_ref().map(|c| c.to_vec()).unwrap_or_default();
        let port_vec = svc.ports.clone().unwrap_or_default();
        let mut vol_vec = Vec::new();
        if let Some(vols) = &svc.volumes {
            for v in vols {
                if let Some((src, dest)) = v.split_once(':') {
                    if !src.starts_with('/') && !src.starts_with('.') && !src.starts_with('~') {
                        vol_vec.push(format!("{}_{}:{}", self.name, src, dest));
                    } else {
                        vol_vec.push(v.clone());
                    }
                } else {
                    vol_vec.push(v.clone());
                }
            }
        }
        Ok(RunArgs {
            interactive: false,
            tty: false,
            detach,
            rm: false,
            name: Some(container_name.to_string()),
            env: env_vec,
            ports: port_vec,
            volumes: vol_vec,
            memory: None,
            labels: Vec::new(),
            dns: Vec::new(),
            cidfile: None,
            cpus: None,
            pids_limit: None,
            rootless: true,
            restart: svc.restart.clone().unwrap_or_else(|| "no".to_string()),
            health_cmd: None,
            platform: None,
            privileged: false,
            network: "bridge".to_string(),
            disable_content_trust: false,
            gpus: None,
            entrypoint: None,
            env_file: None,
            user: None,
            hostname: None,
            add_host: Vec::new(),
            shm_size: None,
            cap_add: Vec::new(),
            cap_drop: Vec::new(),
            read_only: false,
            init: false,
            tmpfs: Vec::new(),
            devices: Vec::new(),
            security_opt: Vec::new(),
            cpu_shares: None,
            cpuset_cpus: None,
            memory_swap: None,
            memory_reservation: None,
            dns_search: Vec::new(),
            dns_option: Vec::new(),
            expose: Vec::new(),
            sysctl: Vec::new(),
            stop_timeout: None,
            stop_signal: None,
            annotations: Vec::new(),
            ulimits: Vec::new(),
            ipc: None,
            pid: None,
            uts: None,
            userns: None,
            cgroupns: None,
            cgroup_parent: None,
            isolation: None,
            cpu_count: None,
            cpu_percent: None,
            io_maxbandwidth: None,
            io_maxiops: None,
            publish_all: false,
            ip: None,
            ip6: None,
            mac_address: None,
            link: Vec::new(),
            network_alias: Vec::new(),
            mount: Vec::new(),
            health_interval: None,
            health_timeout: None,
            health_retries: None,
            health_start_period: None,
            health_start_interval: None,
            no_healthcheck: false,
            attach: Vec::new(),
            pull: None,
            quiet: false,
            log_driver: None,
            log_opt: Vec::new(),
            oom_kill_disable: false,
            oom_score_adj: None,
            group_add: Vec::new(),
            label_file: None,
            umask: None,
            domainname: None,
            detach_keys: None,
            blkio_weight: None,
            blkio_weight_device: Vec::new(),
            cpu_period: None,
            cpu_quota: None,
            cpu_rt_period: None,
            cpu_rt_runtime: None,
            cpuset_mems: None,
            device_cgroup_rule: Vec::new(),
            device_read_bps: Vec::new(),
            device_read_iops: Vec::new(),
            device_write_bps: Vec::new(),
            device_write_iops: Vec::new(),
            link_local_ip: Vec::new(),
            memory_swappiness: None,
            runtime: None,
            sig_proxy: true,
            storage_opt: Vec::new(),
            use_api_socket: false,
            volume_driver: None,
            volumes_from: Vec::new(),
            workdir: None,
            pod: None,
            image: image_name.to_string(),
            command: cmd_vec,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compose_yaml_parse_and_dependency_order() {
        let yaml = r#"
version: '3.8'
services:
  web:
    image: nginx:latest
    ports:
      - "80:80"
    depends_on:
      - api
  api:
    image: node:18
    ports:
      - "3000:3000"
    depends_on:
      - db
  db:
    image: postgres:15
    environment:
      POSTGRES_PASSWORD: secret
    volumes:
      - pgdata:/var/lib/postgresql/data
volumes:
  pgdata:
"#;

        let compose: ComposeFile = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(compose.services.len(), 3);
        assert!(compose.volumes.contains_key("pgdata"));

        let project = ComposeProject {
            name: "myapp".to_string(),
            compose_file_path: PathBuf::from("docker-compose.yml"),
            compose,
        };

        let order = project.dependency_order().unwrap();
        // db must come before api, api must come before web
        let db_pos = order.iter().position(|s| s == "db").unwrap();
        let api_pos = order.iter().position(|s| s == "api").unwrap();
        let web_pos = order.iter().position(|s| s == "web").unwrap();

        assert!(db_pos < api_pos);
        assert!(api_pos < web_pos);
    }

    #[test]
    fn test_compose_environment_overrides_env_file() {
        let temp = tempfile::tempdir().unwrap();
        let env_file_path = temp.path().join(".env.test");
        fs::write(
            &env_file_path,
            "PORT=8000\nDB=staging\nSHARED=env_file_val\n",
        )
        .unwrap();

        let yaml = format!(
            r#"
version: '3.8'
services:
  web:
    image: nginx:latest
    env_file:
      - {}
    environment:
      PORT: "9000"
      SHARED: "inline_val"
"#,
            env_file_path.display()
        );

        let proj = ComposeProject::from_str(&yaml, "env-test").unwrap();
        let svc = &proj.compose.services["web"];

        let mut env_vec = Vec::new();
        if let Some(ef) = &svc.env_file {
            for path_str in ef.to_vec() {
                if let Ok(content) = fs::read_to_string(&path_str) {
                    for line in content.lines() {
                        let trimmed = line.trim();
                        if !trimmed.is_empty() && !trimmed.starts_with('#') {
                            env_vec.push(trimmed.to_string());
                        }
                    }
                }
            }
        }
        if let Some(env_entries) = &svc.environment {
            for e in env_entries.to_vec() {
                if let Some((k, _)) = e.split_once('=') {
                    env_vec.retain(|existing| {
                        existing
                            .split_once('=')
                            .map(|(ek, _)| ek != k)
                            .unwrap_or(true)
                    });
                }
                env_vec.push(e);
            }
        }

        assert!(env_vec.contains(&"PORT=9000".to_string()));
        assert!(env_vec.contains(&"SHARED=inline_val".to_string()));
        assert!(env_vec.contains(&"DB=staging".to_string()));
        assert!(!env_vec.contains(&"PORT=8000".to_string()));
        assert!(!env_vec.contains(&"SHARED=env_file_val".to_string()));
    }

    #[test]
    fn test_compose_down_avoids_prefix_collision() {
        let yaml = r#"
version: '3.8'
services:
  web:
    image: nginx:latest
"#;
        let proj = ComposeProject::from_str(yaml, "app").unwrap();
        let prefix = "app_";
        let other_project_container = "app_backend_web_1";

        let matches_other = other_project_container
            .strip_prefix(prefix)
            .map(|rest| {
                if let Some((svc, _)) = rest.rsplit_once('_') {
                    proj.compose.services.contains_key(svc)
                } else {
                    false
                }
            })
            .unwrap_or(false);

        assert!(
            !matches_other,
            "Compose down for 'app' must not match 'app_backend_web_1'"
        );
    }
}
