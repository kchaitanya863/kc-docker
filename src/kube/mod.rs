use crate::cli::RunArgs;
use crate::pod::PodStore;
use crate::storage::ContainerStore;
use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubePodYaml {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    pub metadata: KubeMetadata,
    pub spec: KubePodSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubeMetadata {
    pub name: String,
    #[serde(default)]
    pub labels: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubePodSpec {
    pub containers: Vec<KubeContainerSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubeContainerSpec {
    pub name: String,
    pub image: String,
    #[serde(default)]
    pub command: Option<Vec<String>>,
    #[serde(default)]
    pub ports: Option<Vec<KubeContainerPort>>,
    #[serde(default)]
    pub env: Option<Vec<KubeEnvVar>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubeContainerPort {
    #[serde(rename = "containerPort")]
    pub container_port: u16,
    #[serde(rename = "hostPort")]
    pub host_port: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubeEnvVar {
    pub name: String,
    pub value: String,
}

pub struct KubeManager;

impl KubeManager {
    /// Play a Kubernetes Pod YAML file (boxr play kube)
    pub async fn play_kube(yaml_path: &Path) -> Result<()> {
        let content = fs::read_to_string(yaml_path)
            .with_context(|| format!("Failed to read Kubernetes YAML at {:?}", yaml_path))?;

        let pod_yaml: KubePodYaml =
            serde_yaml::from_str(&content).context("Failed to parse Kubernetes Pod YAML")?;

        if pod_yaml.kind != "Pod" {
            return Err(anyhow!(
                "Unsupported Kubernetes kind '{}', expected 'Pod'",
                pod_yaml.kind
            ));
        }

        println!("Playing Kubernetes Pod '{}'...", pod_yaml.metadata.name);

        let pod_store = PodStore::new();
        let pod = pod_store.create(Some(&pod_yaml.metadata.name), Vec::new())?;

        for c_spec in &pod_yaml.spec.containers {
            let container_name = format!("{}-{}", pod.name, c_spec.name);
            let mut ports_vec = Vec::new();

            if let Some(ports) = &c_spec.ports {
                for p in ports {
                    let host_p = p.host_port.unwrap_or(p.container_port);
                    ports_vec.push(format!("{}:{}", host_p, p.container_port));
                }
            }

            let mut env_vec = Vec::new();
            if let Some(envs) = &c_spec.env {
                for e in envs {
                    env_vec.push(format!("{}={}", e.name, e.value));
                }
            }

            println!("Creating pod container: {}", container_name);

            let run_args = RunArgs {
                interactive: false,
                tty: false,
                detach: true,
                rm: false,
                name: Some(container_name.clone()),
                env: env_vec,
                ports: ports_vec,
                volumes: Vec::new(),
                memory: None,
                labels: Vec::new(),
                dns: Vec::new(),
                cidfile: None,
                cpus: None,
                pids_limit: None,
                rootless: true,
                restart: "no".to_string(),
                health_cmd: None,
                platform: None,
                privileged: false,
                network: "auto".to_string(),
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
                workdir: None,
                image: c_spec.image.clone(),
                command: c_spec.command.clone().unwrap_or_default(),
            };

            let _ = crate::run_container(run_args).await?;
            let _ = pod_store.add_container_to_pod(&pod.name, &container_name);
        }

        println!(
            "Pod '{}' started successfully with {} container(s)",
            pod.name,
            pod_yaml.spec.containers.len()
        );
        Ok(())
    }

    /// Generate a Kubernetes Pod YAML for an existing container or pod (boxr generate kube)
    pub fn generate_kube(target: &str) -> Result<String> {
        let c_store = ContainerStore::new();
        let p_store = PodStore::new();

        let (pod_name, containers) = if let Some(pod) = p_store.find(target) {
            let mut conts = Vec::new();
            for cid in &pod.containers {
                if let Some(c) = c_store.find(cid) {
                    conts.push(c);
                }
            }
            (pod.name, conts)
        } else if let Some(c) = c_store.find(target) {
            (c.name.clone(), vec![c])
        } else {
            return Err(anyhow!("No such container or pod: '{}'", target));
        };

        let mut kube_containers = Vec::new();
        for c in containers {
            let mut ports = Vec::new();
            for p in &c.ports {
                ports.push(KubeContainerPort {
                    container_port: p.container_port,
                    host_port: Some(p.host_port),
                });
            }

            kube_containers.push(KubeContainerSpec {
                name: c.name.clone(),
                image: c.image.clone(),
                command: if c.command.is_empty() {
                    None
                } else {
                    Some(c.command.clone())
                },
                ports: if ports.is_empty() { None } else { Some(ports) },
                env: None,
            });
        }

        let pod_yaml = KubePodYaml {
            api_version: "v1".to_string(),
            kind: "Pod".to_string(),
            metadata: KubeMetadata {
                name: pod_name,
                labels: HashMap::new(),
            },
            spec: KubePodSpec {
                containers: kube_containers,
            },
        };

        let yaml_str = serde_yaml::to_string(&pod_yaml)?;
        Ok(yaml_str)
    }

    /// Run a command in a new user namespace (boxr unshare)
    pub fn unshare_command(command: &[String]) -> Result<i32> {
        let default_cmd = vec!["/bin/sh".to_string()];
        let cmd = if command.is_empty() {
            &default_cmd
        } else {
            command
        };

        #[cfg(target_os = "linux")]
        {
            crate::runtime::linux::run_unshare_cli(cmd)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let mut child = std::process::Command::new(&cmd[0]);
            if cmd.len() > 1 {
                child.args(&cmd[1..]);
            }
            let status = child.status()?;
            Ok(status.code().unwrap_or(0))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_and_generate_kube_pod_yaml() {
        let yaml = r#"
apiVersion: v1
kind: Pod
metadata:
  name: test-pod
spec:
  containers:
    - name: web
      image: nginx:latest
      ports:
        - containerPort: 80
          hostPort: 8080
"#;

        let parsed: KubePodYaml = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(parsed.kind, "Pod");
        assert_eq!(parsed.metadata.name, "test-pod");
        assert_eq!(parsed.spec.containers.len(), 1);
        assert_eq!(parsed.spec.containers[0].name, "web");

        let generated = serde_yaml::to_string(&parsed).unwrap();
        assert!(generated.contains("kind: Pod"));
        assert!(generated.contains("name: test-pod"));
    }
}
