use crate::{
    model::{ArtifactChunk, ArtifactRead},
    Error, Result, Store,
};

impl Store {
    /// Registered artifacts only. Verify the entire immutable artifact on every read.
    /// The interface must reauthorize the run's account before calling this service.
    pub async fn artifact_chunk(&self, request: ArtifactRead) -> Result<ArtifactChunk> {
        let invalid = || {
            Error::action(
                "invalid-artifact-range",
                "Use a UTF-8 byte boundary and a chunk limit between 4 and 16384 bytes.",
                "Start at offset zero, then use the returned next_offset.",
            )
        };
        if !(4..=16384).contains(&request.limit) {
            return Err(invalid());
        }
        let (artifact, bytes) = self.artifact(request.run_id, request.artifact_id).await?;
        let text = std::str::from_utf8(&bytes).map_err(|_| {
            Error::action(
                "artifact-invalid-text",
                "This artifact is not UTF-8 text.",
                "Use the authenticated binary artifact download.",
            )
        })?;
        let start = usize::try_from(request.offset).map_err(|_| invalid())?;
        if start > bytes.len() || !text.is_char_boundary(start) {
            return Err(invalid());
        }
        let mut end = start
            .saturating_add(request.limit as usize)
            .min(bytes.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        Ok(ArtifactChunk {
            artifact,
            offset: request.offset,
            next_offset: (end < bytes.len()).then_some(end as u64),
            eof: end == bytes.len(),
            text: text[start..end].to_string(),
        })
    }
}
