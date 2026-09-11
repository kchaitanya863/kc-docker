use anyhow::{Result, anyhow};
use std::fmt;

/// An image reference parsed into its constituent parts according to OCI conventions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageReference {
    pub registry: String,
    pub repository: String,
    pub tag: String,
    pub digest: Option<String>,
}

impl ImageReference {
    /// Default registry for unqualified images.
    pub const DEFAULT_REGISTRY: &'static str = "registry-1.docker.io";
    /// Default tag if none is specified.
    pub const DEFAULT_TAG: &'static str = "latest";

    /// Parse a string reference such as "hello-world", "alpine:3.19", or "ghcr.io/org/repo:tag".
    pub fn parse(input: &str) -> Result<Self> {
        let input = input.trim();
        if input.is_empty() {
            return Err(anyhow!("Empty image reference"));
        }

        // Check for digest first: name@sha256:xxx
        let (remainder, digest) = if let Some(idx) = input.find('@') {
            let (name, digest_part) = input.split_at(idx);
            (name, Some(digest_part[1..].to_string()))
        } else {
            (input, None)
        };

        // Check for tag: name:tag
        // Note: A port in the registry (e.g., localhost:5000/repo:tag) must not be confused with a tag
        let (name_part, tag) = if let Some(colon_idx) = remainder.rfind(':') {
            // If the colon is before the first slash, it's a port, not a tag
            if let Some(slash_idx) = remainder.find('/') {
                if colon_idx < slash_idx {
                    (remainder, Self::DEFAULT_TAG.to_string())
                } else {
                    (
                        &remainder[..colon_idx],
                        remainder[colon_idx + 1..].to_string(),
                    )
                }
            } else {
                (
                    &remainder[..colon_idx],
                    remainder[colon_idx + 1..].to_string(),
                )
            }
        } else {
            (remainder, Self::DEFAULT_TAG.to_string())
        };

        // Parse registry and repository
        let (registry, repository) = if let Some(slash_idx) = name_part.find('/') {
            let potential_registry = &name_part[..slash_idx];
            if potential_registry.contains('.')
                || potential_registry.contains(':')
                || potential_registry == "localhost"
            {
                (
                    potential_registry.to_string(),
                    name_part[slash_idx + 1..].to_string(),
                )
            } else {
                // Docker Hub official/user repo like "library/hello-world" or "myuser/myimage"
                (Self::DEFAULT_REGISTRY.to_string(), name_part.to_string())
            }
        } else {
            // Official library image on Docker Hub
            (
                Self::DEFAULT_REGISTRY.to_string(),
                format!("library/{}", name_part),
            )
        };

        // If repository still doesn't have a slash and registry is docker.io, prefix with library/
        let repository = if registry == Self::DEFAULT_REGISTRY && !repository.contains('/') {
            format!("library/{}", repository)
        } else {
            repository
        };

        Ok(Self {
            registry,
            repository,
            tag,
            digest,
        })
    }

    /// Full canonical name (e.g. "registry-1.docker.io/library/hello-world:latest")
    #[allow(dead_code)]
    pub fn canonical(&self) -> String {
        format!("{}/{}:{}", self.registry, self.repository, self.tag)
    }

    /// Short human-readable display name (e.g. "hello-world:latest")
    pub fn display_name(&self) -> String {
        if self.registry == Self::DEFAULT_REGISTRY {
            if let Some(stripped) = self.repository.strip_prefix("library/") {
                return format!("{}:{}", stripped, self.tag);
            }
            return format!("{}:{}", self.repository, self.tag);
        }
        format!("{}/{}:{}", self.registry, self.repository, self.tag)
    }
}

impl fmt::Display for ImageReference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple() {
        let r = ImageReference::parse("hello-world").unwrap();
        assert_eq!(r.registry, ImageReference::DEFAULT_REGISTRY);
        assert_eq!(r.repository, "library/hello-world");
        assert_eq!(r.tag, "latest");
    }

    #[test]
    fn test_parse_with_tag() {
        let r = ImageReference::parse("alpine:3.19").unwrap();
        assert_eq!(r.registry, ImageReference::DEFAULT_REGISTRY);
        assert_eq!(r.repository, "library/alpine");
        assert_eq!(r.tag, "3.19");
    }

    #[test]
    fn test_parse_custom_registry() {
        let r = ImageReference::parse("ghcr.io/org/repo:v1.0").unwrap();
        assert_eq!(r.registry, "ghcr.io");
        assert_eq!(r.repository, "org/repo");
        assert_eq!(r.tag, "v1.0");
    }

    #[test]
    fn test_parse_digest() {
        let r = ImageReference::parse("alpine@sha256:abcdef").unwrap();
        assert_eq!(r.digest.as_deref(), Some("sha256:abcdef"));
    }
}
