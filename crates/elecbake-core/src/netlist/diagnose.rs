//! Findings derived from the finished netlist.

use std::collections::BTreeMap;

use super::build::{NetEntry, SheetInstance};
use super::sheet::SheetNets;
use crate::diagnostic::{Diagnostic, DiagnosticKind};
use crate::schematic::RawSheet;

pub(super) fn diagnose(
    sheets: &[RawSheet],
    local: &[SheetNets],
    instances: &[SheetInstance],
    nets: &[NetEntry],
    out: &mut Vec<Diagnostic>,
) {
    // Findings about a sheet itself are reported once, on its first instance.
    let mut first: Vec<Option<usize>> = vec![None; sheets.len()];
    let mut uses = vec![0usize; sheets.len()];
    for (i, instance) in instances.iter().enumerate() {
        first[instance.sheet].get_or_insert(i);
        uses[instance.sheet] += 1;
    }

    for (s, sheet) in sheets.iter().enumerate() {
        let Some(i) = first[s] else { continue };
        let id = &instances[i].id;
        for &end in &local[s].dangling {
            out.push(
                Diagnostic::new(DiagnosticKind::DanglingWire, "wire end is not connected to anything")
                    .on_sheet(id)
                    .at(end),
            );
        }
        if sheet.bus_count > 0 || sheet.harness_count > 0 {
            out.push(
                Diagnostic::new(
                    DiagnosticKind::BusNotExpanded,
                    format!(
                        "sheet has {} bus and {} harness object(s); connections made through them are not followed",
                        sheet.bus_count, sheet.harness_count
                    ),
                )
                .on_sheet(id),
            );
        }
        if uses[s] > 1 {
            out.push(
                Diagnostic::new(
                    DiagnosticKind::RepeatedSheet,
                    format!(
                        "sheet '{}' is used {} times; its designators get the sheet symbol name as a suffix, \
                         which may differ from Altium's annotation",
                        sheet.path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default(),
                        uses[s]
                    ),
                )
                .on_sheet(id),
            );
        }
    }

    for net in nets {
        if net.touches_bus {
            out.push(
                Diagnostic::new(
                    DiagnosticKind::BusNotExpanded,
                    format!("net '{}' touches a bus entry; its bus connections are not followed", net.name),
                )
                .net(&net.name),
            );
        }
        match net.pins.as_slice() {
            [pin] if net.generated && !net.no_connect => {
                let instance = &instances[pin.instance];
                let at = sheets[instance.sheet].components[pin.component].pins[pin.pin].hot;
                out.push(
                    Diagnostic::new(
                        DiagnosticKind::UnconnectedPin,
                        format!(
                            "pin {}.{} is not connected and has no no-connect marker",
                            pin.designator, pin.number
                        ),
                    )
                    .on_sheet(&instance.id)
                    .at(at)
                    .designator(&pin.designator),
                );
            }
            _ => {}
        }
    }

    // designator -> (part number, instance) of every placement
    let mut placements: BTreeMap<String, Vec<(i32, usize)>> = BTreeMap::new();
    for (i, instance) in instances.iter().enumerate() {
        for component in &sheets[instance.sheet].components {
            placements
                .entry(format!("{}{}", component.designator, instance.suffix))
                .or_default()
                .push((component.part_id, i));
        }
    }
    for (designator, placed) in placements {
        let mut parts: Vec<i32> = placed.iter().map(|&(part, _)| part).collect();
        parts.sort();
        parts.dedup();
        if parts.len() == placed.len() {
            continue;
        }
        let message = if designator.contains('?') {
            format!(
                "{} components share the unannotated designator '{designator}'; \
                 annotate the design so each has its own",
                placed.len()
            )
        } else {
            format!("designator '{designator}' is used by {} components", placed.len())
        };
        out.push(
            Diagnostic::new(DiagnosticKind::DuplicateDesignator, message)
                .on_sheet(&instances[placed[0].1].id)
                .designator(&designator),
        );
    }
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::{Diagnostic, DiagnosticKind};
    use crate::netlist::{NetScope, build};
    use crate::schematic::testkit::*;

    fn of_kind(diagnostics: &[Diagnostic], kind: DiagnosticKind) -> Vec<&Diagnostic> {
        diagnostics.iter().filter(|d| d.kind == kind).collect()
    }

    #[test]
    fn a_lone_pin_without_a_no_connect_marker_is_reported() {
        let mut s = sheet("a.SchDoc");
        s.components = vec![component("U1", &[("1", (0, 0)), ("2", (0, 10))])];
        s.no_connects = vec![pt(0, 10)];
        let netlist = build(&[s], NetScope::Automatic);
        let found = of_kind(&netlist.diagnostics, DiagnosticKind::UnconnectedPin);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].designators, ["U1"]);
        assert_eq!(found[0].at, Some(pt(0, 0)));
        assert_eq!(found[0].sheet.as_deref(), Some("a"));
        assert!(found[0].message.contains("U1.1"));
    }

    #[test]
    fn a_lone_pin_on_a_named_net_is_not_reported() {
        let mut s = sheet("a.SchDoc");
        s.components = vec![component("U1", &[("1", (0, 0))])];
        s.power = vec![power("GND", (0, 0))];
        let netlist = build(&[s], NetScope::Automatic);
        assert!(of_kind(&netlist.diagnostics, DiagnosticKind::UnconnectedPin).is_empty());
    }

    #[test]
    fn a_dangling_wire_end_is_reported_with_its_position() {
        let mut s = sheet("a.SchDoc");
        s.components = vec![component("R1", &[("1", (0, 0))]), component("R2", &[("1", (5, 0))])];
        s.wires = vec![wire(&[(0, 0), (10, 0)])];
        let netlist = build(&[s], NetScope::Automatic);
        let found = of_kind(&netlist.diagnostics, DiagnosticKind::DanglingWire);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].at, Some(pt(10, 0)));
    }

    #[test]
    fn buses_and_harnesses_are_reported_per_sheet_and_per_net() {
        let mut s = sheet("a.SchDoc");
        s.components = vec![component("R1", &[("1", (0, 0))]), component("R2", &[("1", (5, 0))])];
        s.wires = vec![wire(&[(0, 0), (10, 0)])];
        s.bus_entries = vec![[pt(10, 0), pt(12, 2)]];
        s.bus_count = 1;
        let netlist = build(&[s], NetScope::Automatic);
        let found = of_kind(&netlist.diagnostics, DiagnosticKind::BusNotExpanded);
        assert_eq!(found.len(), 2);
        assert!(found.iter().any(|d| d.nets == ["N$R1_1"]));
        assert!(found.iter().any(|d| d.sheet.as_deref() == Some("a") && d.nets.is_empty()));
    }

    #[test]
    fn the_same_designator_twice_is_reported() {
        let mut s = sheet("a.SchDoc");
        s.components = vec![component("R1", &[("1", (0, 0))]), component("R1", &[("1", (50, 0))])];
        let netlist = build(&[s], NetScope::Automatic);
        let found = of_kind(&netlist.diagnostics, DiagnosticKind::DuplicateDesignator);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].designators, ["R1"]);
    }

    #[test]
    fn parts_of_one_multi_part_component_are_not_duplicates() {
        let mut s = sheet("a.SchDoc");
        s.components = vec![component("U1", &[("1", (0, 0))]), component("U1", &[("5", (50, 0))])];
        s.components[1].part_id = 2;
        let netlist = build(&[s], NetScope::Automatic);
        assert!(of_kind(&netlist.diagnostics, DiagnosticKind::DuplicateDesignator).is_empty());
    }

    #[test]
    fn an_unannotated_design_loads_and_asks_for_annotation() {
        let mut s = sheet("a.SchDoc");
        s.components = vec![
            component("R?", &[("1", (0, 0))]),
            component("R?", &[("1", (50, 0))]),
            component("R?", &[("1", (90, 0))]),
        ];
        let netlist = build(&[s], NetScope::Automatic);
        let found = of_kind(&netlist.diagnostics, DiagnosticKind::DuplicateDesignator);
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("annotate"), "{}", found[0].message);
        assert!(found[0].message.contains('3'));
    }

    #[test]
    fn a_repeated_sheet_is_reported_once_and_is_not_a_duplicate() {
        let mut parent = sheet("top.SchDoc");
        parent.symbols = vec![symbol("A", "child.SchDoc", &[]), symbol("B", "child.SchDoc", &[])];
        let mut child = sheet("child.SchDoc");
        child.components = vec![component("R1", &[("1", (0, 0))])];
        child.power = vec![power("GND", (0, 0))];
        let netlist = build(&[parent, child], NetScope::Automatic);
        assert_eq!(of_kind(&netlist.diagnostics, DiagnosticKind::RepeatedSheet).len(), 1);
        assert!(of_kind(&netlist.diagnostics, DiagnosticKind::DuplicateDesignator).is_empty());
    }
}
