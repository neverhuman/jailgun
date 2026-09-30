use jailgun_core::browser_registry::profile_ownership::{claim, MARKER};
use std::{fs, path::Path, sync::Barrier};

#[test]
fn competing_installations_cannot_replace_the_durable_profile_owner() {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/profile-owner-proofs");
    fs::create_dir_all(&parent).unwrap();
    let root = tempfile::tempdir_in(parent).unwrap();
    let profile = root.path().join("profile");
    fs::create_dir(&profile).unwrap();
    fs::write(
        profile.join("retained-profile-data"),
        b"synthetic profile data",
    )
    .unwrap();
    let barrier = Barrier::new(3);
    let owners = [
        root.path().join("runtime-one"),
        root.path().join("runtime-two"),
    ];
    let results = std::thread::scope(|scope| {
        let handles = owners
            .iter()
            .map(|owner| {
                let (barrier, profile) = (&barrier, &profile);
                scope.spawn(move || {
                    barrier.wait();
                    claim(profile, owner)
                })
            })
            .collect::<Vec<_>>();
        barrier.wait();
        handles
            .into_iter()
            .map(|task| task.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    let winner = results.iter().position(Result::is_ok).unwrap();
    assert_eq!(
        results[1 - winner].as_ref().unwrap_err().kind(),
        std::io::ErrorKind::AlreadyExists
    );
    assert_eq!(
        fs::read_to_string(profile.join(MARKER)).unwrap(),
        format!("{}\n", owners[winner].display())
    );
    assert!(claim(&profile, &owners[1 - winner]).is_err());
    claim(&profile, &owners[winner]).unwrap();
    assert_eq!(
        fs::read(profile.join("retained-profile-data")).unwrap(),
        b"synthetic profile data"
    );
    let names = fs::read_dir(&profile)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    assert_eq!(names.len(), 2, "pending ownership files remain");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(profile.join(MARKER))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

#[test]
fn same_installation_claims_are_idempotent_during_concurrent_registration() {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/profile-owner-proofs");
    fs::create_dir_all(&parent).unwrap();
    let root = tempfile::tempdir_in(parent).unwrap();
    let profile = root.path().join("profile");
    fs::create_dir(&profile).unwrap();
    let barrier = Barrier::new(8);
    std::thread::scope(|scope| {
        for _ in 0..8 {
            let (barrier, profile, runtime) = (&barrier, &profile, root.path());
            scope.spawn(move || {
                barrier.wait();
                claim(profile, runtime).unwrap();
            });
        }
    });
    assert_eq!(fs::read_dir(profile).unwrap().count(), 1);
}
