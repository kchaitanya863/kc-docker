use super::container_store::{ContainerRecord, ContainerStatus};
use super::image_store::ImageRecord;
use anyhow::Result;

/// Interface Segregation: Read-only container querying interface
pub trait ContainerReader: Send + Sync {
    fn find(&self, id_or_name: &str) -> Option<ContainerRecord>;
    fn list(&self) -> Vec<ContainerRecord>;
}

/// Interface Segregation: Write-only / mutation container interface
pub trait ContainerWriter: Send + Sync {
    fn add(&self, record: ContainerRecord) -> Result<()>;
    fn update_status(&self, id_or_name: &str, status: ContainerStatus) -> Result<()>;
    fn remove(&self, id_or_name: &str) -> Result<()>;
}

/// Combined container store operations contract (LSP compliant)
pub trait ContainerStoreOps: ContainerReader + ContainerWriter {}
impl<T: ContainerReader + ContainerWriter> ContainerStoreOps for T {}

/// Interface Segregation: Read-only image querying interface
pub trait ImageReader: Send + Sync {
    fn find(&self, reference: &str) -> Option<ImageRecord>;
    fn find_with_platform(&self, reference: &str, platform: Option<&str>) -> Option<ImageRecord>;
    fn list(&self) -> Vec<ImageRecord>;
}

/// Interface Segregation: Image modification interface
pub trait ImageWriter: Send + Sync {
    fn add(&self, record: ImageRecord) -> Result<()>;
    fn remove(&self, reference: &str) -> Result<()>;
}

/// Combined image store operations contract (LSP compliant)
pub trait ImageStoreOps: ImageReader + ImageWriter {}
impl<T: ImageReader + ImageWriter> ImageStoreOps for T {}
