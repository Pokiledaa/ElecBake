//! Compares a derived netlist with one exported by Altium (Protel format).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use crate::model::{Design, Net};
use crate::netlist::net_key;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceNet {
    pub name: String,
    /// Pins as "R1.2".
    pub pins: BTreeSet<String>,
}

/// Parse the nets of a Protel-format netlist. Component blocks are skipped.
pub fn parse_protel(text: &str) -> Vec<ReferenceNet> {
    let mut nets = Vec::new();
    let mut current: Option<ReferenceNet> = None;
    let mut in_net = false;
    for line in text.lines().map(str::trim) {
        match line {
            "(" => {
                in_net = true;
                current = None;
            }
            ")" => {
                nets.extend(current.take());
                in_net = false;
            }
            "" => {}
            _ if in_net => match &mut current {
                None => {
                    current = Some(ReferenceNet {
                        name: line.to_string(),
                        pins: BTreeSet::new(),
                    })
                }
                Some(net) => {
                    let pin = match line.split_once('-') {
                        Some((designator, number)) => format!("{designator}.{number}"),
                        None => line.to_string(),
                    };
                    net.pins.insert(pin);
                }
            },
            _ => {}
        }
    }
    nets
}

#[derive(Debug, Clone, Default)]
pub struct Comparison {
    pub matched: usize,
    /// (name in the reference, name in the design) for nets with the same pins but different names.
    pub renamed: Vec<(String, String)>,
    pub only_in_reference: Vec<String>,
    pub only_in_design: Vec<String>,
}

impl Comparison {
    pub fn is_match(&self) -> bool {
        self.renamed.is_empty()
            && self.only_in_reference.is_empty()
            && self.only_in_design.is_empty()
    }

    pub fn report(&self) -> String {
        let mut out = String::new();
        if self.is_match() {
            let _ = writeln!(
                out,
                "MATCH: {} nets have the same pins and names as the reference",
                self.matched
            );
            return out;
        }
        let _ = writeln!(
            out,
            "MISMATCH: {} nets match, {} named differently, {} only in the reference, {} only in the design",
            self.matched,
            self.renamed.len(),
            self.only_in_reference.len(),
            self.only_in_design.len()
        );
        for (reference, design) in &self.renamed {
            let _ = writeln!(out, "  name: reference '{reference}', design '{design}'");
        }
        for net in &self.only_in_reference {
            let _ = writeln!(out, "  only in reference: {net}");
        }
        for net in &self.only_in_design {
            let _ = writeln!(out, "  only in design:    {net}");
        }
        out
    }
}

fn describe(name: &str, pins: &BTreeSet<String>) -> String {
    format!(
        "{name}: {}",
        pins.iter().cloned().collect::<Vec<_>>().join(" ")
    )
}

pub fn compare(design: &Design, reference: &[ReferenceNet]) -> Comparison {
    let mut ours: BTreeMap<BTreeSet<String>, &Net> = design
        .nets
        .iter()
        .map(|net| {
            (
                net.pins
                    .iter()
                    .map(|p| format!("{}.{}", p.designator, p.number))
                    .collect(),
                net,
            )
        })
        .collect();

    let mut result = Comparison::default();
    for theirs in reference {
        match ours.remove(&theirs.pins) {
            Some(net) => {
                result.matched += 1;
                let altium_generated = theirs.name.starts_with("Net");
                if !altium_generated && net_key(&net.name) != net_key(&theirs.name) {
                    result.renamed.push((theirs.name.clone(), net.name.clone()));
                }
            }
            None => result
                .only_in_reference
                .push(describe(&theirs.name, &theirs.pins)),
        }
    }
    for (pins, net) in ours {
        // Altium leaves single-pin nets out of the export by default.
        if pins.len() > 1 {
            result.only_in_design.push(describe(&net.name, &pins));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::assemble;
    use crate::netlist::{NetScope, build};
    use crate::schematic::testkit::*;

    const REFERENCE: &str = "[\r\nR1\r\n0603\r\n10k\r\n\r\n\r\n\r\n]\r\n\
        (\r\nGND\r\nR1-2\r\nR2-2\r\n)\r\n\
        (\r\nNetR1_1\r\nR1-1\r\nR2-1\r\n)\r\n";

    /// R1 and R2 in parallel: pins 1 joined by a wire, pins 2 on GND, plus a lone pin R3.1.
    fn design() -> Design {
        let mut s = sheet("a.SchDoc");
        s.components = vec![
            component("R1", &[("1", (0, 0)), ("2", (0, 50))]),
            component("R2", &[("1", (10, 0)), ("2", (10, 50))]),
            component("R3", &[("1", (90, 90))]),
        ];
        s.wires = vec![wire(&[(0, 0), (10, 0)])];
        s.power = vec![power("GND", (0, 50)), power("GND", (10, 50))];
        let sheets = [s];
        assemble(
            "demo".into(),
            "a.SchDoc".into(),
            &sheets,
            build(&sheets, NetScope::Automatic),
            Vec::new(),
        )
    }

    #[test]
    fn parses_nets_and_skips_component_blocks() {
        let nets = parse_protel(REFERENCE);
        assert_eq!(nets.len(), 2);
        assert_eq!(nets[0].name, "GND");
        assert_eq!(nets[0].pins.iter().collect::<Vec<_>>(), ["R1.2", "R2.2"]);
        assert_eq!(nets[1].name, "NetR1_1");
    }

    #[test]
    fn a_pin_number_may_itself_contain_a_dash() {
        let nets = parse_protel("(\nVBUS\nJ1-A4-B9\n)\n");
        assert_eq!(nets[0].pins.iter().collect::<Vec<_>>(), ["J1.A4-B9"]);
    }

    #[test]
    fn matching_netlists_match_and_lone_pins_are_ignored() {
        let result = compare(&design(), &parse_protel(REFERENCE));
        assert!(result.is_match(), "{}", result.report());
        assert_eq!(result.matched, 2);
        assert!(result.report().starts_with("MATCH: 2 nets"));
    }

    #[test]
    fn a_net_with_different_pins_shows_on_both_sides() {
        let reference = parse_protel("(\nGND\nR1-2\nR2-2\nR9-1\n)\n(\nNetR1_1\nR1-1\nR2-1\n)\n");
        let result = compare(&design(), &reference);
        assert!(!result.is_match());
        assert_eq!(result.only_in_reference, ["GND: R1.2 R2.2 R9.1"]);
        assert_eq!(result.only_in_design, ["GND: R1.2 R2.2"]);
        assert!(result.report().starts_with("MISMATCH"));
    }

    #[test]
    fn a_named_net_must_keep_its_name() {
        let reference = parse_protel("(\nAGND\nR1-2\nR2-2\n)\n(\nNetR1_1\nR1-1\nR2-1\n)\n");
        let result = compare(&design(), &reference);
        assert_eq!(result.renamed, [("AGND".to_string(), "GND".to_string())]);
        assert!(!result.is_match());
    }

    #[test]
    fn names_are_compared_ignoring_case() {
        let reference = parse_protel("(\ngnd\nR1-2\nR2-2\n)\n(\nNetR1_1\nR1-1\nR2-1\n)\n");
        assert!(compare(&design(), &reference).is_match());
    }
}
