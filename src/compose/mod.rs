use crate::builder::{BuildOptions, ImageBuilder};
use crate::cli::RunArgs;
use crate::network::NetworkStore;
use crate::storage::{ContainerRecord, ContainerStore};
use crate::volume::VolumeStore;
use anyhow::{anyhow, Context, Result};
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
    pub fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read compose file at {:?}", path))?;

        let compose: ComposeFile = serde_yaml::from_str(&content)
            .context("Failed to parse YAML compose file")?;

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
            return Err(anyhow!("Cyclic dependency detected involving service '{}'", service));
        }

        if !visited.contains(service) {
            visiting.insert(service.to_string());

            if let Some(cfg) = self.compose.services.get(service) {
                if let Some(deps) = &cfg.depends_on {
                    for dep in deps {
                        if !self.compose.services.contains_key(dep) {
                            return Err(anyhow!("Service '{}' depends on undefined service '{}'", service, dep));
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
        println!("Starting compose project '{}' (service order: {:?})", self.name, order);

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
            let container_name = format!("{}_{}_1", self.name, svc_name);

            // Determine image
            let image_name = if let Some(build_path_str) = &svc.build {
                if build || svc.image.is_none() {
                    let build_path = root_dir.join(build_path_str);
                    let builder = ImageBuilder::new();
                    let built_tag = format!("{}_{}:latest", self.name, svc_name);
                    let record = builder.build(BuildOptions {
                        context_dir: build_path.clone(),
                        dockerfile_path: build_path.join("Dockerfile"),
                        tag: Some(built_tag.clone()),
                    }).await?;
                    record.reference
                } else {
                    svc.image.clone().unwrap()
                }
            } else if let Some(img) = &svc.image {
                img.clone()
            } else {
                return Err(anyhow!("Service '{}' must specify either image or build", svc_name));
            };

            let env_vec = svc.environment.as_ref().map(|e| e.to_vec()).unwrap_or_default();
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

            println!("Creating and starting service '{}' ({})", svc_name, container_name);

            // Attach to network
            let _ = net_store.connect_container(&project_net_name, &container_name, &svc_name);

            let run_args = RunArgs {
                interactive: false,
                detach,
                rm: false,
                name: Some(container_name),
                env: env_vec,
                ports: port_vec,
                volumes: vol_vec,
                image: image_name,
                command: cmd_vec,
            };

            let _ = crate::run_container(run_args).await;
        }

        println!("Project '{}' started successfully.", self.name);
        Ok(())
    }

    pub fn down(&self, remove_volumes: bool) -> Result<()> {
        println!("Stopping compose project '{}'...", self.name);
        let store = ContainerStore::new();
        let prefix = format!("{}_", self.name);

        for c in store.list() {
            if c.name.starts_with(&prefix) {
                println!("Removing container {}", c.name);
                let _ = store.remove(&c.id);
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

    pub fn ps(&self) -> Result<Vec<ContainerRecord>> {
        let store = ContainerStore::new();
        let prefix = format!("{}_", self.name);
        let containers: Vec<ContainerRecord> = store
            .list()
            .into_iter()
            .filter(|c| c.name.starts_with(&prefix))
            .collect();
        Ok(containers)
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
}
