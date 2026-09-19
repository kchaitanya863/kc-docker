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
    pub fn new(
        event_type: &str,
        action: &str,
        actor_id: &str,
        actor_name: &str,
        attributes: HashMap<String, String>,
    ) -> Self {
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

        crate::guardrails::LogRotator::rotate_events_if_needed();

        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)
        {
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

        let match_filter = |event: &ContainerEvent, filt: &str| -> bool {
            if let Some((k, v)) = filt.split_once('=') {
                match k.trim() {
                    "event" | "action" => event.action == v.trim(),
                    "type" => event.event_type == v.trim(),
                    "container" | "image" => event.actor_name == v.trim() || event.actor_id.starts_with(v.trim()),
                    _ => event.attributes.get(k.trim()).map(|val| val == v.trim()).unwrap_or(false),
                }
            } else {
                event.event_type.contains(filt)
                    || event.action.contains(filt)
                    || event.actor_name.contains(filt)
            }
        };

        let file = fs::File::open(&file_path)?;
        let mut reader = BufReader::new(file);
        let mut line = String::new();

        let is_test = cfg!(test);
        let mut empty_ticks = 0;

        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => {
                    empty_ticks += 1;
                    if is_test && empty_ticks > 1 {
                        break;
                    }
                    // EOF reached, wait for new events if streaming
                    std::thread::sleep(std::time::Duration::from_millis(200));
                }
                Ok(_) => {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        if let Ok(event) = serde_json::from_str::<ContainerEvent>(trimmed) {
                            if let Some(filt) = filter {
                                if !match_filter(&event, filt) {
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
                Err(_) => break,
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

        let event =
            ContainerEvent::new("container", "start", "1234567890ab", "my-container", attrs);
        let line = event.display_line();

        assert!(line.contains("container start 1234567890ab"));
        assert!(line.contains("image=alpine:latest"));
    }

    #[test]
    fn test_event_filter_key_value_matching() {
        let mut attrs = HashMap::new();
        attrs.insert("image".to_string(), "alpine:latest".to_string());
        let event = ContainerEvent::new("container", "create", "c123", "test-box", attrs);

        let match_filter = |event: &ContainerEvent, filt: &str| -> bool {
            if let Some((k, v)) = filt.split_once('=') {
                match k.trim() {
                    "event" | "action" => event.action == v.trim(),
                    "type" => event.event_type == v.trim(),
                    "container" | "image" => event.actor_name == v.trim() || event.actor_id.starts_with(v.trim()),
                    _ => event.attributes.get(k.trim()).map(|val| val == v.trim()).unwrap_or(false),
                }
            } else {
                event.event_type.contains(filt)
                    || event.action.contains(filt)
                    || event.actor_name.contains(filt)
            }
        };

        assert!(match_filter(&event, "event=create"));
        assert!(match_filter(&event, "type=container"));
        assert!(match_filter(&event, "container=test-box"));
        assert!(!match_filter(&event, "event=die"));
        assert!(!match_filter(&event, "container=other"));
    }
}
