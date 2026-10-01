use super::*;

#[tokio::test]
async fn bounded_unicode_chunks_are_lossless_and_reverify_integrity() {
    let (dir, store) = fixture(10).await;
    let _run = store
        .submit(request(5, "chunks"), ModelSelection::Current, 0)
        .await
        .unwrap();
    let lease = store.claim(0, 0).await.unwrap().unwrap();
    accept(&store, &lease, 0).await;
    let mut response = capture(&lease, 5);
    let original = "a😀é\n".repeat(12000);
    response.markdown = original.clone();
    let run = store.capture(lease, response, 2).await.unwrap();
    let artifact = run
        .artifacts
        .iter()
        .find(|a| a.name.ends_with(".md"))
        .unwrap();
    let request = ArtifactRead {
        run_id: run.id.clone(),
        artifact_id: artifact.id.clone(),
        offset: 0,
        limit: 16384,
    };
    let mut offset = 0;
    let mut joined = String::new();
    loop {
        let chunk = store
            .artifact_chunk(ArtifactRead {
                offset,
                ..request.clone()
            })
            .await
            .unwrap();
        assert_eq!(chunk.offset, offset);
        assert!(chunk.text.len() <= 16384);
        assert_eq!(chunk.artifact.sha256, artifact.sha256);
        joined.push_str(&chunk.text);
        match chunk.next_offset {
            Some(next) => {
                assert!(!chunk.eof);
                assert!(next > offset);
                offset = next;
            }
            None => {
                assert!(chunk.eof);
                break;
            }
        }
    }
    assert_eq!(joined, original);
    assert_eq!(crate::database::hash(joined.as_bytes()), artifact.sha256);
    let eof = store
        .artifact_chunk(ArtifactRead {
            offset: original.len() as u64,
            ..request.clone()
        })
        .await
        .unwrap();
    assert!(eof.eof && eof.text.is_empty());
    for invalid in [
        ArtifactRead {
            offset: 2,
            ..request.clone()
        },
        ArtifactRead {
            offset: u64::MAX,
            ..request.clone()
        },
        ArtifactRead {
            limit: 3,
            ..request.clone()
        },
        ArtifactRead {
            limit: 16385,
            ..request.clone()
        },
    ] {
        assert_eq!(
            store.artifact_chunk(invalid).await.unwrap_err().code(),
            "invalid-artifact-range"
        );
    }
    assert_eq!(
        store
            .artifact_chunk(ArtifactRead {
                artifact_id: "../../operator-token".into(),
                ..request.clone()
            })
            .await
            .unwrap_err()
            .code(),
        "not-found"
    );
    fs::write(
        dir.path().join("runtime/artifacts").join(&artifact.id),
        "modified",
    )
    .unwrap();
    assert_eq!(
        store.artifact_chunk(request).await.unwrap_err().code(),
        "artifact-integrity"
    );
    store.close().await.unwrap();
}
