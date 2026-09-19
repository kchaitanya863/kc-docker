use crate::oci::image::{Descriptor, ImageConfig, ImageManifest, ManifestListOrIndex, media_types};
use crate::oci::reference::ImageReference;
use anyhow::{Context, Result, anyhow};
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue};
use sha2::{Digest, Sha256};
use std::env;
use std::fs::File;
use std::io::Write;
use std::path::Path;

pub struct RegistryClient {
    client: Client,
    token: Option<String>,
    basic_auth: Option<String>,
}

impl RegistryClient {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .user_agent("boxr/0.1.0")
                .build()
                .unwrap_or_else(|_| Client::new()),
            token: None,
            basic_auth: None,
        }
    }

    /// Authenticate against registry if needed (e.g. Docker Hub auth token or private registry).
    pub async fn authenticate(&mut self, reference: &ImageReference) -> Result<()> {
        let cred_store = crate::auth::CredentialStore::new();
        let creds = cred_store.get_credentials(&reference.registry);
        if let Some((u, p)) = &creds {
            let encoded = crate::auth::custom_base64_encode(&format!("{}:{}", u, p));
            self.basic_auth = Some(encoded);
        }

        let ping_url = format!("https://{}/v2/", reference.registry);
        let mut req = self.client.get(&ping_url);
        if let Some(auth) = &self.basic_auth {
            if let Ok(val) = HeaderValue::from_str(&format!("Basic {}", auth)) {
                req = req.header(AUTHORIZATION, val);
            }
        }
        let resp = req.send().await?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            if let Some(auth_header) = resp.headers().get("www-authenticate") {
                let auth_str = auth_header.to_str()?;
                if let Some(token) = self
                    .fetch_bearer_token(auth_str, reference, creds.as_ref())
                    .await?
                {
                    self.token = Some(token);
                }
            }
        }
        Ok(())
    }

    async fn fetch_bearer_token(
        &self,
        auth_header: &str,
        reference: &ImageReference,
        creds: Option<&(String, String)>,
    ) -> Result<Option<String>> {
        // Example: Bearer realm="https://auth.docker.io/token",service="registry.docker.io",scope="repository:library/hello-world:pull"
        if !auth_header.starts_with("Bearer ") {
            return Ok(None);
        }

        let params_str = &auth_header[7..];
        let mut realm = None;
        let mut service = None;

        for part in params_str.split(',') {
            let part = part.trim();
            if let Some((k, v)) = part.split_once('=') {
                let v = v.trim_matches('"');
                match k {
                    "realm" => realm = Some(v.to_string()),
                    "service" => service = Some(v.to_string()),
                    _ => {}
                }
            }
        }

        let realm = match realm {
            Some(r) => r,
            None => return Ok(None),
        };

        let mut url = format!("{}?scope=repository:{}:pull", realm, reference.repository);
        if let Some(s) = service {
            url.push_str(&format!("&service={}", s));
        }

        let mut req = self.client.get(&url);
        if let Some((u, p)) = creds {
            req = req.basic_auth(u, Some(p));
        }
        let resp = req.send().await?;
        if !resp.status().is_success() {
            return Err(anyhow!(
                "Failed to obtain registry auth token: status {}",
                resp.status()
            ));
        }

        #[derive(serde::Deserialize)]
        struct TokenResponse {
            token: Option<String>,
            access_token: Option<String>,
        }

        let token_data: TokenResponse = resp.json().await?;
        let token = token_data.token.or(token_data.access_token);
        Ok(token)
    }

    fn auth_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Some(token) = &self.token {
            if let Ok(val) = HeaderValue::from_str(&format!("Bearer {}", token)) {
                headers.insert(AUTHORIZATION, val);
            }
        } else if let Some(basic) = &self.basic_auth {
            if let Ok(val) = HeaderValue::from_str(&format!("Basic {}", basic)) {
                headers.insert(AUTHORIZATION, val);
            }
        }
        headers
    }

    /// Fetch manifest for the reference, resolving index / manifest lists to the target platform.
    pub async fn fetch_manifest(
        &mut self,
        reference: &ImageReference,
    ) -> Result<(ImageManifest, String)> {
        self.fetch_manifest_with_platform(reference, None).await
    }

    /// Fetch manifest for the reference with explicit target platform architecture.
    pub async fn fetch_manifest_with_platform(
        &mut self,
        reference: &ImageReference,
        target_platform: Option<&str>,
    ) -> Result<(ImageManifest, String)> {
        self.authenticate(reference).await?;

        let tag_or_digest = reference.digest.as_deref().unwrap_or(&reference.tag);

        let url = format!(
            "https://{}/v2/{}/manifests/{}",
            reference.registry, reference.repository, tag_or_digest
        );

        let mut headers = self.auth_headers();
        let accept_header = [
            media_types::OCI_INDEX_V1,
            media_types::OCI_MANIFEST_V1,
            media_types::DOCKER_MANIFEST_LIST_V2,
            media_types::DOCKER_MANIFEST_V2,
        ]
        .join(", ");
        headers.insert(ACCEPT, HeaderValue::from_str(&accept_header)?);

        let resp = self.client.get(&url).headers(headers).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let err_body = resp.text().await.unwrap_or_default();
            return Err(anyhow!(
                "Failed to fetch manifest from {}: status {} - {}",
                url,
                status,
                err_body
            ));
        }

        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let body_bytes = resp.bytes().await?;

        // Determine if this is an Index / Manifest List
        if content_type.contains("manifest.list")
            || content_type.contains("image.index")
            || serde_json::from_slice::<ManifestListOrIndex>(&body_bytes).is_ok()
        {
            if let Ok(index) = serde_json::from_slice::<ManifestListOrIndex>(&body_bytes) {
                // Determine target architecture
                let (target_os, target_arch) = if let Some(plat) = target_platform {
                    if let Some((os_p, arch_p)) = plat.split_once('/') {
                        (os_p, arch_p)
                    } else {
                        ("linux", plat)
                    }
                } else {
                    let host_arch = match env::consts::ARCH {
                        "aarch64" => "arm64",
                        "x86_64" => "amd64",
                        other => other,
                    };
                    ("linux", host_arch)
                };

                let chosen_descriptor = index
                    .manifests
                    .iter()
                    .find(|desc| {
                        if let Some(p) = &desc.platform {
                            p.os == target_os && p.architecture == target_arch
                        } else {
                            false
                        }
                    })
                    .or_else(|| {
                        // Fallback to linux/amd64 or any linux
                        index.manifests.iter().find(|desc| {
                            if let Some(p) = &desc.platform {
                                p.os == "linux"
                            } else {
                                false
                            }
                        })
                    })
                    .or_else(|| index.manifests.first())
                    .ok_or_else(|| {
                        anyhow!("No suitable manifest found in index for target platform")
                    })?;

                // Recursively fetch platform manifest by digest
                let mut platform_ref = reference.clone();
                platform_ref.digest = Some(chosen_descriptor.digest.clone());
                return Box::pin(self.fetch_manifest_with_platform(&platform_ref, target_platform))
                    .await;
            }
        }

        // Direct image manifest
        let manifest: ImageManifest =
            serde_json::from_slice(&body_bytes).context("Failed to parse image manifest JSON")?;

        let digest = format!("sha256:{}", hex::encode(Sha256::digest(&body_bytes)));
        Ok((manifest, digest))
    }

    /// Fetch image configuration JSON blob
    pub async fn fetch_config(
        &self,
        reference: &ImageReference,
        config_descriptor: &Descriptor,
    ) -> Result<ImageConfig> {
        let url = format!(
            "https://{}/v2/{}/blobs/{}",
            reference.registry, reference.repository, config_descriptor.digest
        );

        let resp = self
            .client
            .get(&url)
            .headers(self.auth_headers())
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(anyhow!(
                "Failed to fetch config blob {}: status {}",
                config_descriptor.digest,
                resp.status()
            ));
        }

        let body_bytes = resp.bytes().await?;
        let config: ImageConfig =
            serde_json::from_slice(&body_bytes).context("Failed to parse image config JSON")?;

        Ok(config)
    }

    /// Download a blob (e.g. layer archive) to a file with progress and SHA-256 verification.
    pub async fn download_blob_to_file(
        &self,
        reference: &ImageReference,
        descriptor: &Descriptor,
        dest_path: &Path,
    ) -> Result<()> {
        if dest_path.exists() {
            // Verify existing cached file matches the expected SHA-256 digest
            if let Ok(mut f) = File::open(dest_path) {
                let mut hasher = Sha256::new();
                if std::io::copy(&mut f, &mut hasher).is_ok() {
                    let calculated = format!("sha256:{}", hex::encode(hasher.finalize()));
                    if calculated == descriptor.digest {
                        return Ok(());
                    }
                }
            }
            // Corrupt or partial cache file; purge and re-download
            let _ = std::fs::remove_file(dest_path);
        }

        let url = format!(
            "https://{}/v2/{}/blobs/{}",
            reference.registry, reference.repository, descriptor.digest
        );

        let resp = self
            .client
            .get(&url)
            .headers(self.auth_headers())
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(anyhow!(
                "Failed to download blob {}: status {}",
                descriptor.digest,
                resp.status()
            ));
        }

        let total_size = resp.content_length().unwrap_or(descriptor.size as u64);
        let short_digest = if descriptor.digest.len() > 19 {
            &descriptor.digest[..19]
        } else {
            &descriptor.digest
        };

        let pb = ProgressBar::new(total_size);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta}) {msg}")
                .unwrap()
                .progress_chars("#>-"),
        );
        pb.set_message(format!("Downloading {}", short_digest));

        let temp_path = dest_path.with_extension("download");
        let mut file = File::create(&temp_path)?;
        let mut hasher = Sha256::new();
        let mut stream = resp.bytes_stream();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            file.write_all(&chunk)?;
            hasher.update(&chunk);
            pb.inc(chunk.len() as u64);
        }

        file.flush()?;
        pb.finish_with_message(format!("Downloaded {}", short_digest));

        // Validate digest
        let calculated_digest = format!("sha256:{}", hex::encode(hasher.finalize()));
        if calculated_digest != descriptor.digest {
            let _ = std::fs::remove_file(&temp_path);
            return Err(anyhow!(
                "Checksum mismatch for blob {}: expected {}, got {}",
                descriptor.digest,
                descriptor.digest,
                calculated_digest
            ));
        }

        // Rename temp to target destination
        std::fs::rename(temp_path, dest_path)?;
        Ok(())
    }
}
