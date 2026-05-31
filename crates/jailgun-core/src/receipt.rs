use std::{
    fs::{self, File},
    io::{self, Read},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ReceiptError {
    #[error("receipt I/O error at {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("receipt serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReceiptRecord {
    pub run_id: String,
    pub tab_id: Option<u16>,
    pub artifact_path: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub recorded_at: String,
}

pub fn sha256_file(path: impl AsRef<Path>) -> Result<String, ReceiptError> {
    let path = path.as_ref();
    let mut file = File::open(path).map_err(|source| ReceiptError::Io {
        path: path.display().to_string(),
        source,
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|source| ReceiptError::Io {
            path: path.display().to_string(),
            source,
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn write_json_receipt(
    path: impl AsRef<Path>,
    receipt: &ReceiptRecord,
) -> Result<PathBuf, ReceiptError> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| ReceiptError::Io {
            path: parent.display().to_string(),
            source,
        })?;
    }
    let bytes = serde_json::to_vec_pretty(receipt)?;
    fs::write(path, bytes).map_err(|source| ReceiptError::Io {
        path: path.display().to_string(),
        source,
    })?;
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_and_writes_receipt() {
        let temp = tempfile::tempdir().expect("tempdir");
        let artifact = temp.path().join("archive.tar.gz");
        fs::write(&artifact, b"archive bytes").expect("write artifact");

        let hash = sha256_file(&artifact).expect("hash");
        let receipt = ReceiptRecord {
            run_id: "run".into(),
            tab_id: Some(1),
            artifact_path: artifact.display().to_string(),
            sha256: hash.clone(),
            size_bytes: 13,
            recorded_at: "2026-01-01T00:00:00Z".into(),
        };
        let receipt_path =
            write_json_receipt(temp.path().join("receipts/one.json"), &receipt).expect("receipt");
        let text = fs::read_to_string(receipt_path).expect("read receipt");
        assert!(text.contains(&hash));
    }
}
