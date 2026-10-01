use super::*;

#[tokio::test]
async fn round_robin_grants_survive_restart_and_include_reduction_stages() {
    let (dir, store) = fixture(10).await;
    let first = store
        .submit(request(5, "first-fair"), ModelSelection::Current, 0)
        .await
        .unwrap();
    let second = store
        .submit(request(5, "second-fair"), ModelSelection::Current, 1)
        .await
        .unwrap();
    for n in 0..10 {
        let lease = store.claim(n * 30_000, 0).await.unwrap().unwrap();
        assert_eq!(
            lease.run_id,
            if n % 2 == 0 { &first.id } else { &second.id }.to_string()
        );
        accept(&store, &lease, n * 30_000).await;
        store
            .capture(lease.clone(), capture(&lease, 5), n * 30_000 + 2)
            .await
            .unwrap();
    }
    store.close().await.unwrap();
    let store = Store::open(dir.path().join("runtime")).unwrap();
    for (n, expected) in [(10, first.id), (11, second.id)] {
        let lease = store.claim(n * 30_000, 0).await.unwrap().unwrap();
        assert_eq!(lease.run_id, expected);
        assert_eq!(lease.stage, "compare");
        accept(&store, &lease, n * 30_000).await;
        store
            .capture(lease.clone(), capture(&lease, 5), n * 30_000 + 2)
            .await
            .unwrap();
    }
    store.close().await.unwrap();
}

#[tokio::test]
async fn paused_and_auth_waiting_runs_expire_once_without_releasing_reservations() {
    let (_dir, store) = fixture(10).await;
    let paused = store
        .submit(request(5, "paused-deadline"), ModelSelection::Current, 0)
        .await
        .unwrap();
    let lease = store.claim(0, 0).await.unwrap().unwrap();
    accept(&store, &lease, 0).await;
    store.pause(paused.id.clone(), 2).await.unwrap();
    store
        .account_expired("test-account".into(), 3)
        .await
        .unwrap();
    let waiting = store
        .submit(request(5, "waiting-deadline"), ModelSelection::Current, 4)
        .await
        .unwrap();
    assert_eq!(waiting.status, "waiting-for-auth");
    assert!(store.claim(RUN_DEADLINE_MS + 4, 0).await.unwrap().is_none());
    assert!(store.claim(RUN_DEADLINE_MS + 5, 0).await.unwrap().is_none());
    for id in [paused.id, waiting.id] {
        assert_eq!(
            store.run(id.clone()).await.unwrap().pause_reason.as_deref(),
            Some("run-deadline")
        );
        assert_eq!(
            store
                .resume(id.clone(), false, RUN_DEADLINE_MS + 5)
                .await
                .unwrap_err()
                .code(),
            "run-deadline"
        );
        assert_eq!(
            store
                .events(id, 0)
                .await
                .unwrap()
                .iter()
                .filter(|e| e.data["reason"] == "run-deadline")
                .count(),
            1
        );
    }
    assert_eq!(store.accounts().await.unwrap()[0].active, 1);
    store.close().await.unwrap();
}
