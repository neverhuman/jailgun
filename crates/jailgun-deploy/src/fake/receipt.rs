use std::path::PathBuf;

use async_trait::async_trait;

use crate::deploy::{DeployError, DeployReceipt, JsonReceiptWriter};

pub struct FakeReceiptWriter {
    pub root: PathBuf,
}

impl FakeReceiptWriter {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

#[async_trait]
impl JsonReceiptWriter for FakeReceiptWriter {
    async fn write_receipt(&mut self, receipt: &DeployReceipt) -> Result<PathBuf, DeployError> {
        let dir = self.root.join(&receipt.run_id);
        tokio::fs::create_dir_all(&dir).await?;
        let path = dir.join(format!(
            "{}-tab-{:02}-deploy.json",
            receipt.run_id, receipt.tab_id
        ));
        let bytes = serde_json::to_vec_pretty(receipt)?;
        tokio::fs::write(&path, bytes).await?;
        Ok(path)
    }
}
