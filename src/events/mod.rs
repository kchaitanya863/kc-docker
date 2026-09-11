use crate::storage::boxr_home;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerEvent {
    pub timestamp: DateTime<Utc>,
    pub event_type: String, // "container", "image", "volume", "network"
    pub action: String,     // "create", "start", "die", "stop", "destroy"
    pub actor_id: String,
    pub actor_name: String,
    pub attributes: HashMap<String, String>,
}

impl ContainerEvent {
    pub fn new(event_type: &str, action: &str, actor_id: &str, actor_name: &str, attributes: HashMap<String, String>) -> Self {
        Self {
            timestamp: Utc::now(),
            event_type: event_type.to_string(),
            action: action.to_string(),
            actor_id: actor_id.to_string(),
            actor_name: actor_name.to_string(),
            attributes,
        }
    }

    pub fn display_line(&self) -> String {
        let mut attrs_str = Vec::new();
        for (k, v) in &self.attributes {
            attrs_str.push(format!("{}={}", k, v));
        }
        let attrs_formatted = if attrs_str.is_empty() {
            "".to_string()
        } else {
            format!(" ({})", attrs_str.join(", "))
        };

        format!(
            "{} {} {} {}{}",
            self.timestamp.to_rfc3339(),
            self.event_type,
            self.action,
            &self.actor_id[..12.min(self.actor_id.len())],
            attrs_formatted
        )
    }
}

pub struct EventManager;

impl EventManager {
    fn events_file() -> PathBuf {
        boxr_home().join("events.jsonl")
    }

    /// Record a lifecycle event to ~/.boxr/events.jsonl
    pub fn record(event: ContainerEvent) {
        let file_path = Self::events_file();
        if let Some(parent) = file_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&file_path) {
            if let Ok(line) = serde_json::to_string(&event) {
                let _ = writeln!(file, "{}", line);
            }
        }
    }

    /// Stream or read recorded events
    pub fn stream_events(since: Option<&str>, filter: Option<&str>) -> Result<()> {
        let file_path = Self::events_file();
        if !file_path.exists() {
            return Ok(());
        }

        let file = fs::File::open(&file_path)?;
        let reader = BufReader::new(file);

        for line_res in reader.lines() {
            if let Ok(line) = line_res {
                if let Ok(event) = serde_json::from_str::<ContainerEvent>(&line) {
                    if let Some(filt) = filter {
                        if !event.event_type.contains(filt) && !event.action.contains(filt) && !event.actor_name.contains(filt) {
                            continue;
                        }
                    }
                    if let Some(s) = since {
                        if let Ok(since_dt) = DateTime::parse_from_rfc3339(s) {
                            if event.timestamp < since_dt {
                                continue;
                            }
                        }
                    }
                    println!("{}", event.display_line());
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_formatting() {
        let mut attrs = HashMap::new();
        attrs.insert("image".to_string(), "alpine:latest".to_string());
        attrs.insert("name".to_string(), "my-container".to_string());

        let event = ContainerEvent::new("container", "start", "1234567890ab", "my-container", attrs);
        let line = event.display_line();

        assert!(line.contains("container start 1234567890ab"));
        assert!(line.contains("image=alpine:latest"));
    }
}
