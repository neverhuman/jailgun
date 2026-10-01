//! NDJSON stdout reader for the chrome-bridge child.

use tokio::{
    io::{AsyncBufReadExt, AsyncRead, BufReader},
    sync::mpsc,
};

use super::protocol::{decode_envelope, Envelope, ProtocolError, MAX_LINE_BYTES};

const BRIDGE_STDOUT_QUEUE: usize = 128;

pub fn start_stdout_reader<R: AsyncRead + Unpin + Send + 'static>(
    stdout: R,
) -> mpsc::Receiver<Result<Envelope<serde_json::Value>, ProtocolError>> {
    let (tx, rx) = mpsc::channel(BRIDGE_STDOUT_QUEUE);
    tokio::spawn(async move {
        let mut reader = BufReader::new(stdout);
        loop {
            match next_frame(&mut reader).await {
                Ok(Some(line)) => {
                    let result = decode_envelope(&line);
                    if tx.send(result).await.is_err() {
                        break;
                    }
                }
                Ok(None) => break,
                Err(error) => {
                    let _ = tx.send(Err(error)).await;
                    break;
                }
            }
        }
    });
    rx
}

// Bound bytes before allocating a complete line: a peer may never send LF or EOF.
async fn next_frame<R: tokio::io::AsyncBufRead + Unpin>(
    reader: &mut R,
) -> Result<Option<String>, ProtocolError> {
    let mut frame = Vec::new();
    loop {
        let chunk = reader
            .fill_buf()
            .await
            .map_err(|error| ProtocolError::Decode(serde_json::Error::io(error)))?;
        if chunk.is_empty() {
            if frame.is_empty() {
                return Ok(None);
            }
            break;
        }
        let newline = chunk.iter().position(|byte| *byte == b'\n');
        let part = &chunk[..newline.unwrap_or(chunk.len())];
        let size = frame.len() + part.len();
        let trailing_cr = part.last().or_else(|| frame.last()) == Some(&b'\r');
        if size > MAX_LINE_BYTES + usize::from(trailing_cr) {
            return Err(ProtocolError::LineTooLong {
                got: size,
                max: MAX_LINE_BYTES,
            });
        }
        frame.extend_from_slice(part);
        let consumed = part.len() + usize::from(newline.is_some());
        reader.consume(consumed);
        if newline.is_some() {
            break;
        }
    }
    if frame.last() == Some(&b'\r') {
        frame.pop();
    }
    String::from_utf8(frame).map(Some).map_err(|_| {
        ProtocolError::Decode(serde_json::Error::io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "bridge frame is not UTF-8",
        )))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn rejects_oversized_output_before_newline_or_eof() {
        let (read, mut write) = tokio::io::duplex(8192);
        let mut results = start_stdout_reader(read);
        let writer = tokio::spawn(async move {
            let _ = write.write_all(&vec![b'x'; MAX_LINE_BYTES + 1]).await;
            std::future::pending::<()>().await;
            drop(write);
        });
        let result = tokio::time::timeout(std::time::Duration::from_secs(1), results.recv()).await;
        writer.abort();
        assert!(
            matches!(result, Ok(Some(Err(ProtocolError::LineTooLong { .. })))),
            "a producer can hold an oversized frame open indefinitely"
        );
    }

    #[tokio::test]
    async fn accepts_exact_limit_crlf_and_unterminated_final_frame() {
        let mut bytes = vec![b'x'; MAX_LINE_BYTES];
        bytes.extend_from_slice(b"\r\nlast");
        let mut reader = BufReader::with_capacity(17, bytes.as_slice());
        assert_eq!(
            next_frame(&mut reader).await.unwrap().unwrap().len(),
            MAX_LINE_BYTES
        );
        assert_eq!(
            next_frame(&mut reader).await.unwrap().as_deref(),
            Some("last")
        );
        assert!(next_frame(&mut reader).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn decodes_split_utf8_and_rejects_invalid_bytes() {
        let mut reader = BufReader::with_capacity(1, "café\n".as_bytes());
        assert_eq!(
            next_frame(&mut reader).await.unwrap().as_deref(),
            Some("café")
        );
        let mut reader = BufReader::new(&b"\xff\n"[..]);
        assert!(matches!(
            next_frame(&mut reader).await,
            Err(ProtocolError::Decode(_))
        ));
    }
}
