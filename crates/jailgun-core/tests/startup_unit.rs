use jailgun_core::startup_unit::Definition;

#[test]
fn startup_definitions_reject_instruction_injection_and_relative_paths() {
    for executable in [
        "relative/bin/jailgun",
        "/bin/jailgun\nExecStart=/bin/false",
        "/bin/jailgun\0",
    ] {
        let definition = Definition {
            name: "jailgun".into(),
            executable: executable.into(),
            runtime: "/private/runtime".into(),
            arguments: vec![],
        };
        assert!(definition.systemd().is_err());
        assert!(definition.launchd().is_err());
    }
    for name in ["", "..", "-option", "other/unit", "job</string>", "job\n"] {
        let definition = Definition {
            name: name.into(),
            executable: "/bin/jailgun".into(),
            runtime: "/private/runtime".into(),
            arguments: vec![],
        };
        assert!(definition.systemd().is_err());
        assert!(definition.launchd().is_err());
    }
    let definition = Definition {
        name: "jailgun".into(),
        executable: "/bin/jailgun".into(),
        runtime: "/private/runtime".into(),
        arguments: vec!["serve\n--advanced".into()],
    };
    assert!(definition.systemd().is_err());
    assert!(definition.launchd().is_err());
}
