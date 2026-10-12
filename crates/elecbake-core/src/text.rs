//! Plain-text views of a design.

use std::fmt::Write;

use crate::model::Design;

/// The netlist as text: a summary line, one line per net, then diagnostics.
pub fn netlist_text(design: &Design) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{}: {} sheets, {} components, {} nets\n",
        design.name,
        design.sheets.len(),
        design.components.len(),
        design.nets.len()
    );
    for net in &design.nets {
        let pins: Vec<String> = net.pins.iter().map(|p| format!("{}.{}", p.designator, p.number)).collect();
        let _ = writeln!(out, "{}: {}", net.name, pins.join(" "));
    }
    if !design.diagnostics.is_empty() {
        let _ = writeln!(out, "\nDiagnostics ({}):", design.diagnostics.len());
        for diagnostic in &design.diagnostics {
            let sheet = diagnostic.sheet.as_deref().map(|id| format!("{id}: ")).unwrap_or_default();
            let _ = writeln!(out, "  [{}] {sheet}{}", diagnostic.kind.as_str(), diagnostic.message);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::assemble;
    use crate::netlist::{NetScope, build};
    use crate::schematic::testkit::*;

    #[test]
    fn lists_nets_then_diagnostics() {
        let mut s = sheet("a.SchDoc");
        s.components = vec![component("R1", &[("1", (0, 0)), ("2", (0, 50))]), component("R2", &[("1", (10, 0))])];
        s.wires = vec![wire(&[(0, 0), (10, 0)])];
        s.power = vec![power("GND", (0, 50))];
        let sheets = [s];
        let design =
            assemble("demo".into(), "a.SchDoc".into(), &sheets, build(&sheets, NetScope::Automatic), Vec::new());

        assert_eq!(
            netlist_text(&design),
            "demo: 1 sheets, 2 components, 2 nets\n\
             \n\
             GND: R1.2\n\
             N$R1_1: R1.1 R2.1\n"
        );
    }

    #[test]
    fn diagnostics_name_their_kind_and_sheet() {
        let mut s = sheet("a.SchDoc");
        s.components = vec![component("R1", &[("1", (0, 0))])];
        let sheets = [s];
        let design =
            assemble("demo".into(), "a.SchDoc".into(), &sheets, build(&sheets, NetScope::Automatic), Vec::new());
        let text = netlist_text(&design);
        assert!(text.contains("\nDiagnostics (1):\n"), "{text}");
        assert!(
            text.contains("  [unconnected_pin] a: pin R1.1 is not connected and has no no-connect marker\n"),
            "{text}"
        );
    }
}
