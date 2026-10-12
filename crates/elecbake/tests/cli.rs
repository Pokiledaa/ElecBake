use std::process::Command;

fn elecbake() -> Command {
    Command::new(env!("CARGO_BIN_EXE_elecbake"))
}

#[test]
fn prints_its_version() {
    let output = elecbake().arg("--version").output().unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "elecbake 0.1.0"
    );
}
fn fixture(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/altiumsharp"
    ))
    .join(name)
}

#[test]
fn netlist_prints_text_by_default() {
    let output = elecbake()
        .arg("netlist")
        .arg(fixture("Overview.SchDoc"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.starts_with("Overview: 5 sheets, "), "{text}");
    assert!(
        text.lines()
            .any(|line| line.starts_with("CS_3V3: ") && line.contains("J2.5")),
        "{text}"
    );
}

#[test]
fn netlist_prints_the_design_model_as_json() {
    let output = elecbake()
        .arg("netlist")
        .arg(fixture("Overview.SchDoc"))
        .args(["--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let design: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(design["name"], "Overview");
    assert_eq!(design["sheets"].as_array().unwrap().len(), 5);
    assert!(
        design["nets"]
            .as_array()
            .unwrap()
            .iter()
            .any(|net| net["name"] == "GND")
    );
}

#[test]
fn render_writes_one_svg_per_sheet_file() {
    let dir = tempfile::tempdir().unwrap();
    let output = elecbake()
        .arg("render")
        .arg(fixture("Overview.SchDoc"))
        .arg("-o")
        .arg(dir.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut written: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    written.sort();
    assert_eq!(
        written,
        [
            "DAC.svg",
            "Level Shifter.svg",
            "Overview.svg",
            "Power Supply.svg"
        ]
    );
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
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("broken.SchDoc"),
            "{command}"
        );
        assert!(output.stdout.is_empty(), "{command}");
    }
}
/// A reference netlist written from our own output, as "name, then designator-pin lines".
fn reference_from_our_output(schematic: &std::path::Path) -> String {
    let output = elecbake().arg("netlist").arg(schematic).output().unwrap();
    let mut reference = String::new();
    for line in String::from_utf8_lossy(&output.stdout).lines().skip(2) {
        if line.is_empty() {
            break;
        }
        let (name, pins) = line.split_once(": ").unwrap();
        reference.push_str(&format!("(\r\n{name}\r\n"));
        for pin in pins.split(' ') {
            let (designator, number) = pin.split_once('.').unwrap();
            reference.push_str(&format!("{designator}-{number}\r\n"));
        }
        reference.push_str(")\r\n");
    }
    reference
}

#[test]
fn compare_reports_a_match_and_exits_zero() {
    let dir = tempfile::tempdir().unwrap();
    let schematic = fixture("Level Shifter.SchDoc");
    let reference = dir.path().join("reference.NET");
    std::fs::write(&reference, reference_from_our_output(&schematic)).unwrap();

    let output = elecbake()
        .arg("netlist")
        .arg(&schematic)
        .arg("--compare")
        .arg(&reference)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{text}");
    assert!(text.starts_with("MATCH: "), "{text}");
}

#[test]
fn compare_reports_a_mismatch_and_exits_one() {
    let dir = tempfile::tempdir().unwrap();
    let schematic = fixture("Level Shifter.SchDoc");
    let reference = dir.path().join("reference.NET");
    std::fs::write(&reference, "(\r\nGND\r\nIC5-1\r\nZZ9-9\r\n)\r\n").unwrap();

    let output = elecbake()
        .arg("netlist")
        .arg(&schematic)
        .arg("--compare")
        .arg(&reference)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.starts_with("MISMATCH: "), "{text}");
    assert!(
        text.contains("only in reference: GND: IC5.1 ZZ9.9"),
        "{text}"
    );
}

#[test]
fn compare_reads_a_windows_1252_reference_file() {
    let dir = tempfile::tempdir().unwrap();
    let schematic = fixture("Level Shifter.SchDoc");
    let reference = dir.path().join("reference.NET");
    // 0xB1 is "±" in Windows-1252 and is not valid UTF-8 on its own.
    let mut bytes = b"(\r\n5V".to_vec();
    bytes.push(0xB1);
    bytes.extend_from_slice(b"\r\nIC5-1\r\nZZ9-9\r\n)\r\n");
    std::fs::write(&reference, bytes).unwrap();

    let output = elecbake()
        .arg("netlist")
        .arg(&schematic)
        .arg("--compare")
        .arg(&reference)
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("IC5.1 ZZ9.9"));
}
