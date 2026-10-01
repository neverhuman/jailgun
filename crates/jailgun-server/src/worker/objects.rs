use super::*;

impl WorkerService {
    pub async fn objects(&self) -> WorkerObjects {
        WorkerObjects {
            objects: self
                .inner
                .state
                .read()
                .await
                .objects
                .values()
                .cloned()
                .collect(),
        }
    }

    pub async fn put_object(&self, request: ObjectPut) -> Result<ObjectPutResult> {
        validate_object_request(&request)?;
        let bytes = BASE64.decode(&request.data_base64).map_err(|_| {
            action(
                "invalid-base64",
                "Object chunks must use standard base64.",
                "Encode the original tar.gz bytes without line wrapping.",
            )
        })?;
        if bytes.len() > MAX_CHUNK_BYTES {
            return Err(action(
                "chunk-too-large",
                format!("Decoded chunks may not exceed {MAX_CHUNK_BYTES} bytes."),
                "Send a smaller chunk and continue at next_offset.",
            ));
        }
        let upload_id = request
            .upload_id
            .clone()
            .unwrap_or_else(|| format!("upload-{}", uuid::Uuid::new_v4().simple()));
        validate_id(&upload_id, "upload")?;
        let path = self
            .inner
            .root
            .join("uploads")
            .join(format!("{upload_id}.part"));
        {
            let mut uploads = self.inner.uploads.lock().await;
            let upload = uploads
                .entry(upload_id.clone())
                .or_insert_with(|| UploadState {
                    name: request.name.clone(),
                    path: path.clone(),
                    total_bytes: request.total_bytes,
                    sha256: request.sha256.to_lowercase(),
                });
            if upload.name != request.name
                || upload.total_bytes != request.total_bytes
                || upload.sha256 != request.sha256.to_lowercase()
            {
                return Err(action(
                    "upload-metadata-conflict",
                    "Upload metadata changed between chunks.",
                    "Restart with a new upload_id and consistent metadata.",
                ));
            }
            let current = upload.path.metadata().map(|m| m.len()).unwrap_or(0);
            if current != request.offset {
                return Err(action(
                    "upload-offset-conflict",
                    format!("Expected offset {current}, received {}.", request.offset),
                    "Retry using the returned next_offset.",
                ));
            }
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&upload.path)?;
            file.write_all(&bytes)?;
            file.sync_data()?;
        }
        let next_offset = request.offset + bytes.len() as u64;
        if !request.final_chunk {
            return Ok(ObjectPutResult {
                upload_id,
                next_offset,
                committed: false,
                object: None,
            });
        }
        if next_offset != request.total_bytes {
            return Err(action(
                "upload-size-mismatch",
                format!(
                    "Expected {} bytes, received {next_offset}.",
                    request.total_bytes
                ),
                "Resume the upload or restart it with the correct total_bytes.",
            ));
        }
        let digest = sha256_file(&path)?;
        if digest != request.sha256.to_lowercase() {
            return Err(action(
                "upload-digest-mismatch",
                "The completed upload does not match sha256.",
                "Discard the upload and resend the original tar.gz bytes.",
            ));
        }
        jailgun_core::validate_tar_gz(&path, false).map_err(|error| {
            action(
                "unsafe-archive",
                error.to_string(),
                "Upload a non-empty tar.gz with safe relative paths and no .git entries.",
            )
        })?;
        let object_id = format!("object-{}", uuid::Uuid::new_v4().simple());
        let destination = self.object_path(&object_id);
        std::fs::rename(&path, &destination)?;
        let object = WorkerObject {
            object_id: object_id.clone(),
            name: request.name,
            kind: "input".into(),
            media_type: "application/gzip".into(),
            sha256: digest,
            size_bytes: next_offset,
            created_ms: now_ms(),
        };
        persist_object(&self.inner.root, &object)?;
        self.inner
            .state
            .write()
            .await
            .objects
            .insert(object_id, object.clone());
        self.inner.uploads.lock().await.remove(&upload_id);
        Ok(ObjectPutResult {
            upload_id,
            next_offset,
            committed: true,
            object: Some(object),
        })
    }

    pub async fn get_object(&self, request: ObjectGet) -> Result<ObjectChunk> {
        if request.limit == 0 || request.limit as usize > MAX_CHUNK_BYTES {
            return Err(action(
                "invalid-chunk-limit",
                format!("limit must be between 1 and {MAX_CHUNK_BYTES}."),
                "Use a bounded chunk size and follow next_offset.",
            ));
        }
        let object = self.object(&request.object_id).await?;
        if request.offset > object.size_bytes {
            return Err(action(
                "invalid-object-offset",
                "offset is beyond the end of the object.",
                "Begin at zero or use the previous next_offset.",
            ));
        }
        let mut file = File::open(self.object_path(&object.object_id))?;
        use std::io::{Seek, SeekFrom};
        file.seek(SeekFrom::Start(request.offset))?;
        let remaining = object.size_bytes - request.offset;
        let count = remaining.min(u64::from(request.limit)) as usize;
        let mut bytes = vec![0; count];
        file.read_exact(&mut bytes)?;
        let end = request.offset + count as u64;
        Ok(ObjectChunk {
            object,
            offset: request.offset,
            next_offset: (end < remaining + request.offset).then_some(end),
            eof: end == remaining + request.offset,
            data_base64: BASE64.encode(bytes),
        })
    }

    pub(super) async fn object(&self, object_id: &str) -> Result<WorkerObject> {
        self.inner
            .state
            .read()
            .await
            .objects
            .get(object_id)
            .cloned()
            .ok_or_else(|| not_found("object"))
    }

    pub(super) fn object_path(&self, object_id: &str) -> PathBuf {
        self.inner
            .root
            .join("objects")
            .join(format!("{object_id}.tar.gz"))
    }
}
