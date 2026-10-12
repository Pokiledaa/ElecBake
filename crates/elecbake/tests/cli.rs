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
fn fixture(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/altiumsharp")).join(name)
}

#[test]
fn netlist_prints_text_by_default() {
    let output = elecbake().arg("netlist").arg(fixture("Overview.SchDoc")).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.starts_with("Overview: 5 sheets, "), "{text}");
    assert!(text.lines().any(|line| line.starts_with("CS_3V3: ") && line.contains("J2.5")), "{text}");
}

#[test]
fn netlist_prints_the_design_model_as_json() {
    let output =
        elecbake().arg("netlist").arg(fixture("Overview.SchDoc")).args(["--format", "json"]).output().unwrap();
    assert!(output.status.success());
    let design: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(design["name"], "Overview");
    assert_eq!(design["sheets"].as_array().unwrap().len(), 5);
    assert!(design["nets"].as_array().unwrap().iter().any(|net| net["name"] == "GND"));
}

#[test]
fn render_writes_one_svg_per_sheet_file() {
    let dir = tempfile::tempdir().unwrap();
    let output =
        elecbake().arg("render").arg(fixture("Overview.SchDoc")).arg("-o").arg(dir.path()).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let mut written: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    written.sort();
    assert_eq!(written, ["DAC.svg", "Level Shifter.svg", "Overview.svg", "Power Supply.svg"]);
    let svg = std::fs::read_to_string(dir.path().join("Overview.svg")).unwrap();
    assert!(svg.starts_with("<svg"));
}

#[test]
fn a_file_that_cannot_be_read_fails_with_a_message() {
    let dir = tempfile::tempdir().unwrap();
    let broken = dir.path().join("broken.SchDoc");
    std::fs::write(&broken, b"nope").unwrap();
    for command in ["netlist", "render"] {
        let output = elecbake().arg(command).arg(&broken).output().unwrap();
        assert_eq!(output.status.code(), Some(1), "{command}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("broken.SchDoc"), "{command}");
        assert!(output.stdout.is_empty(), "{command}");
    }
}
