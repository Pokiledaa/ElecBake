//! End-to-end tests on the AltiumSharp sample design.

use std::path::PathBuf;

use elecbake_core::diagnostic::DiagnosticKind;
use elecbake_core::netlist::NetScope;
use elecbake_core::{Design, load_design};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/altiumsharp"
    ))
    .join(name)
}

fn pins(design: &Design, net: &str) -> Vec<String> {
    design
        .net(net)
        .unwrap_or_else(|| panic!("no net {net}"))
        .pins
        .iter()
        .map(|p| format!("{}.{}", p.designator, p.number))
        .collect()
}

#[test]
fn a_top_sheet_pulls_in_its_child_sheets() {
    let design = load_design(&fixture("Overview.SchDoc")).unwrap();
    assert_eq!(design.name, "Overview");
    assert_eq!(design.scope, NetScope::Hierarchical);
    let mut ids: Vec<&str> = design.sheets.iter().map(|s| s.id.as_str()).collect();
    ids.sort();
    assert_eq!(
        ids,
        [
            "Overview",
            "Overview/DAC",
            "Overview/LDAC_SHIFT",
            "Overview/PSU",
            "Overview/SPI_SHIFT"
        ]
    );
    assert!(
        !design.diagnostics.iter().any(|d| matches!(
            d.kind,
            DiagnosticKind::SheetLoadFailed
                | DiagnosticKind::MissingSheet
                | DiagnosticKind::HierarchyCycle
        )),
        "{:#?}",
        design.diagnostics
    );
}

#[test]
fn a_signal_is_followed_from_the_top_sheet_into_a_child_sheet() {
    let design = load_design(&fixture("Overview.SchDoc")).unwrap();
    let cs = pins(&design, "CS_3V3");
    assert!(cs.contains(&"J2.5".to_string()), "{cs:?}");
    assert!(cs.contains(&"IC5_SPI_SHIFT.2".to_string()), "{cs:?}");
    assert!(!cs.contains(&"IC5_LDAC_SHIFT.2".to_string()), "{cs:?}");
}

#[test]
fn a_sheet_used_twice_gives_two_sets_of_components() {
    let design = load_design(&fixture("Overview.SchDoc")).unwrap();
    assert!(design.component("IC5_SPI_SHIFT").is_some());
    assert!(design.component("IC5_LDAC_SHIFT").is_some());
    assert!(design.component("IC5").is_none());
    assert_eq!(
        design.component("IC5_SPI_SHIFT").unwrap().value.as_deref(),
        Some("TXU0304PWR")
    );
    assert!(
        design
            .diagnostics
            .iter()
            .any(|d| d.kind == DiagnosticKind::RepeatedSheet)
    );
}

#[test]
fn ground_spans_every_sheet_that_uses_it() {
    let design = load_design(&fixture("Overview.SchDoc")).unwrap();
    let gnd = design.net("GND").unwrap();
    assert!(gnd.sheets.len() >= 3, "{:?}", gnd.sheets);
    assert!(gnd.pins.len() >= 10, "{}", gnd.pins.len());
}

#[test]
fn every_pin_belongs_to_a_named_net() {
    let design = load_design(&fixture("Overview.SchDoc")).unwrap();
    for component in &design.components {
        for pin in &component.pins {
            assert!(
                design.net(&pin.net).is_some(),
                "{}.{} -> '{}'",
                component.designator,
                pin.number,
                pin.net
            );
        }
    }
}

#[test]
fn every_standalone_sample_sheet_loads_as_a_design() {
    for name in [
        "Fanout Buffer",
        "Power Supplies",
        "SPI Isolator",
        "USB Power",
    ] {
        let design = load_design(&fixture(&format!("{name}.SchDoc")))
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(!design.components.is_empty(), "{name}");
        assert!(!design.nets.is_empty(), "{name}");
    }
}

#[test]
fn a_project_file_loads_its_schematics_and_reports_missing_ones() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["Overview", "DAC", "Power Supply", "Level Shifter"] {
        std::fs::copy(
            fixture(&format!("{name}.SchDoc")),
            dir.path().join(format!("{name}.SchDoc")),
        )
        .unwrap();
    }
    let project = dir.path().join("Board.PrjPcb");
    std::fs::write(
        &project,
        "[Design]\r\nVersion=1.0\r\nHierarchyMode=0\r\n\r\n\
         [Document1]\r\nDocumentPath=overview.schdoc\r\n\r\n\
         [Document2]\r\nDocumentPath=Extra.SchDoc\r\n",
    )
    .unwrap();

    let design = load_design(&project).unwrap();
    assert_eq!(design.name, "Board");
    assert_eq!(design.sheets.len(), 5);
    let failed: Vec<&str> = design
        .diagnostics
        .iter()
        .filter(|d| d.kind == DiagnosticKind::SheetLoadFailed)
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(failed.len(), 1);
    assert!(failed[0].contains("Extra.SchDoc"), "{failed:?}");
}

#[test]
fn a_broken_child_sheet_does_not_stop_the_rest_from_loading() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["Overview", "DAC", "Power Supply"] {
        std::fs::copy(
            fixture(&format!("{name}.SchDoc")),
            dir.path().join(format!("{name}.SchDoc")),
        )
        .unwrap();
    }
    std::fs::write(
        dir.path().join("Level Shifter.SchDoc"),
        b"not an Altium file",
    )
    .unwrap();

    let design = load_design(&dir.path().join("Overview.SchDoc")).unwrap();
    assert!(design.component("J2").is_some());
    assert!(
        design
            .diagnostics
            .iter()
            .any(|d| d.kind == DiagnosticKind::SheetLoadFailed)
    );
    assert!(
        design
            .diagnostics
            .iter()
            .any(|d| d.kind == DiagnosticKind::MissingSheet)
    );
}

#[test]
fn unusable_inputs_are_errors() {
    let dir = tempfile::tempdir().unwrap();
    let broken = dir.path().join("broken.SchDoc");
    std::fs::write(&broken, b"nope").unwrap();
    assert!(load_design(&broken).is_err());
    assert!(load_design(&dir.path().join("notes.txt")).is_err());
    assert!(load_design(&dir.path().join("missing.SchDoc")).is_err());
}

#[test]
fn an_unreadable_project_option_becomes_a_diagnostic() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::copy(
        fixture("Level Shifter.SchDoc"),
        dir.path().join("Level Shifter.SchDoc"),
    )
    .unwrap();
    let project = dir.path().join("Board.PrjPcb");
    std::fs::write(
        &project,
        "[Design]\r\nVersion=1.0\r\nHierarchyMode=7\r\n\r\n\
         [Document1]\r\nDocumentPath=Level Shifter.SchDoc\r\n",
    )
    .unwrap();

    let design = load_design(&project).unwrap();
    assert_eq!(design.components.len(), 5);
    assert!(
        design
            .diagnostics
            .iter()
            .any(|d| d.kind == DiagnosticKind::ProjectOptionsIgnored)
    );
}
