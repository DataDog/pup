//! Check the shipped executable, where `cfg(test)` is not enabled.
use std::fs;

#[test]
fn mock_server_override_matches_build_profile() {
    let executable = fs::read(env!("CARGO_BIN_EXE_pup")).expect("read pup executable");
    let variable = b"PUP_MOCK_SERVER";
    let contains_override = executable
        .windows(variable.len())
        .any(|window| window == variable);

    assert_eq!(
        contains_override,
        cfg!(all(debug_assertions, not(feature = "browser"))),
        "only non-browser debug executables should contain the mock-server environment lookup"
    );
}
