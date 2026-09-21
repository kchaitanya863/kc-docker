//! Podman Specgen (SpecGenerator JSON) generation.

use crate::storage::ContainerStore;
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecGenerator {
    pub name: String,
    pub image: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub command: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub entrypoint: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub env: Vec<String>,
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    pub labels: HashMap<String, String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub portmappings: Vec<crate::network::PortMapping>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work_dir: Option<String>,
    #[serde(default)]
    pub terminal: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restart_policy: Option<String>,
}

pub struct SpecgenManager;

impl SpecgenManager {
    pub fn generate_spec(query: &str) -> Result<String> {
        let store = ContainerStore::new();
        let cont = store
            .find(query)
            .ok_or_else(|| anyhow!("Container '{}' not found", query))?;

        let spec_gen = SpecGenerator {
            name: cont.name.clone(),
            image: cont.image.clone(),
            command: cont.command.clone(),
            entrypoint: Vec::new(),
            env: Vec::new(),
            labels: HashMap::new(),
            portmappings: cont.ports.clone(),
            work_dir: None,
            terminal: false,
            hostname: Some(cont.name.clone()),
            restart_policy: Some(format!("{:?}", cont.restart_policy)),
        };

        let json = serde_json::to_string_pretty(&spec_gen)?;
        Ok(json)
    }
}
