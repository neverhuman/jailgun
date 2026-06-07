use super::tests::{base_request, write_test_config};
use super::*;

#[test]
fn jmcp_run_without_ready_account_fails_closed() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry_env = format!("JAILGUN_TEST_BROWSER_PROFILES_{}", uuid::Uuid::new_v4());
    let registry_path = temp.path().join("browser-profiles.json");
    std::env::set_var(&registry_env, &registry_path);
    let config_path = write_test_config(temp.path(), &registry_env);
    let prompt_file = temp.path().join("prompt.txt");
    std::fs::write(&prompt_file, "prompt").expect("write prompt");
    let mut request = base_request(prompt_file, config_path);
    request.browser.bridge_cmd = vec!["fake-bridge".into()];

    let error = prepare_agent_run(
        request,
        AgentRunPaths {
            events_jsonl: temp.path().join("events.jsonl"),
            summary_json: temp.path().join("summary.json"),
        },
    )
    .expect_err("missing ready account rejected");

    assert!(error.to_string().contains("ready browser account"));
}

#[test]
fn explicit_account_ids_build_profile_pool_and_port_map() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry_env = format!("JAILGUN_TEST_BROWSER_PROFILES_{}", uuid::Uuid::new_v4());
    let registry_path = temp.path().join("browser-profiles.json");
    std::env::set_var(&registry_env, &registry_path);
    let roots = jailgun_core::BrowserAccountRoots {
        profile_root: temp.path().join("profiles"),
        state_root: temp.path().join("state"),
        downloads_root: temp.path().join("downloads"),
    };
    let mut registry = jailgun_core::BrowserProfileRegistry::default();
    let _first = registry
        .upsert_account("a@example.com", Some("acct-a".into()), &roots, 9224, 2)
        .expect("first");
    let _second = registry
        .upsert_account("b@example.com", Some("acct-b".into()), &roots, 9301, 2)
        .expect("second");
    for account in &mut registry.accounts {
        account.status = jailgun_core::BrowserAccountStatus::Ready;
    }
    registry.save(&registry_path).expect("save registry");

    let config_path = write_test_config(temp.path(), &registry_env);
    let prompt_file = temp.path().join("prompt.txt");
    std::fs::write(&prompt_file, "prompt").expect("write prompt");
    let mut request = base_request(prompt_file, config_path);
    request.browser.bridge_cmd = vec!["fake-bridge".into()];
    request.browser.account_ids = vec!["acct-a".into(), "acct-b".into()];

    let prepared = prepare_agent_run(
        request,
        AgentRunPaths {
            events_jsonl: temp.path().join("events.jsonl"),
            summary_json: temp.path().join("summary.json"),
        },
    )
    .expect("prepared");

    let lease = prepared.browser_lease.expect("browser lease prepared");
    assert_eq!(
        lease.request.account_ids,
        vec!["acct-a".to_string(), "acct-b".to_string()]
    );
    assert_eq!(lease.request.tabs, 2);
    assert_eq!(lease.registry_path, registry_path);
    assert_eq!(prepared.opts.profile_pool, Vec::<std::path::PathBuf>::new());
    assert!(!prepared
        .opts
        .bridge_env
        .contains_key("JAILGUN_CHROME_PROFILE_POOL"));
}

#[test]
fn account_ids_override_request_bridge_profile_env() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry_env = format!("JAILGUN_TEST_BROWSER_PROFILES_{}", uuid::Uuid::new_v4());
    let registry_path = temp.path().join("browser-profiles.json");
    std::env::set_var(&registry_env, &registry_path);
    let roots = jailgun_core::BrowserAccountRoots {
        profile_root: temp.path().join("profiles"),
        state_root: temp.path().join("state"),
        downloads_root: temp.path().join("downloads"),
    };
    let mut registry = jailgun_core::BrowserProfileRegistry::default();
    let _account = registry
        .upsert_account("a@example.com", Some("acct-a".into()), &roots, 9224, 2)
        .expect("account");
    registry.accounts[0].status = jailgun_core::BrowserAccountStatus::Ready;
    registry.save(&registry_path).expect("save registry");

    let config_path = write_test_config(temp.path(), &registry_env);
    let prompt_file = temp.path().join("prompt.txt");
    std::fs::write(&prompt_file, "prompt").expect("write prompt");
    let mut request = base_request(prompt_file, config_path);
    request.browser.bridge_cmd = vec!["fake-bridge".into()];
    request.browser.account_ids = vec!["acct-a".into()];
    request.browser.bridge_env.insert(
        "JAILGUN_CHROME_PROFILE_POOL".into(),
        "evil=/tmp/evil".into(),
    );
    request
        .browser
        .bridge_env
        .insert("JAILGUN_CHROME_PROFILE_PORTS".into(), "evil=6666".into());
    request
        .browser
        .bridge_env
        .insert("JAILGUN_CDP_URL".into(), "http://127.0.0.1:6666".into());
    request
        .browser
        .bridge_env
        .insert("JAILGUN_CDP_PORT".into(), "6666".into());
    request.browser.bridge_env.insert(
        "JAILGUN_CHROME_PROFILE_DIR".into(),
        temp.path().join("evil-profile").display().to_string(),
    );

    let prepared = prepare_agent_run(
        request,
        AgentRunPaths {
            events_jsonl: temp.path().join("events.jsonl"),
            summary_json: temp.path().join("summary.json"),
        },
    )
    .expect("prepared");

    assert!(prepared.browser_lease.is_some());
    for key in [
        "JAILGUN_CHROME_PROFILE_POOL",
        "JAILGUN_CHROME_PROFILE_PORTS",
        "JAILGUN_CDP_URL",
        "JAILGUN_CDP_PORT",
        "JAILGUN_CHROME_PROFILE_DIR",
    ] {
        assert!(
            !prepared.opts.bridge_env.contains_key(key),
            "{key} should be scrubbed before lease acquisition"
        );
    }
}

#[test]
fn jmcp_run_rejects_raw_profile_paths() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry_env = format!("JAILGUN_TEST_BROWSER_PROFILES_{}", uuid::Uuid::new_v4());
    let registry_path = temp.path().join("browser-profiles.json");
    std::env::set_var(&registry_env, &registry_path);
    let config_path = write_test_config(temp.path(), &registry_env);
    let prompt_file = temp.path().join("prompt.txt");
    std::fs::write(&prompt_file, "prompt").expect("write prompt");
    let mut request = base_request(prompt_file, config_path);
    request.browser.bridge_cmd = vec!["fake-bridge".into()];
    request.browser.profile_dir = Some(temp.path().join("raw-profile"));

    let error = prepare_agent_run(
        request,
        AgentRunPaths {
            events_jsonl: temp.path().join("events.jsonl"),
            summary_json: temp.path().join("summary.json"),
        },
    )
    .expect_err("raw profile path rejected for jmcp");

    assert!(error.to_string().contains("raw profile paths"));
}
