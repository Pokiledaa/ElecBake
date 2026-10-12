use std::process::Command;

fn elecbake() -> Command {
    Command::new(env!("CARGO_BIN_EXE_elecbake"))
}

#[test]
fn prints_its_version() {
    let output = elecbake().arg("--version").output().unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "elecbake 0.1.0");
}
