use jailgun_core::{validate_account_id, BrowserRegistryError};
use proptest::prelude::*;

fn valid_account_id() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![
            b'a'..=b'z',
            b'A'..=b'Z',
            b'0'..=b'9',
            Just(b'.'),
            Just(b'_'),
            Just(b'-'),
        ],
        1..=64,
    )
    .prop_map(|bytes| String::from_utf8(bytes).expect("ascii account id"))
    .prop_filter("dot-only path aliases are not valid ids", |id| {
        id != "." && id != ".."
    })
}

proptest! {
    #[test]
    fn account_ids_accept_only_safe_route_components(id in valid_account_id()) {
        prop_assert!(validate_account_id(&id).is_ok());
    }

    #[test]
    fn account_ids_reject_path_and_shell_separators(prefix in valid_account_id(), suffix in valid_account_id()) {
        for separator in ["/", "\\", ":", ";", " ", "\n", "\t", "="] {
            let candidate = format!("{prefix}{separator}{suffix}");
            prop_assert!(matches!(
                validate_account_id(&candidate),
                Err(BrowserRegistryError::InvalidAccountId(_))
            ));
        }
    }
}
